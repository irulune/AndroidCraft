//! The media picker: photos, videos, audio and other importable files from the device, shown as a
//! grid of thumbnails you tick, then add to the open composition as layers.
//!
//! * **Recent** lists the newest photos, videos and audio from the shared folders the platform offers.
//! * **Folders** browses those folders (and, with "all files access", the whole storage).
//! * Reading shared storage needs a permission on Android. The platform ([`crate::platform`])
//!   answers whether it is granted and asks for it; until then the picker explains why and offers
//!   the button, and polls so it fills in the moment the permission arrives.
//!
//! Folder scans and thumbnails run on worker threads, so scrolling never waits on storage.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use super::*;
use crate::browser::{self, File, Folder, Kind, Listing, Thumb, Thumbs};
use crate::platform::{Access, Needs, Root};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Recent,
    Folders,
}

/// Which kinds of file the grid shows.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Filter {
    #[default]
    All,
    Photos,
    Videos,
    Audio,
}

impl Filter {
    const ALL: [(Filter, &'static str); 4] = [(Filter::All, "All"), (Filter::Photos, "Photos"), (Filter::Videos, "Videos"), (Filter::Audio, "Audio")];

    fn keeps(self, kind: Kind) -> bool {
        matches!((self, kind), (Filter::All, _) | (Filter::Photos, Kind::Image) | (Filter::Videos, Kind::Video) | (Filter::Audio, Kind::Audio))
    }

    fn nothing(self) -> &'static str {
        match self {
            Filter::All => "Nothing here that can be added.",
            Filter::Photos => "No photos here.",
            Filter::Videos => "No videos here.",
            Filter::Audio => "No audio here.",
        }
    }
}

type ListMsg = Result<Listing, String>;

/// The picker's own state: what is open and what is ticked.
pub(in crate::ui) struct Picker {
    tab: Tab,
    filter: Filter,
    dir: Option<PathBuf>,
    listing: Option<ListMsg>,
    list_rx: Option<(PathBuf, Receiver<ListMsg>)>,
    recent: Option<Vec<File>>,
    recent_rx: Option<Receiver<Vec<File>>>,
    /// The access state `recent` was made under, so it is redone when a permission arrives.
    recent_for: Option<(bool, bool)>,
    selected: Vec<PathBuf>,
    asked: bool,
    asked_all: bool,
    /// Last polled access (media, all files) and when.
    access: (bool, bool),
    polled_at: f64,
}

impl Picker {
    pub(in crate::ui) fn new() -> Picker {
        Picker {
            tab: Tab::Recent,
            filter: Filter::All,
            dir: None,
            listing: None,
            list_rx: None,
            recent: None,
            recent_rx: None,
            recent_for: None,
            selected: Vec::new(),
            asked: false,
            asked_all: false,
            access: (false, false),
            polled_at: f64::NEG_INFINITY,
        }
    }
}

/// Thumbnails for the picker: decoded textures plus the worker that makes the missing ones.
#[derive(Default)]
pub(in crate::ui) struct MediaThumbs {
    textures: HashMap<PathBuf, (String, Option<f64>, Option<TextureHandle>)>,
    jobs: Option<Sender<File>>,
    done: Option<Receiver<(File, Result<Thumb, String>)>>,
    asked: HashSet<PathBuf>,
    failed: HashSet<PathBuf>,
}

impl MediaThumbs {
    /// True while thumbnails that were asked for are still being drawn.
    pub(in crate::ui) fn has_pending(&self) -> bool {
        self.asked.iter().any(|p| !self.textures.contains_key(p) && !self.failed.contains(p))
    }
}

fn stamp(f: &File) -> String {
    format!("{}:{}", f.modified, f.bytes)
}

fn load_texture(ctx: &egui::Context, png: &Path) -> Option<TextureHandle> {
    let img = image::open(png).ok()?.to_rgba8();
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    Some(ctx.load_texture(format!("media-{}", png.display()), ci, TextureOptions::LINEAR))
}

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

fn readable(r: &Root, media: bool, all: bool) -> bool {
    match r.needs {
        Needs::Nothing => true,
        Needs::Media => media || all,
        Needs::AllFiles => all,
    }
}

impl MobileApp {
    fn thumbs_dir(&self) -> PathBuf {
        self.storage.join("cache").join("media")
    }

