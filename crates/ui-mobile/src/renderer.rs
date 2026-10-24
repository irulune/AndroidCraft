//! Renders viewer frames on a worker thread, so a heavy frame never stalls touches or scrolling.
//!
//! The UI asks for the frame it wants; the worker renders the newest request and skips any that
//! were superseded while it was busy. Until the new frame arrives the UI keeps showing the last
//! one, so scrubbing stays smooth however slow the composition is.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use effectcraft_engine::Session;
use effectcraft_engine::project::{ItemId, Project};
use effectcraft_engine::render::{ExprHost, FootageSource, LayerCache, RenderOpts, Renderer};
use effectcraft_engine::time::Tick;

use crate::editor::Frame;

/// A frozen copy of what rendering reads, so frames can be drawn on another thread while the session
/// keeps changing. It sees the project as it was when it was made; take a new one after an edit.
/// The project is shared, not copied, until the next edit. CPU compositor only.
pub struct FrameRenderer {
    project: Arc<Project>,
    footage: Arc<dyn FootageSource>,
    expr: Option<Arc<dyn ExprHost>>,
    cache: Arc<LayerCache>,
    nested_switches: bool,
    draft_shadows: bool,
}

impl FrameRenderer {
    /// Takes the copy from a session.
    pub fn of(session: &Session) -> FrameRenderer {
        FrameRenderer {
            project: session.project.clone(),
            footage: session.footage.clone(),
            expr: session.expr.clone(),
            cache: session.layer_cache.clone(),
            nested_switches: session.prefs.general.switches_affect_nested_comps,
            draft_shadows: session.prefs.three_d.realtime_shadows,
        }
    }

    /// One frame of `comp` at `t` as opaque RGBA8 over the comp background, its longest side at most
    /// `max_side` pixels (0 = full size). `draft` trades quality for speed (fewer motion-blur
    /// samples, bilinear sampling), for frames shown while something is moving.
    pub fn render_rgba8(&self, comp: ItemId, t: Tick, max_side: u32, draft: bool) -> Option<(u32, u32, Vec<u8>)> {
        let c = self.project.comp(comp)?;
        let long = c.width.max(c.height).max(1) as f64;
        let scale = if max_side == 0 { 1.0 } else { (max_side as f64 / long).min(1.0) };
        let opts = RenderOpts { scale, draft, nested_switches: self.nested_switches, draft_shadows: self.draft_shadows, ..Default::default() };
        let mut r = Renderer::new(&self.project, self.footage.as_ref(), opts);
        r.expr = self.expr.as_deref();
        r.cache = Some(&self.cache);
        let img = r.comp_frame(comp, t);
        Some((img.width, img.height, img.to_rgba8_over(c.background)))
    }
}

/// What a frame was rendered for; the UI compares it with what it wants now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub comp: u64,
    /// Time in milliseconds.
    pub time: i64,
    pub revision: u64,
    pub side: u32,
    pub draft: bool,
}

struct Request {
    stamp: Stamp,
    seconds: f64,
    source: Arc<FrameRenderer>,
}

/// A finished request; `frame` is `None` when the render failed.
pub struct Done {
    pub stamp: Stamp,
    pub frame: Option<Frame>,
    /// How long the frame took to draw.
    pub took: std::time::Duration,
}

/// The worker thread and the channels to it.
pub struct RenderWorker {
    requests: Sender<Request>,
    done: Receiver<Done>,
    pending: Arc<AtomicUsize>,
}

fn render(r: Request) -> Done {
    let Request { stamp, seconds, source } = r;
    let started = std::time::Instant::now();
    // A panic in a render must not take the app (or the worker) down; the frame is just missed.
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        source.render_rgba8(ItemId(stamp.comp), Tick::from_seconds_f64(seconds), stamp.side, stamp.draft)
    }));
    let frame = out.ok().flatten().map(|(width, height, rgba)| Frame { width, height, rgba });
    Done { stamp, frame, took: started.elapsed() }
}

