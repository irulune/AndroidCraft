//! The project gallery screen: a grid of saved projects with pictures, search, sorting, and a menu
//! per project (open, rename, duplicate, delete). The files, thumbnails and their cache live in
//! [`crate::gallery`]; this module draws them and runs the background job that makes the pictures.
//!
//! Leaving the editor for the gallery saves unsaved changes first (to the project's own file, or a
//! new file in `projects/`), so opening another project from here can never lose work.

use std::path::Path;
use std::sync::mpsc::{self, TryRecvError};

use super::*;
use crate::gallery::{self, DEMO_KEY, Entry, Library, Sort, Source};

type ThumbMsg = (PathBuf, Result<(), String>);

/// Everything the gallery screen remembers between frames.
#[derive(Default)]
pub(super) struct GalleryState {
    entries: Vec<Entry>,
    demo: Option<Entry>,
    query: String,
    sort: Sort,
    textures: HashMap<PathBuf, (String, TextureHandle)>,
    /// The thumbnail worker, while it runs.
    rx: Option<Receiver<ThumbMsg>>,
    /// What the running worker was asked to draw, to mark the leftovers as failed if it dies.
    asked: Vec<PathBuf>,
    /// Projects whose picture could not be made; not retried until the gallery is reopened.
    failed: HashSet<PathBuf>,
    /// A message to show once the gallery is on screen (the autosave on the way in).
    note: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Editor,
    Gallery,
}

enum Tile {
    New,
    Item(Box<Entry>),
}

enum Hit {
    New,
    Open(Box<Entry>),
    Menu(PathBuf),
}