    /// Takes in the thumbnails the worker has finished.
    fn poll_media_thumbs(&mut self, ctx: &egui::Context) {
        let mut finished = Vec::new();
        let mut dead = false;
        if let Some(rx) = &self.media_thumbs.done {
            loop {
                match rx.try_recv() {
                    Ok(m) => finished.push(m),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        dead = true;
                        break;
                    }
                }
            }
        }
        for (f, r) in finished {
            let t = &mut self.media_thumbs;
            // Audio has a length and no picture; anything else needs its picture to count.
            let ready = r.ok().and_then(|th| match &th.png {
                Some(png) => load_texture(ctx, png).map(|tex| (th.duration, Some(tex))),
                None => Some((th.duration, None)),
            });
            match ready {
                Some((dur, tex)) => {
                    if t.textures.len() > 400 {
                        t.textures.clear();
                    }
                    t.textures.insert(f.path.clone(), (stamp(&f), dur, tex));
                }
                None => {
                    t.failed.insert(f.path.clone());
                }
            }
        }
        if dead {
            // The worker is gone (it should not be); what it was asked for is not coming.
            let t = &mut self.media_thumbs;
            t.failed.extend(t.asked.drain());
            t.jobs = None;
            t.done = None;
        }
    }

    /// The thumbnail for a file if it is ready; otherwise asks for it and returns `None`.
    fn media_thumb(&mut self, ctx: &egui::Context, f: &File) -> Option<(Option<TextureHandle>, Option<f64>)> {
        if let Some((s, d, t)) = self.media_thumbs.textures.get(&f.path)
            && *s == stamp(f)
        {
            return Some((t.clone(), *d));
        }
        if self.media_thumbs.failed.contains(&f.path) {
            return None;
        }
        if !self.media_thumbs.asked.contains(&f.path) {
            // Already drawn in an earlier session? Then no worker is needed.
            if let Some(th) = Thumbs::new(self.thumbs_dir()).lookup(f) {
                let tex = match &th.png {
                    Some(png) => load_texture(ctx, png),
                    None => None,
                };
                if tex.is_some() || th.png.is_none() {
                    self.media_thumbs.textures.insert(f.path.clone(), (stamp(f), th.duration, tex.clone()));
                    return Some((tex, th.duration));
                }
            }
            self.request_media_thumb(ctx, f);
        }
        None
    }

    fn request_media_thumb(&mut self, ctx: &egui::Context, f: &File) {
        if self.media_thumbs.jobs.is_none() {
            let (jtx, jrx) = mpsc::channel::<File>();
            let (dtx, drx) = mpsc::channel();
            let thumbs = Thumbs::new(self.thumbs_dir());
            let repaint = ctx.clone();
            let spawned = std::thread::Builder::new().name("effectia-media-thumbs".into()).spawn(move || {
                thumbs.prune();
                let mut scratch = None;
                while let Ok(file) = jrx.recv() {
                    let r = thumbs.make(&file, &mut scratch);
                    if dtx.send((file, r)).is_err() {
                        break;
                    }
                    repaint.request_repaint();
                }
            });
            if spawned.is_err() {
                self.media_thumbs.failed.insert(f.path.clone());
                return;
            }
            self.media_thumbs.jobs = Some(jtx);
            self.media_thumbs.done = Some(drx);
        }
        let sent = self.media_thumbs.jobs.as_ref().is_some_and(|j| j.send(f.clone()).is_ok());
        if sent {
            self.media_thumbs.asked.insert(f.path.clone());
        } else {
            self.media_thumbs.failed.insert(f.path.clone());
            self.media_thumbs.jobs = None;
        }
    }

