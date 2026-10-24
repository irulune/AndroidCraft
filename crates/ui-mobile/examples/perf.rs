//! Headless benchmark: how long a UI frame takes, and which engine commands it issues.
//!
//! `cargo run --release -p effectia-ui-mobile --example perf -- [width] [height] [ppp]`

use std::time::{Duration, Instant};

use effectia_ui_mobile::editor::Editor;
use effectia_ui_mobile::ui::{MobileApp, apply_theme};
use egui::{Event, Pos2, RawInput, Rect, vec2};

fn frame(ctx: &egui::Context, app: &mut MobileApp, w: f32, h: f32, ppp: f32, events: Vec<Event>) -> Duration {
    static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tick = CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(w, h))),
        viewports: std::iter::once((egui::ViewportId::ROOT, egui::ViewportInfo { native_pixels_per_point: Some(ppp), ..Default::default() })).collect(),
        events,
        // A steady 60 Hz clock, so playback advances by a real frame's worth each time.
        time: Some(tick as f64 / 60.0),
        ..Default::default()
    };
    let t = Instant::now();
    let out = ctx.run_ui(raw, |ui| app.show(ui));
    // Tessellation happens on the GPU thread in the real app, but it is a cost per frame too.
    let _ = ctx.tessellate(out.shapes, out.pixels_per_point);
    t.elapsed()
}

fn report(name: &str, app: &mut MobileApp, times: &mut Vec<Duration>) {
    times.sort();
    let n = times.len().max(1);
    let total: Duration = times.iter().sum();
    println!(
        "{name}: {n} frames, mean {:.2} ms, median {:.2} ms, p95 {:.2} ms, worst {:.2} ms",
        total.as_secs_f64() * 1e3 / n as f64,
        times[n / 2].as_secs_f64() * 1e3,
        times[(n * 95 / 100).min(n - 1)].as_secs_f64() * 1e3,
        times[n - 1].as_secs_f64() * 1e3
    );
    for (cmd, calls, spent) in app.editor.take_profile().into_iter().take(6) {
        println!("    {cmd:<22} {:>6.1} calls/frame {:>8.3} ms/frame", calls as f64 / n as f64, spent.as_secs_f64() * 1e3 / n as f64);
    }
    times.clear();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let w: f32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(412.0);
    let h: f32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(892.0);
    let ppp: f32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(2.6);
    println!("viewport {w}x{h} @ {ppp}x");

    let ctx = egui::Context::default();
    apply_theme(&ctx);
    let mut editor = Editor::new(effectcraft_host::session());
    editor.open_demo().ok();
    let mut app = MobileApp::new(editor);
    if w >= 600.0 && h >= 500.0 {
        app.layout.inspector_open = true;
    }
    for _ in 0..4 {
        frame(&ctx, &mut app, w, h, ppp, vec![]);
    }
    app.editor.take_profile();
    let mut times = Vec::new();

    // What one frame costs the engine, drawn in full and as a draft.
    let comp = app.editor.comp().ok().flatten();
    let full = ((w.max(h) * ppp) as u32).clamp(64, 1920);
    let src = app.editor.frame_renderer();
    if let Some(c) = comp {
        for (name, side, draft) in [("full", full, false), ("draft", full / 2, true)] {
            let mut ts = Vec::new();
            for i in 0..10 {
                let t = Instant::now();
                let _ = src.render_rgba8(
                    effectcraft_engine::project::ItemId(c.id),
                    effectcraft_engine::time::Tick::from_seconds_f64(c.duration * f64::from(i) / 10.0),
                    side,
                    draft,
                );
                ts.push(t.elapsed());
            }
            ts.sort();
            println!("engine {name} frame ({side} px): median {:.1} ms, worst {:.1} ms", ts[5].as_secs_f64() * 1e3, ts[9].as_secs_f64() * 1e3);
        }
    }

    // A still screen: nothing changes, so nothing should cost.
    for _ in 0..60 {
        times.push(frame(&ctx, &mut app, w, h, ppp, vec![]));
    }
    report("idle", &mut app, &mut times);

    // Scrubbing: the time moves every frame.
    let dur = app.editor.comp().ok().flatten().map(|c| c.duration).unwrap_or(5.0);
    app.editor.take_profile();
    for i in 0..60 {
        app.editor.set_time(dur * (i as f64 / 60.0)).ok();
        times.push(frame(&ctx, &mut app, w, h, ppp, vec![]));
    }
    report("scrub", &mut app, &mut times);

    // Playing.
    app.playing = true;
    app.editor.take_profile();
    for _ in 0..60 {
        times.push(frame(&ctx, &mut app, w, h, ppp, vec![]));
    }
    report("play", &mut app, &mut times);
    app.playing = false;
}