fn source_path(s: &Source) -> PathBuf {
    match s {
        Source::Demo => PathBuf::from(DEMO_KEY),
        Source::Project(p) => p.clone(),
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl MobileApp {
    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn in_gallery(&self) -> bool {
        self.screen == Screen::Gallery
    }

    /// True while previews are being drawn in the background.
    pub fn gallery_busy(&self) -> bool {
        let t = &self.media_thumbs;
        self.gallery.rx.is_some() || t.has_pending()
    }

    fn library(&self) -> Library {
        Library::new(self.storage.clone())
    }

    /// True when there is an open project to go back to.
    fn can_resume(&mut self) -> bool {
        self.editor.comp().ok().flatten().is_some()
    }

    /// The open project's file, if it has been saved.
    fn current_project(&mut self) -> Option<PathBuf> {
        self.editor.project_path().ok().flatten().map(PathBuf::from)
    }

    pub(super) fn is_current_project(&mut self, p: &Path) -> bool {
        self.current_project().is_some_and(|c| same_file(&c, p))
    }

    /// Saves unsaved changes: to the project's file, or to a new file named after the composition.
    /// Does nothing when there is nothing to keep. Returns the file written.
    fn autosave(&mut self) -> crate::editor::Result<Option<PathBuf>> {
        if !self.editor.is_dirty()? {
            return Ok(None);
        }
        let target = match self.current_project() {
            Some(p) => p,
            None => {
                // A project that was never saved counts as "changed" only if the user did something:
                // the bundled demo is marked unsaved when it opens, but its undo history is empty.
                if !self.editor.can_undo()? {
                    return Ok(None);
                }
                let Some(comp) = self.editor.comp()? else { return Ok(None) };
                unique(&self.dir("projects"), &safe_name(&comp.name), "ecproj")
            }
        };
        self.editor.save(Some(&target.to_string_lossy()))?;
        Ok(Some(target))
    }

    /// Goes to the gallery, saving first. False (and an error in the header) when the save failed,
    /// in which case the editor stays open.
    pub fn show_gallery(&mut self) -> bool {
        self.playing = false;
        self.sheet = None;
        self.pick_subject = None;
        match self.autosave() {
            Ok(saved) => {
                self.gallery.note = saved.map(|p| format!("Saved {}", p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned())));
            }
            Err(e) => {
                self.status = Some(format!("Could not save before leaving: {e}"));
                return false;
            }
        }
        self.screen = Screen::Gallery;
        self.gallery.failed.clear();
        self.refresh_gallery();
        let lib = self.library();
        lib.prune(&self.gallery.entries);
        true
    }

    /// Back to the open project.
    pub fn leave_gallery(&mut self) {
        self.screen = Screen::Editor;
        self.sheet = None;
    }

    pub(super) fn refresh_gallery(&mut self) {
        let lib = self.library();
        self.gallery.entries = lib.scan();
        self.gallery.demo = Some(lib.demo());
    }

    /// Starts the background job that draws the missing pictures, if there are any.
    fn start_thumbnails(&mut self, ctx: &egui::Context) {
        if self.gallery.rx.is_some() {
            return;
        }
        let lib = self.library();
        let mut todo = lib.stale(&self.gallery.entries);
        todo.retain(|s| !self.gallery.failed.contains(&source_path(s)));
        if todo.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel::<ThumbMsg>();
        let repaint = ctx.clone();
        let asked: Vec<PathBuf> = todo.iter().map(source_path).collect();
        let spawned = std::thread::Builder::new().name("effectia-thumbs".into()).spawn(move || {
            for s in todo {
                let r = lib.make_thumbnail(&s).map_err(|e| e.to_string());
                if tx.send((source_path(&s), r)).is_err() {
                    return;
                }
                repaint.request_repaint();
            }
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.gallery.rx = Some(rx);
                self.gallery.asked = asked;
            }
            Err(e) => self.status = Some(format!("could not draw previews: {e}")),
        }
    }

    fn poll_gallery(&mut self, ctx: &egui::Context) {
        let g = &mut self.gallery;
        let (mut changed, mut finished) = (false, false);
        if let Some(rx) = &g.rx {
            loop {
                match rx.try_recv() {
                    Ok((p, r)) => {
                        changed = true;
                        if r.is_err() {
                            g.failed.insert(p);
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        if changed || finished {
            self.refresh_gallery();
        }
        if finished {
            // Anything the worker was asked for and still lacks a picture died with it: do not loop.
            let lib = self.library();
            let stale: Vec<PathBuf> = lib.stale(&self.gallery.entries).iter().map(source_path).collect();
            for p in std::mem::take(&mut self.gallery.asked) {
                if stale.contains(&p) {
                    self.gallery.failed.insert(p);
                }
            }
            self.gallery.rx = None;
        }
        self.start_thumbnails(ctx);
        if self.gallery.rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }

    /// The thumbnail as a texture, decoded the first time it is on screen.
    fn thumb_texture(&mut self, ctx: &egui::Context, e: &Entry) -> Option<TextureHandle> {
        let thumb = e.thumb.as_ref()?;
        let stamp = format!("{}:{}", e.modified, e.bytes);
        if let Some((s, t)) = self.gallery.textures.get(thumb)
            && *s == stamp
        {
            return Some(t.clone());
        }
        let img = image::open(thumb).ok()?.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        let tex = ctx.load_texture(format!("thumb-{}", thumb.display()), ci, TextureOptions::LINEAR);
        self.gallery.textures.insert(thumb.clone(), (stamp, tex.clone()));
        Some(tex)
    }

    fn gallery_open(&mut self, ctx: &egui::Context, e: &Entry) {
        let r = if e.is_demo() { self.editor.open_demo() } else { self.editor.open(&e.path.to_string_lossy()) };
        match r {
            Ok(()) => {
                self.forget_selection_caches();
                self.playing = false;
                self.screen = Screen::Editor;
                self.status = None;
                let name = e.name.clone();
                self.notify(ctx, format!("Opened {name}"));
            }
            Err(err) => self.notify(ctx, err.to_string()),
        }
    }

    /// The same flow as File, New project: an empty project, then the new-composition form. A
    /// dedicated page can take over from here without touching the rest of the gallery.
    fn gallery_new(&mut self, ctx: &egui::Context) {
        match self.editor.new_project() {
            Ok(()) => {
                self.forget_selection_caches();
                self.playing = false;
                self.screen = Screen::Editor;
                self.sheet = Some(sheets::Sheet::NewComp(sheets::CompForm::default_new()));
            }
            Err(err) => self.notify(ctx, err.to_string()),
        }
    }

    pub(super) fn rename_project(&mut self, p: &Path, name: &str, ctx: &egui::Context) -> crate::editor::Result<()> {
        let was_open = self.is_current_project(p);
        let new = self.library().rename(p, name)?;
        if was_open {
            self.editor.open(&new.to_string_lossy())?;
            self.forget_selection_caches();
        }
        self.refresh_gallery();
        let stem = new.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        self.notify(ctx, format!("Renamed to {stem}"));
        Ok(())
    }

    pub(super) fn duplicate_project(&mut self, p: &Path, ctx: &egui::Context) -> crate::editor::Result<()> {
        let new = self.library().duplicate(p)?;
        self.refresh_gallery();
        let stem = new.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        self.notify(ctx, format!("Duplicated as {stem}"));
        Ok(())
    }

    pub(super) fn delete_project(&mut self, p: &Path, ctx: &egui::Context) -> crate::editor::Result<()> {
        let was_open = self.is_current_project(p);
        self.library().delete(p)?;
        if was_open {
            // The open copy has nothing left on disk to save to: start clean rather than keep a
            // ghost that would silently recreate the file.
            self.editor.new_project()?;
            self.forget_selection_caches();
        }
        self.refresh_gallery();
        self.notify(ctx, "Project deleted");
        Ok(())
    }

    /// Opens a project from its gallery menu.
    pub(super) fn open_project_file(&mut self, p: &Path, ctx: &egui::Context) {
        let name = p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let entry = Entry { path: p.to_path_buf(), name, modified: 0, bytes: 0, info: None, thumb: None, error: None };
        self.gallery_open(ctx, &entry);
    }

    /// Draws the gallery over the whole window.
    pub(super) fn gallery_screen(&mut self, ui: &mut Ui, full: ERect) {
        let ctx = ui.ctx().clone();
        self.poll_gallery(&ctx);
        if let Some(msg) = self.gallery.note.take() {
            self.notify(&ctx, msg);
        }
        let safe = self.safe;
        let phone = layout::classify(full.width(), full.height()) != WorkspaceKind::Tablet;
        let can_resume = self.can_resume();
        ui.painter().rect_filled(full, 0.0, col(DARK.canvas));

        // Title bar.
        let bar = ERect::from_min_size(full.min, vec2(full.width(), safe.top + 56.0));
        ui.painter().rect_filled(bar, 0.0, col(DARK.header));
        ui.painter().line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(1.0, col(DARK.border)));
        let mid = full.top() + safe.top + 28.0;
        ui.painter().text(
            pos2(full.left() + safe.left + tokens::SPACE_4, mid),
            Align2::LEFT_CENTER,
            "Projects",
            FontId::new(tokens::HEADING.size, semibold()),
            col(DARK.text_primary),
        );
        if can_resume {
            let b = ERect::from_center_size(pos2(full.right() - safe.right - tokens::SPACE_2 - 22.0, mid), vec2(44.0, 44.0));
            if icon_button(ui, Id::new("gallery-back"), b, Icon::Close, col(DARK.icon), "Back to editor", true).clicked() {
                self.leave_gallery();
                return;
            }
        }

        // Search and sort.
        let (left, right) = (full.left() + safe.left + tokens::SPACE_4, full.right() - safe.right - tokens::SPACE_4);
        let row_y = bar.bottom() + tokens::SPACE_3;
        let chip_w = 76.0;
        let chips_x = right - 2.0 * chip_w - tokens::SPACE_2;
        let search = ERect::from_min_max(pos2(left, row_y), pos2(chips_x - tokens::SPACE_2, row_y + 44.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(search));
        let r = child.add_sized(
            search.size(),
            egui::TextEdit::singleline(&mut self.gallery.query)
                .hint_text("Search projects")
                .font(FontId::new(15.0, FontFamily::Proportional))
                .margin(vec2(12.0, 11.0)),
        );
        r.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, true, "Search projects"));
        for (i, (sort, label, spoken)) in [(Sort::Recent, "Recent", "Sort by recent"), (Sort::Name, "A\u{2013}Z", "Sort by name")].into_iter().enumerate() {
            let rect = ERect::from_min_size(pos2(chips_x + i as f32 * (chip_w + tokens::SPACE_2), row_y + 4.0), vec2(chip_w, 36.0));
            let resp = ui.interact(rect, Id::new(("gallery-sort", i)), Sense::click());
            resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, self.gallery.sort == sort, spoken));
            let on = self.gallery.sort == sort;
            ui.painter().rect_filled(rect, 18.0, if on { col(DARK.accent) } else { col(DARK.field) });
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::new(13.0, medium()),
                if on { col(DARK.on_accent) } else { col(DARK.text_secondary) },
            );
            if resp.clicked() {
                self.gallery.sort = sort;
            }
        }

        // The grid.
        let area = ERect::from_min_max(pos2(left, row_y + 44.0 + tokens::SPACE_3), pos2(right, full.bottom() - safe.bottom));
        let query = self.gallery.query.clone();
        let searching = !query.trim().is_empty();
        let mut tiles: Vec<Tile> = Vec::new();
        if !searching {
            tiles.push(Tile::New);
        }
        if let Some(d) = self.gallery.demo.clone()
            && (!searching || !gallery::order(std::slice::from_ref(&d), &query, Sort::Name).is_empty())
        {
            tiles.push(Tile::Item(Box::new(d)));
        }
        let ordered = gallery::order(&self.gallery.entries, &query, self.gallery.sort);
        let have_projects = !self.gallery.entries.is_empty();
        for i in ordered {
            if let Some(e) = self.gallery.entries.get(i) {
                tiles.push(Tile::Item(Box::new(e.clone())));
            }
        }
        let gap = tokens::SPACE_3;
        let cols = (((area.width() + gap) / (156.0 + gap)).floor() as usize).clamp(2, 6);
        let card_w = (area.width() - gap * (cols as f32 - 1.0)) / cols as f32;
        let thumb_h = (card_w - 12.0) * 9.0 / 16.0;
        let card_h = thumb_h + 12.0 + 66.0;
        let mut hit: Option<Hit> = None;
        let now = now_secs();
        self.scoped(ui, area, |s, ui| {
            egui::ScrollArea::vertical().id_salt("gallery-grid").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(gap, gap);
                for row in tiles.chunks(cols) {
                    let (strip, _) = ui.allocate_exact_size(vec2(area.width(), card_h), Sense::hover());
                    for (c, tile) in row.iter().enumerate() {
                        let rect = ERect::from_min_size(pos2(strip.left() + c as f32 * (card_w + gap), strip.top()), vec2(card_w, card_h));
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        match tile {
                            Tile::New => {
                                if s.new_card(ui, rect, thumb_h) {
                                    hit = Some(Hit::New);
                                }
                            }
                            Tile::Item(e) => match s.project_card(ui, rect, thumb_h, e, now) {
                                Some(true) => hit = Some(Hit::Menu(e.path.clone())),
                                Some(false) => hit = Some(Hit::Open(e.clone())),
                                None => {}
                            },
                        }
                    }
                }
                let hint = if searching && tiles.is_empty() {
                    Some(format!("No project matches \u{201c}{}\u{201d}", query.trim()))
                } else if !searching && !have_projects {
                    Some("Projects you save appear here.".to_string())
                } else {
                    None
                };
                if let Some(h) = hint {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(h).size(13.0).color(col(DARK.text_secondary)));
                }
                ui.add_space(24.0);
            });
        });

        self.sheet_ui(ui, full, None, phone);
        match hit {
            Some(Hit::New) => self.gallery_new(&ctx),
            Some(Hit::Open(e)) => self.gallery_open(&ctx, &e),
            Some(Hit::Menu(p)) => self.sheet = Some(sheets::Sheet::Project(p)),
            None => {}
        }
    }

    fn new_card(&mut self, ui: &mut Ui, rect: ERect, thumb_h: f32) -> bool {
        let resp = ui.interact(rect, Id::new("gallery-new"), Sense::click());
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "New project"));
        let pressed = resp.is_pointer_button_down_on();
        let p = ui.painter();
        p.rect_filled(rect, tokens::RADIUS_LG, if pressed { col(DARK.row_selected) } else { col(DARK.surface) });
        p.rect_stroke(rect, tokens::RADIUS_LG, Stroke::new(1.0, col(DARK.accent).gamma_multiply(0.7)), StrokeKind::Inside);
        let thumb = ERect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(rect.width() - 12.0, thumb_h));
        paint_icon(p, Icon::Plus, thumb.center(), 34.0, col(DARK.accent));
        let x = rect.left() + 12.0;
        let y = thumb.bottom() + 16.0;
        p.text(pos2(x, y), Align2::LEFT_CENTER, "New project", FontId::new(15.0, semibold()), col(DARK.text_primary));
        p.text(pos2(x, y + 20.0), Align2::LEFT_CENTER, "Start from scratch", FontId::new(12.0, FontFamily::Proportional), col(DARK.text_secondary));
        resp.clicked()
    }

    /// Draws one project card. `Some(false)` = open it, `Some(true)` = its menu.
    fn project_card(&mut self, ui: &mut Ui, rect: ERect, thumb_h: f32, e: &Entry, now: u64) -> Option<bool> {
        let ctx = ui.ctx().clone();
        let body = ui.interact(rect, Id::new(("gallery-card", &e.path)), Sense::click());
        let label = if e.is_demo() { format!("Open demo {}", e.name) } else { format!("Open {}", e.name) };
        body.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label.clone()));
        let pressed = body.is_pointer_button_down_on();
        let thumb = ERect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(rect.width() - 12.0, thumb_h));
        let tex = self.thumb_texture(&ctx, e);
        let p = ui.painter();
        p.rect_filled(rect, tokens::RADIUS_LG, if pressed { col(DARK.row_selected) } else { col(DARK.surface_raised) });
        p.rect_stroke(rect, tokens::RADIUS_LG, Stroke::new(1.0, col(DARK.border)), StrokeKind::Inside);
        p.rect_filled(thumb, 8.0, col(DARK.canvas));
        match tex {
            Some(t) => {
                let size = t.size_vec2();
                let scale = (thumb.width() / size.x.max(1.0)).min(thumb.height() / size.y.max(1.0));
                let img = ERect::from_center_size(thumb.center(), size * scale);
                p.image(t.id(), img, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                let tint = if e.error.is_some() { col(DARK.danger) } else { col(DARK.text_disabled) };
                paint_icon(p, Icon::Film, thumb.center(), 30.0, tint);
            }
        }
        if e.is_demo() {
            let chip = ERect::from_min_size(thumb.min + vec2(8.0, 8.0), vec2(48.0, 20.0));
            p.rect_filled(chip, 10.0, col(DARK.accent));
            p.text(chip.center(), Align2::CENTER_CENTER, "DEMO", FontId::new(10.0, semibold()), col(DARK.on_accent));
        }
        let menu_w = if e.is_demo() { 0.0 } else { 40.0 };
        let x = rect.left() + 12.0;
        let y = thumb.bottom() + 16.0;
        let clip = p.with_clip_rect(ERect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 10.0 - menu_w, rect.bottom())));
        clip.text(pos2(x, y), Align2::LEFT_CENTER, &e.name, FontId::new(15.0, semibold()), col(DARK.text_primary));
        let facts_color = if e.error.is_some() { col(DARK.danger) } else { col(DARK.text_secondary) };
        clip.text(pos2(x, y + 20.0), Align2::LEFT_CENTER, e.facts(), FontId::new(12.0, FontFamily::Proportional), facts_color);
        let when = if e.is_demo() { "Template".to_string() } else { gallery::age_label(now, e.modified) };
        let when = match e.layers_label() {
            Some(l) => format!("{l} \u{00b7} {when}"),
            None => when,
        };
        let p_line3 = ui.painter().with_clip_rect(ERect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 8.0, rect.bottom())));
        p_line3.text(pos2(x, y + 37.0), Align2::LEFT_CENTER, when, FontId::new(11.0, FontFamily::Proportional), col(DARK.text_disabled));
        let mut menu = false;
        if !e.is_demo() {
            let b = ERect::from_center_size(pos2(rect.right() - 6.0 - 20.0, y + 4.0), vec2(40.0, 44.0));
            menu = icon_button(ui, Id::new(("gallery-menu", &e.path)), b, Icon::More, col(DARK.icon), &format!("Actions for {}", e.name), true).clicked();
        }
        if menu {
            Some(true)
        } else if body.clicked() {
            Some(false)
        } else {
            None
        }
    }
}