    fn poll_picker(&mut self, p: &mut Picker, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        // Permissions: asked of the platform at most twice a second, and only while one is missing.
        if p.access != (true, true) && now - p.polled_at >= 0.5 {
            p.polled_at = now;
            p.access = (self.platform.media_access() == Access::Granted, self.platform.all_files_access() == Access::Granted);
        }
        let (media, all) = p.access;
        // The recent list is made for the current permissions; redo it when they change.
        if p.recent_for != Some((media, all)) {
            p.recent_for = Some((media, all));
            p.recent = None;
            let mut paths: Vec<PathBuf> =
                self.platform.roots().iter().filter(|r| r.needs != Needs::AllFiles && readable(r, media, all)).map(|r| r.path.clone()).collect();
            paths.push(self.storage.join("media"));
            let (tx, rx) = mpsc::channel();
            let repaint = ctx.clone();
            let spawned = std::thread::Builder::new().name("effectia-recent".into()).spawn(move || {
                let _ = tx.send(browser::recent(&paths, 400));
                repaint.request_repaint();
            });
            if spawned.is_ok() {
                p.recent_rx = Some(rx);
            } else {
                p.recent = Some(Vec::new());
            }
        }
        if let Some(rx) = &p.recent_rx {
            match rx.try_recv() {
                Ok(v) => {
                    p.recent = Some(v);
                    p.recent_rx = None;
                }
                Err(TryRecvError::Disconnected) => {
                    p.recent = Some(Vec::new());
                    p.recent_rx = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some((dir, rx)) = &p.list_rx {
            match rx.try_recv() {
                Ok(l) => {
                    if p.dir.as_ref() == Some(dir) {
                        p.listing = Some(l);
                    }
                    p.list_rx = None;
                }
                Err(TryRecvError::Disconnected) => {
                    p.listing = Some(Err("the folder could not be read".into()));
                    p.list_rx = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if p.recent_rx.is_some() || p.list_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        } else if p.access != (true, true) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn open_folder(&mut self, p: &mut Picker, dir: PathBuf, ctx: &egui::Context) {
        p.dir = Some(dir.clone());
        p.listing = None;
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        let d = dir.clone();
        let spawned = std::thread::Builder::new().name("effectia-list".into()).spawn(move || {
            let _ = tx.send(browser::list(&d));
            repaint.request_repaint();
        });
        if spawned.is_ok() {
            p.list_rx = Some((dir, rx));
        } else {
            p.listing = Some(Err("could not read the folder".into()));
        }
    }

    /// All folders the picker may start from, the app's own media folder last.
    fn picker_roots(&self) -> Vec<Root> {
        let mut roots = self.platform.roots();
        roots.push(Root::new("Effectia media folder", self.storage.join("media"), Needs::Nothing));
        roots
    }

    fn go_up(&mut self, p: &mut Picker, ctx: &egui::Context) {
        let Some(dir) = p.dir.clone() else { return };
        let roots = self.picker_roots();
        if roots.iter().any(|r| r.path == dir) {
            p.dir = None;
            p.listing = None;
            p.list_rx = None;
        } else if let Some(parent) = dir.parent() {
            self.open_folder(p, parent.to_path_buf(), ctx);
        }
    }

    pub(super) fn sheet_media(&mut self, ui: &mut Ui, full: ERect, phone: bool, p: &mut Picker, ctx: &egui::Context) -> Next {
        self.poll_media_thumbs(ctx);
        self.poll_picker(p, ctx);
        let (media_ok, all_ok) = p.access;
        let (body, closed) = frame(ui, full, "Add media", 4000.0, phone, false);
        let pad = tokens::SPACE_4;

        // Tabs (and the way up when inside a folder).
        let tabs = ERect::from_min_size(pos2(body.left() + pad, body.top() + 8.0), vec2(body.width() - 2.0 * pad, 40.0));
        let mut next_tab = None;
        for (i, (tab, label)) in [(Tab::Recent, "Recent"), (Tab::Folders, "Folders")].into_iter().enumerate() {
            let w = 92.0;
            let r = ERect::from_min_size(pos2(tabs.left() + i as f32 * (w + 8.0), tabs.top() + 2.0), vec2(w, 36.0));
            let on = p.tab == tab;
            let resp = ui.interact(r, Id::new(("picker-tab", i)), Sense::click());
            resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, label));
            ui.painter().rect_filled(r, 18.0, if on { col(DARK.accent) } else { col(DARK.field) });
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::new(13.0, medium()),
                if on { col(DARK.on_accent) } else { col(DARK.text_secondary) },
            );
            if resp.clicked() {
                next_tab = Some(tab);
            }
        }
        if let Some(t) = next_tab {
            p.tab = t;
        }
        let mut up = false;
        if p.tab == Tab::Folders && p.dir.is_some() {
            let name = p.dir.as_ref().and_then(|d| d.file_name()).map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let r = ERect::from_min_size(pos2(tabs.left() + 2.0 * 100.0 + 4.0, tabs.top()), vec2(tabs.right() - tabs.left() - 204.0, 40.0));
            let resp = ui.interact(r, Id::new("picker-up"), Sense::click());
            resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Up one folder"));
            paint_icon(ui.painter(), Icon::StepBack, pos2(r.left() + 14.0, r.center().y), 18.0, col(DARK.icon));
            let clip = ui.painter().with_clip_rect(r);
            clip.text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, name, FontId::new(14.0, medium()), col(DARK.text_primary));
            up = resp.clicked();
        }
        if up {
            self.go_up(p, ctx);
        }

        // What to show: everything, or one kind.
        let chips_top = tabs.bottom() + 2.0;
        let mut x = tabs.left() + 2.0;
        for (f, label) in Filter::ALL {
            let w = 14.0 + label.len() as f32 * 8.0 + 14.0;
            let r = ERect::from_min_size(pos2(x, chips_top), vec2(w, 32.0));
            x += w + 6.0;
            let on = p.filter == f;
            let resp = ui.interact(r, Id::new(("picker-filter", label)), Sense::click());
            resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, format!("Show {}", label.to_lowercase())));
            ui.painter().rect_stroke(r, 16.0, Stroke::new(1.0, if on { col(DARK.accent) } else { col(DARK.border) }), StrokeKind::Inside);
            if on {
                ui.painter().rect_filled(r, 16.0, col(DARK.accent).gamma_multiply(0.22));
            }
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::new(12.5, medium()),
                if on { col(DARK.text_primary) } else { col(DARK.text_secondary) },
            );
            if resp.clicked() {
                p.filter = f;
            }
        }