impl RenderWorker {
    /// Starts the worker; `wake` is called whenever a frame is ready (to repaint the screen).
    /// `None` when the thread cannot be started, in which case the caller renders inline.
    pub fn start(wake: impl Fn() + Send + 'static) -> Option<RenderWorker> {
        let (requests, inbox) = channel::<Request>();
        let (outbox, done) = channel::<Done>();
        let pending = Arc::new(AtomicUsize::new(0));
        let count = pending.clone();
        std::thread::Builder::new()
            .name("effectia-render".into())
            .spawn(move || {
                while let Ok(mut req) = inbox.recv() {
                    let mut taken = 1;
                    while let Ok(newer) = inbox.try_recv() {
                        req = newer;
                        taken += 1;
                    }
                    let result = render(req);
                    let sent = outbox.send(result).is_ok();
                    count.fetch_sub(taken, Ordering::SeqCst);
                    if !sent {
                        break;
                    }
                    wake();
                }
            })
            .ok()?;
        Some(RenderWorker { requests, done, pending })
    }

    /// Asks for a frame. Returns false when the worker is gone.
    pub fn submit(&self, stamp: Stamp, seconds: f64, source: Arc<FrameRenderer>) -> bool {
        self.pending.fetch_add(1, Ordering::SeqCst);
        if self.requests.send(Request { stamp, seconds, source }).is_err() {
            self.pending.fetch_sub(1, Ordering::SeqCst);
            return false;
        }
        true
    }

    /// The newest finished frame, if any arrived since the last call.
    pub fn latest(&self) -> Option<Done> {
        let mut last = None;
        while let Ok(d) = self.done.try_recv() {
            last = Some(d);
        }
        last
    }

    /// Whether a request is still being rendered or waiting.
    pub fn busy(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;

    fn demo() -> (Editor, u64) {
        let mut e = Editor::new(effectcraft_host::session());
        e.open_demo().unwrap();
        let comp = e.comp().unwrap().unwrap().id;
        (e, comp)
    }

    fn wait(w: &RenderWorker) -> Done {
        for _ in 0..2000 {
            if let Some(d) = w.latest() {
                return d;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("no frame arrived");
    }

    fn stamp(comp: u64, side: u32, draft: bool) -> Stamp {
        Stamp { comp, time: 0, revision: 0, side, draft }
    }

    #[test]
    fn a_frame_comes_back_matching_the_inline_render() {
        let (mut e, comp) = demo();
        e.set_time(0.0).unwrap();
        let w = RenderWorker::start(|| {}).unwrap();
        assert!(w.submit(stamp(comp, 200, false), 0.0, e.frame_renderer()));
        let d = wait(&w);
        let f = d.frame.unwrap();
        assert_eq!(d.stamp, stamp(comp, 200, false));
        let inline = e.render(200).unwrap();
        assert_eq!((f.width, f.height), (inline.width, inline.height));
        assert_eq!(f.rgba, inline.rgba, "the worker draws exactly what the engine draws");
    }

    #[test]
    fn a_superseded_request_is_skipped_and_busy_clears() {
        let (e, comp) = demo();
        let w = RenderWorker::start(|| {}).unwrap();
        let src = e.frame_renderer();
        for t in 0..8 {
            w.submit(Stamp { time: t, ..stamp(comp, 400, false) }, t as f64 / 10.0, src.clone());
        }
        // The last request is always the one that ends up shown.
        let mut last = wait(&w);
        for _ in 0..200 {
            if !w.busy() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            if let Some(d) = w.latest() {
                last = d;
            }
        }
        assert!(!w.busy());
        assert_eq!(last.stamp.time, 7);
    }

    #[test]
    fn an_unknown_comp_yields_no_frame_not_a_crash() {
        let (e, _) = demo();
        let w = RenderWorker::start(|| {}).unwrap();
        w.submit(stamp(9_999_999, 100, true), 0.0, e.frame_renderer());
        assert!(wait(&w).frame.is_none());
    }

    #[test]
    fn a_frame_keeps_its_project_as_it_was_made() {
        let (mut e, comp) = demo();
        let src = e.frame_renderer();
        let before = src.render_rgba8(ItemId(comp), Tick::from_seconds_f64(0.5), 160, false).unwrap().2;
        e.add_layer(crate::ops::NewLayer::Solid).unwrap();
        let after = src.render_rgba8(ItemId(comp), Tick::from_seconds_f64(0.5), 160, false).unwrap().2;
        assert_eq!(before, after, "an edit after the copy was made does not reach it");
        let now = e.render(160).map(|f| f.rgba).unwrap();
        assert_ne!(before, now, "while a fresh render does show the edit");
    }
}