        // Footer: what is ticked, and the button.
        let foot_h = 68.0;
        let foot = ERect::from_min_max(pos2(body.left(), body.bottom() - foot_h), body.max);
        ui.painter().rect_filled(foot, 0.0, col(DARK.surface_overlay));
        ui.painter().line_segment([foot.left_top(), foot.right_top()], Stroke::new(1.0, col(DARK.border)));
        let n = p.selected.len();
        let msg = if n == 0 {
            "Tap items to select them".to_string()
        } else if n == 1 {
            "1 selected".to_string()
        } else {
            format!("{n} selected")
        };
        ui.painter().text(pos2(foot.left() + pad, foot.center().y), Align2::LEFT_CENTER, &msg, FontId::new(14.0, medium()), col(DARK.text_secondary));
        let msg_r = ERect::from_min_size(pos2(foot.left() + pad, foot.top()), vec2(foot.width() - 2.0 * pad - 150.0, foot_h));
        ui.interact(msg_r, Id::new("picker-count"), Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, msg.clone()));
        let add_r = ERect::from_center_size(pos2(foot.right() - pad - 70.0, foot.center().y), vec2(140.0, 46.0));
        let add = ui.interact(add_r, Id::new("picker-add"), if n > 0 { Sense::click() } else { Sense::hover() });
        let add_label = if n > 1 { format!("Add {n}") } else { "Add".to_string() };
        add.widget_info(|| WidgetInfo::labeled(WidgetType::Button, n > 0, "Add selected media"));
        let (fill, fg) = if n > 0 { (col(DARK.accent), col(DARK.on_accent)) } else { (col(DARK.field), col(DARK.text_disabled)) };
        ui.painter().rect_filled(add_r, 10.0, if add.is_pointer_button_down_on() { fill.gamma_multiply(0.8) } else { fill });
        ui.painter().text(add_r.center(), Align2::CENTER_CENTER, &add_label, FontId::new(15.0, semibold()), fg);

        // The scrolling content.
        let area = ERect::from_min_max(pos2(body.left(), tabs.bottom() + 40.0), pos2(body.right(), foot.top()));
        let mut action: Option<Act> = None;
        self.scoped(ui, area, |s, ui| {
            egui::ScrollArea::vertical().id_salt(("picker-scroll", p.tab as u8, p.dir.clone())).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                action = s.picker_content(ui, p, media_ok, all_ok, pad);
            });
        });

        if closed {
            return Next::Close;
        }
        match action {
            Some(Act::Toggle(path)) => {
                if let Some(i) = p.selected.iter().position(|x| *x == path) {
                    p.selected.remove(i);
                } else {
                    p.selected.push(path);
                }
            }
            Some(Act::Folder(dir)) => {
                p.tab = Tab::Folders;
                self.open_folder(p, dir, ctx);
            }
            Some(Act::AskMedia) => {
                p.asked = true;
                self.platform.request_media_access();
                p.polled_at = f64::NEG_INFINITY;
            }
            Some(Act::AskAll) => {
                p.asked_all = true;
                self.platform.request_all_files_access();
                p.polled_at = f64::NEG_INFINITY;
            }
            Some(Act::Settings) => self.platform.open_app_settings(),
            None => {}
        }
        if add.clicked() && n > 0 {
            return self.add_selected(&p.selected, ctx);
        }
        Next::Stay
    }

    fn add_selected(&mut self, paths: &[PathBuf], ctx: &egui::Context) -> Next {
        let (mut added, mut failed) = (0usize, Vec::new());
        for path in paths {
            match self.editor.import_and_add(&path.to_string_lossy()) {
                Ok(_) => added += 1,
                Err(e) => failed.push(format!("{}: {e}", path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()))),
            }
        }
        if added > 0 {
            self.notify(ctx, if added == 1 { "Added 1 layer".to_string() } else { format!("Added {added} layers") });
        }
        if let Some(first) = failed.first() {
            self.status = Some(if failed.len() == 1 { first.clone() } else { format!("{} files could not be added. {first}", failed.len()) });
        }
        if added > 0 || failed.is_empty() { Next::Close } else { Next::Stay }
    }

    /// Draws what is under the tabs and returns what was tapped.
    fn picker_content(&mut self, ui: &mut Ui, p: &mut Picker, media_ok: bool, all_ok: bool, pad: f32) -> Option<Act> {
        let width = ui.available_width();
        let mut act: Option<Act> = None;
        let roots = self.picker_roots();
        let needs_media = roots.iter().any(|r| r.needs == Needs::Media);

        if needs_media && !media_ok && !all_ok {
            act = self.access_card(ui, width, pad, p.asked);
        }

        match (p.tab, p.dir.clone()) {
            (Tab::Recent, _) => match p.recent.clone() {
                None => hint(ui, width, pad, "Looking for photos, videos and audio\u{2026}"),
                Some(files) if files.is_empty() => {
                    let msg =
                        if needs_media && !media_ok && !all_ok { "Nothing to show until access is allowed." } else { "No photos, videos or audio found." };
                    hint(ui, width, pad, msg);
                }
                Some(files) => {
                    let shown: Vec<File> = files.into_iter().filter(|f| p.filter.keeps(f.kind)).collect();
                    if shown.is_empty() {
                        hint(ui, width, pad, p.filter.nothing());
                    } else if let Some(a) = self.grid(ui, &shown, &p.selected, width) {
                        act = Some(a);
                    }
                }
            },
            (Tab::Folders, None) => {
                for r in &roots {
                    let usable = readable(r, media_ok, all_ok);
                    let sub = match (r.needs, usable) {
                        (Needs::AllFiles, false) => "Opens settings to allow access to all files".to_string(),
                        (Needs::Media, false) => "Allow access to photos and videos first".to_string(),
                        _ => r.path.display().to_string(),
                    };
                    let it = Item::new(r.label.clone()).sub(sub).icon(Icon::Folder).enabled(usable || r.needs == Needs::AllFiles);
                    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
                    if item_row(ui, rect, Id::new(("picker-root", &r.path)), &it).clicked() {
                        act = Some(if usable { Act::Folder(r.path.clone()) } else { Act::AskAll });
                    }
                }
                if p.asked_all && !all_ok {
                    hint(ui, width, pad, "After allowing access in settings, come back here.");
                }
            }
            (Tab::Folders, Some(_)) => match p.listing.clone() {
                None => hint(ui, width, pad, "Opening\u{2026}"),
                Some(Err(e)) => hint(ui, width, pad, &format!("Could not open this folder. {e}")),
                Some(Ok(l)) => {
                    for Folder { path, name } in &l.folders {
                        let it = Item::new(name.clone()).icon(Icon::Folder);
                        let (rect, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
                        if item_row(ui, rect, Id::new(("picker-folder", path)), &it).clicked() {
                            act = Some(Act::Folder(path.clone()));
                        }
                    }
                    let shown: Vec<File> = l.files.iter().filter(|f| p.filter.keeps(f.kind)).cloned().collect();
                    if shown.is_empty() && l.folders.is_empty() {
                        hint(ui, width, pad, p.filter.nothing());
                    } else if let Some(a) = self.grid(ui, &shown, &p.selected, width) {
                        act = Some(a);
                    }
                }
            },
        }
        ui.add_space(16.0);
        act
    }

    /// The permission explanation with its buttons.
    fn access_card(&mut self, ui: &mut Ui, width: f32, pad: f32, asked: bool) -> Option<Act> {
        let h = if asked { 214.0 } else { 160.0 };
        let (rect, _) = ui.allocate_exact_size(vec2(width, h + 12.0), Sense::hover());
        let card = ERect::from_min_size(rect.min + vec2(pad, 4.0), vec2(width - 2.0 * pad, h));
        let p = ui.painter();
        p.rect_filled(card, tokens::RADIUS_LG, col(DARK.surface_raised));
        p.rect_stroke(card, tokens::RADIUS_LG, Stroke::new(1.0, col(DARK.border)), StrokeKind::Inside);
        p.text(
            pos2(card.left() + 14.0, card.top() + 22.0),
            Align2::LEFT_CENTER,
            "Allow access to your photos and videos",
            FontId::new(15.0, semibold()),
            col(DARK.text_primary),
        );
        let body = "Effectia needs permission to read them so you can add them to a project. Nothing leaves your device.";
        let g = p.layout(body.to_string(), FontId::new(13.0, FontFamily::Proportional), col(DARK.text_secondary), card.width() - 28.0);
        p.galley(pos2(card.left() + 14.0, card.top() + 40.0), g.clone(), col(DARK.text_secondary));
        let mut act = None;
        let by = card.top() + 40.0 + g.size().y + 12.0;
        let allow = ERect::from_min_size(pos2(card.left() + 14.0, by), vec2(card.width() - 28.0, 44.0));
        let r = ui.interact(allow, Id::new("picker-allow"), Sense::click());
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Allow access"));
        ui.painter().rect_filled(allow, 10.0, if r.is_pointer_button_down_on() { col(DARK.accent).gamma_multiply(0.8) } else { col(DARK.accent) });
        ui.painter().text(allow.center(), Align2::CENTER_CENTER, "Allow access", FontId::new(15.0, semibold()), col(DARK.on_accent));
        if r.clicked() {
            act = Some(Act::AskMedia);
        }
        if asked {
            let set = ERect::from_min_size(pos2(card.left() + 14.0, by + 52.0), vec2(card.width() - 28.0, 40.0));
            let r2 = ui.interact(set, Id::new("picker-settings"), Sense::click());
            r2.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Open app settings"));
            ui.painter().rect_filled(set, 10.0, col(DARK.field));
            ui.painter().text(set.center(), Align2::CENTER_CENTER, "Open app settings", FontId::new(14.0, medium()), col(DARK.text_primary));
            if r2.clicked() {
                act = Some(Act::Settings);
            }
        }
        act
    }

    /// A grid of square thumbnails; ticking is by tapping.
    fn grid(&mut self, ui: &mut Ui, files: &[File], selected: &[PathBuf], width: f32) -> Option<Act> {
        let gap = 3.0;
        let cols = ((width / 110.0).floor() as usize).clamp(3, 8);
        let cell = (width - gap * (cols as f32 - 1.0)) / cols as f32;
        let ctx = ui.ctx().clone();
        let mut act = None;
        for row in files.chunks(cols) {
            let (strip, _) = ui.allocate_exact_size(vec2(width, cell + gap), Sense::hover());
            if !ui.is_rect_visible(strip) {
                continue;
            }
            for (c, f) in row.iter().enumerate() {
                let rect = ERect::from_min_size(pos2(strip.left() + c as f32 * (cell + gap), strip.top()), vec2(cell, cell));
                let order = selected.iter().position(|x| *x == f.path);
                if self.media_cell(ui, &ctx, rect, f, order) {
                    act = Some(Act::Toggle(f.path.clone()));
                }
            }
        }
        act
    }

    fn media_cell(&mut self, ui: &mut Ui, ctx: &egui::Context, rect: ERect, f: &File, order: Option<usize>) -> bool {
        let resp = ui.interact(rect, Id::new(("picker-cell", &f.path)), Sense::click());
        let label = format!("{}, {}", f.name, browser::size_label(f.bytes));
        resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, order.is_some(), label.clone()));
        let thumb = self.media_thumb(ctx, f);
        let p = ui.painter();
        p.rect_filled(rect, 0.0, col(DARK.field));
        match &thumb {
            Some((Some(tex), _)) => {
                // Cover: fill the square, cropping the long side.
                let s = tex.size_vec2();
                let (u, v) = if s.x > s.y { ((s.x - s.y) / (2.0 * s.x), 0.0) } else { (0.0, (s.y - s.x) / (2.0 * s.y)) };
                p.image(tex.id(), rect, ERect::from_min_max(pos2(u, v), pos2(1.0 - u, 1.0 - v)), Color32::WHITE);
            }
            _ => match f.kind {
                Kind::Audio => {
                    let ext = f.path.extension().map_or_else(String::new, |e| e.to_string_lossy().to_uppercase());
                    paint_icon(p, Icon::Music, rect.center() - vec2(0.0, 10.0), 30.0, col(DARK.text_secondary));
                    p.text(rect.center() + vec2(0.0, 20.0), Align2::CENTER_CENTER, ext, FontId::new(11.0, medium()), col(DARK.text_disabled));
                }
                Kind::Video => paint_icon(p, Icon::Film, rect.center(), 26.0, col(DARK.text_disabled)),
                Kind::Image => paint_icon(p, Icon::RectOutline, rect.center(), 26.0, col(DARK.text_disabled)),
            },
        }
        if f.kind == Kind::Video || f.kind == Kind::Audio {
            let text = match (&thumb, f.kind) {
                (Some((_, Some(d))), _) => clock(*d),
                (_, Kind::Audio) => "audio".to_string(),
                _ => "video".to_string(),
            };
            let chip = ERect::from_min_size(pos2(rect.left() + 4.0, rect.bottom() - 22.0), vec2(54.0, 18.0));
            p.rect_filled(chip, 9.0, Color32::from_black_alpha(170));
            if f.kind == Kind::Video {
                tri(p, pos2(chip.left() + 10.0, chip.center().y), 4.0, true, Color32::WHITE);
            } else {
                paint_icon(p, Icon::Music, pos2(chip.left() + 10.0, chip.center().y), 11.0, Color32::WHITE);
            }
            p.text(pos2(chip.left() + 18.0, chip.center().y), Align2::LEFT_CENTER, text, FontId::new(10.5, medium()), Color32::WHITE);
        }
        if order.is_some() {
            p.rect_filled(rect, 0.0, col(DARK.accent).gamma_multiply(0.22));
            p.rect_stroke(rect, 0.0, Stroke::new(3.0, col(DARK.accent)), StrokeKind::Inside);
        }
        let badge = ERect::from_center_size(pos2(rect.right() - 14.0, rect.top() + 14.0), vec2(22.0, 22.0));
        match order {
            Some(i) => {
                p.circle_filled(badge.center(), 11.0, col(DARK.accent));
                p.text(badge.center(), Align2::CENTER_CENTER, (i + 1).to_string(), FontId::new(12.0, semibold()), col(DARK.on_accent));
            }
            None => {
                p.circle_filled(badge.center(), 11.0, Color32::from_black_alpha(80));
                p.circle_stroke(badge.center(), 10.0, Stroke::new(1.6, Color32::from_white_alpha(200)));
            }
        }
        resp.clicked()
    }
}

enum Act {
    Toggle(PathBuf),
    Folder(PathBuf),
    AskMedia,
    AskAll,
    Settings,
}

fn hint(ui: &mut Ui, width: f32, pad: f32, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 56.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + pad, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::new(13.0, FontFamily::Proportional),
        col(DARK.text_secondary),
    );
}
