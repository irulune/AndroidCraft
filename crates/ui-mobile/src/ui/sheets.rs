//! Sheets and menus: add layer, layer actions, blend mode, parent, label, compositions, file,
//! export, effects browser, text input and choice lists. A bottom sheet on phones, a centred card
//! on tablets; every row is a 52 pt touch target and every action is an engine command.

use std::path::{Path, PathBuf};

use super::*;
use crate::ops::{BLEND_MODES, LABELS, NewLayer, RenderFormat};

const ROW: f32 = 52.0;
const HEAD: f32 = 52.0;

pub(super) enum Sheet {
    Add,
    Layer,
    Blend,
    Parent,
    Label,
    File,
    OpenProject,
    ImportMedia,
    Export,
    Comps,
    NewComp(CompForm),
    CompSettings(CompForm),
    Effects { query: String },
    Input { kind: InputKind, value: String, title: String, multiline: bool },
    Choice { layer: u64, path: String, title: String, options: Vec<(i64, String)>, current: i64 },
    Prop { layer: u64, path: String, name: String, animated: bool },
}

pub(super) enum InputKind {
    Rename(u64),
    Text(u64),
    Str(u64, String),
    SaveAs,
}

pub(super) struct CompForm {
    name: String,
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
}

impl CompForm {
    pub(super) fn default_new() -> CompForm {
        CompForm { name: "Composition".into(), width: 1920, height: 1080, fps: 30.0, duration: 10.0 }
    }

    fn from(c: &CompView) -> CompForm {
        CompForm {
            name: c.name.clone(),
            width: c.width,
            height: c.height,
            fps: (c.frame_rate * 100.0).round() / 100.0,
            duration: (c.duration * 100.0).round() / 100.0,
        }
    }
}

/// One row of a list sheet.
struct Item {
    label: String,
    sub: String,
    icon: Option<Icon>,
    swatch: Option<Color32>,
    selected: bool,
    enabled: bool,
    danger: bool,
}

impl Item {
    fn new(label: impl Into<String>) -> Item {
        Item { label: label.into(), sub: String::new(), icon: None, swatch: None, selected: false, enabled: true, danger: false }
    }
    fn sub(mut self, s: impl Into<String>) -> Item {
        self.sub = s.into();
        self
    }
    fn icon(mut self, i: Icon) -> Item {
        self.icon = Some(i);
        self
    }
    fn swatch(mut self, c: Color32) -> Item {
        self.swatch = Some(c);
        self
    }
    fn selected(mut self, on: bool) -> Item {
        self.selected = on;
        self
    }
    fn enabled(mut self, on: bool) -> Item {
        self.enabled = on;
        self
    }
    fn danger(mut self) -> Item {
        self.danger = true;
        self
    }
}

enum Pick {
    Nothing,
    Close,
    Row(usize),
}

/// What a sheet wants to happen after it has drawn.
enum Next {
    Stay,
    Close,
    Open(Sheet),
}

/// Draws the scrim and card; returns the body rectangle and whether the sheet was dismissed.
fn frame(ui: &mut Ui, full: ERect, title: &str, body_h: f32, phone: bool, top: bool) -> (ERect, bool) {
    ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(150));
    let scrim = ui.interact(full, Id::new("sheet-scrim"), Sense::click());
    scrim.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Dismiss"));
    let max_h = if top && phone { (full.height() * 0.72).max(300.0) } else { (full.height() * 0.88).max(240.0) };
    let h = (body_h + HEAD + 8.0).min(max_h);
    let (card, radius) = if phone && top {
        (
            ERect::from_min_size(pos2(full.left(), full.top() + 52.0), vec2(full.width(), h)),
            CornerRadius { nw: 0, ne: 0, sw: tokens::RADIUS_LG as u8, se: tokens::RADIUS_LG as u8 },
        )
    } else if phone {
        (
            ERect::from_min_max(pos2(full.left(), full.bottom() - h), full.max),
            CornerRadius { nw: tokens::RADIUS_LG as u8, ne: tokens::RADIUS_LG as u8, sw: 0, se: 0 },
        )
    } else {
        (ERect::from_center_size(full.center(), vec2(480.0_f32.min(full.width() - 32.0), h)), CornerRadius::same(tokens::RADIUS_LG as u8))
    };
    let swallow = ui.interact(card, Id::new("sheet-card"), Sense::click());
    swallow.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, title.to_string()));
    ui.painter().rect_filled(card.expand(1.0), radius, col(DARK.border));
    ui.painter().rect_filled(card, radius, col(DARK.surface_overlay));
    if phone && !top {
        ui.painter().rect_filled(ERect::from_center_size(pos2(card.center().x, card.top() + 7.0), vec2(36.0, 4.0)), 2.0, col(DARK.border));
    }
    ui.painter().text(
        pos2(card.left() + tokens::SPACE_4, card.top() + HEAD / 2.0 + 2.0),
        Align2::LEFT_CENTER,
        title,
        FontId::new(16.0, semibold()),
        col(DARK.text_primary),
    );
    let close = ERect::from_center_size(pos2(card.right() - 28.0, card.top() + HEAD / 2.0 + 2.0), vec2(44.0, 44.0));
    let closed = icon_button(ui, Id::new("sheet-close"), close, Icon::Close, col(DARK.icon), "Close", true).clicked() || scrim.clicked();
    ui.painter().line_segment([pos2(card.left(), card.top() + HEAD), pos2(card.right(), card.top() + HEAD)], Stroke::new(1.0, col(DARK.border)));
    (ERect::from_min_max(pos2(card.left(), card.top() + HEAD + 1.0), card.max), closed)
}

fn item_row(ui: &mut Ui, rect: ERect, id: Id, it: &Item) -> Response {
    let sense = if it.enabled { Sense::click() } else { Sense::hover() };
    let r = ui.interact(rect, id, sense);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, it.enabled, it.label.clone()));
    if it.enabled && r.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 0.0, col(DARK.row_selected));
    } else if it.selected {
        ui.painter().rect_filled(rect, 0.0, col(DARK.row_selected).gamma_multiply(0.5));
    }
    let mut x = rect.left() + tokens::SPACE_4;
    let tint = if !it.enabled {
        col(DARK.text_disabled)
    } else if it.danger {
        col(DARK.danger)
    } else {
        col(DARK.icon)
    };
    if let Some(i) = it.icon {
        paint_icon(ui.painter(), i, pos2(x + 11.0, rect.center().y), 22.0, tint);
        x += 34.0;
    }
    if let Some(c) = it.swatch {
        ui.painter().rect_filled(ERect::from_center_size(pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0)), 4.0, c);
        x += 28.0;
    }
    let text_color = if !it.enabled {
        col(DARK.text_disabled)
    } else if it.danger {
        col(DARK.danger)
    } else {
        col(DARK.text_primary)
    };
    let clip = ui.painter().with_clip_rect(ERect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 44.0, rect.bottom())));
    if it.sub.is_empty() {
        clip.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, &it.label, FontId::new(15.0, medium()), text_color);
    } else {
        clip.text(pos2(x, rect.center().y - 9.0), Align2::LEFT_CENTER, &it.label, FontId::new(15.0, medium()), text_color);
        clip.text(pos2(x, rect.center().y + 10.0), Align2::LEFT_CENTER, &it.sub, FontId::new(12.0, FontFamily::Proportional), col(DARK.text_secondary));
    }
    if it.selected {
        paint_icon(ui.painter(), Icon::Check, pos2(rect.right() - 28.0, rect.center().y), 20.0, col(DARK.accent));
    }
    ui.painter().line_segment(
        [pos2(rect.left() + tokens::SPACE_4, rect.bottom()), pos2(rect.right(), rect.bottom())],
        Stroke::new(1.0, col(DARK.border).gamma_multiply(0.6)),
    );
    r
}

fn safe_name(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let t = cleaned.trim().trim_matches('.').to_string();
    let t: String = t.chars().take(60).collect();
    if t.is_empty() { "Untitled".into() } else { t }
}

fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..10_000).map(|n| dir.join(format!("{stem}-{n}.{ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

const MEDIA_EXT: [&str; 24] = [
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "svg", "psd", "mp4", "mov", "m4v", "webm", "mkv", "avi", "mp3", "wav", "ogg", "flac", "aac",
    "m4a", "exr", "hdr",
];

fn list_files(dir: &Path, exts: &[&str]) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(std::time::SystemTime, String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_str()?.to_ascii_lowercase();
            if !p.is_file() || !exts.contains(&ext.as_str()) {
                return None;
            }
            let t = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            Some((t, p.file_name()?.to_string_lossy().into_owned(), p))
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.0));
    out.into_iter().map(|(_, n, p)| (n, p)).collect()
}

impl MobileApp {
    fn dir(&self, sub: &str) -> PathBuf {
        let d = self.storage.join(sub);
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn forget_selection_caches(&mut self) {
        self.tree = None;
        self.keys = (u64::MAX, HashMap::new());
        self.sel_key = None;
        self.shown = None;
        self.view = timeline::View::default();
    }

    /// Draws a list sheet and returns which row was tapped.
    fn list_sheet(&mut self, ui: &mut Ui, full: ERect, phone: bool, title: &str, items: &[Item], footer: Option<&str>) -> Pick {
        let foot_h = if footer.is_some() { 44.0 } else { 0.0 };
        let (body, closed) = frame(ui, full, title, items.len() as f32 * ROW + foot_h, phone, false);
        let mut pick = Pick::Nothing;
        let list = ERect::from_min_max(body.min, pos2(body.right(), body.bottom() - foot_h));
        self.scoped(ui, list, |_, ui| {
            egui::ScrollArea::vertical().id_salt(("sheet", title)).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for (i, it) in items.iter().enumerate() {
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
                    if item_row(ui, rect, Id::new(("sheet-item", title, i)), it).clicked() {
                        pick = Pick::Row(i);
                    }
                }
            });
        });
        if let Some(f) = footer {
            let r = ERect::from_min_max(pos2(body.left() + tokens::SPACE_4, body.bottom() - foot_h), pos2(body.right() - tokens::SPACE_4, body.bottom()));
            let galley = ui.painter().layout(f.to_string(), FontId::new(11.0, FontFamily::Proportional), col(DARK.text_secondary), r.width());
            ui.painter().galley(pos2(r.left(), r.center().y - galley.size().y / 2.0), galley, col(DARK.text_secondary));
        }
        if closed { Pick::Close } else { pick }
    }

    /// Draws whichever sheet is open; call last so it sits above everything.
    pub(super) fn sheet_ui(&mut self, ui: &mut Ui, full: ERect, comp: Option<&CompView>, phone: bool) {
        let Some(mut sheet) = self.sheet.take() else { return };
        let ctx = ui.ctx().clone();
        let next = match &mut sheet {
            Sheet::Add => self.sheet_add(ui, full, phone, &ctx),
            Sheet::Layer => self.sheet_layer(ui, full, phone, comp, &ctx),
            Sheet::Blend => self.sheet_blend(ui, full, phone, comp),
            Sheet::Parent => self.sheet_parent(ui, full, phone, comp),
            Sheet::Label => self.sheet_label(ui, full, phone, comp),
            Sheet::File => self.sheet_file(ui, full, phone, comp, &ctx),
            Sheet::OpenProject => self.sheet_open_project(ui, full, phone, &ctx),
            Sheet::ImportMedia => self.sheet_import(ui, full, phone, &ctx),
            Sheet::Export => self.sheet_export(ui, full, phone, comp, &ctx),
            Sheet::Comps => self.sheet_comps(ui, full, phone, comp, &ctx),
            Sheet::NewComp(form) => self.sheet_comp_form(ui, full, phone, form, true, &ctx),
            Sheet::CompSettings(form) => self.sheet_comp_form(ui, full, phone, form, false, &ctx),
            Sheet::Effects { query } => self.sheet_effects(ui, full, phone, query, comp, &ctx),
            Sheet::Input { kind, value, title, multiline } => self.sheet_input(ui, full, phone, kind, value, title, *multiline, comp, &ctx),
            Sheet::Choice { layer, path, title, options, current } => self.sheet_choice(ui, full, phone, *layer, path, title, options, *current),
            Sheet::Prop { layer, path, name, animated } => self.sheet_prop(ui, full, phone, *layer, path, name, *animated, &ctx),
        };
        match next {
            Next::Stay => {
                if self.sheet.is_none() {
                    self.sheet = Some(sheet);
                }
            }
            Next::Close => {}
            Next::Open(s) => self.sheet = Some(s),
        }
    }

    fn sheet_add(&mut self, ui: &mut Ui, full: ERect, phone: bool, ctx: &egui::Context) -> Next {
        let mut items: Vec<Item> = NewLayer::ALL
            .iter()
            .map(|k| {
                let icon = match k {
                    NewLayer::Text => Icon::TextT,
                    NewLayer::Solid => Icon::SolidSq,
                    NewLayer::Rectangle => Icon::RectOutline,
                    NewLayer::Ellipse => Icon::Ellipse,
                    NewLayer::Star => Icon::StarShape,
                    NewLayer::Null => Icon::NullBox,
                    NewLayer::Adjustment => Icon::Sliders,
                    NewLayer::Camera => Icon::Film,
                    NewLayer::Light => Icon::Sun,
                    NewLayer::Cube | NewLayer::Sphere => Icon::Cube,
                };
                Item::new(k.label()).sub(k.hint()).icon(icon)
            })
            .collect();
        items.push(Item::new("Media from the media folder\u{2026}").sub("Images, video and audio").icon(Icon::Folder));
        match self.list_sheet(ui, full, phone, "Add layer", &items, None) {
            Pick::Close => Next::Close,
            Pick::Row(i) if i < NewLayer::ALL.len() => {
                let kind = NewLayer::ALL[i];
                let r = self.editor.add_layer(kind);
                match self.report(r) {
                    Some(id) => {
                        self.notify(ctx, format!("{} added", kind.label()));
                        if kind == NewLayer::Text {
                            return Next::Open(Sheet::Input { kind: InputKind::Text(id), value: "Text".into(), title: "Edit text".into(), multiline: true });
                        }
                        Next::Close
                    }
                    None => Next::Close,
                }
            }
            Pick::Row(_) => Next::Open(Sheet::ImportMedia),
            Pick::Nothing => Next::Stay,
        }
    }

    fn sheet_layer(&mut self, ui: &mut Ui, full: ERect, phone: bool, comp: Option<&CompView>, ctx: &egui::Context) -> Next {
        let Some(l) = comp.and_then(|c| self.selected(c)).cloned() else { return Next::Close };
        let parent = l.parent.and_then(|p| comp.and_then(|c| c.layers.iter().find(|x| x.id == p)).map(|x| x.name.clone())).unwrap_or_else(|| "None".into());
        let is_text = l.kind == "Text";
        let items = vec![
            Item::new("Rename\u{2026}").sub(l.name.clone()),
            Item::new("Edit text\u{2026}").enabled(is_text),
            Item::new("Blend mode").sub(l.blend.clone()),
            Item::new("Parent").sub(parent),
            Item::new("Label").swatch(col(label_color(&l.label))).sub(l.label.clone()),
            Item::new(if l.locked { "Unlock layer" } else { "Lock layer" }).icon(Icon::Lock).selected(l.locked),
            Item::new("Solo").selected(l.solo),
            Item::new("3D layer").selected(l.three_d),
            Item::new("Bring to front").icon(Icon::ChevronUp),
            Item::new("Bring forward").icon(Icon::ChevronUp),
            Item::new("Send backward").icon(Icon::ChevronDown),
            Item::new("Send to back").icon(Icon::ChevronDown),
            Item::new("Duplicate").icon(Icon::Copy),
            Item::new("Delete layer").icon(Icon::Trash).danger(),
        ];
        let title = format!("Layer: {}", l.name);
        match self.list_sheet(ui, full, phone, &title, &items, None) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                let id = l.id;
                let r = match i {
                    0 => {
                        return Next::Open(Sheet::Input { kind: InputKind::Rename(id), value: l.name.clone(), title: "Rename layer".into(), multiline: false });
                    }
                    1 => {
                        let current = self
                            .editor
                            .layer_tree(id, 3)
                            .ok()
                            .and_then(|t| t.find("text/sourceText").and_then(|n| n.value.as_str().map(str::to_string)))
                            .unwrap_or_default();
                        return Next::Open(Sheet::Input { kind: InputKind::Text(id), value: current, title: "Edit text".into(), multiline: true });
                    }
                    2 => return Next::Open(Sheet::Blend),
                    3 => return Next::Open(Sheet::Parent),
                    4 => return Next::Open(Sheet::Label),
                    5 => self.editor.toggle_switch(id, "lock", !l.locked),
                    6 => self.editor.toggle_switch(id, "solo", !l.solo),
                    7 => self.editor.toggle_switch(id, "threeD", !l.three_d),
                    8 => self.editor.arrange(id, "front"),
                    9 => self.editor.arrange(id, "forward"),
                    10 => self.editor.arrange(id, "backward"),
                    11 => self.editor.arrange(id, "back"),
                    12 => self.editor.duplicate_selected().map(drop),
                    _ => {
                        let r = self.editor.delete_selected();
                        if r.is_ok() {
                            self.sel_key = None;
                            self.notify(ctx, "Layer deleted");
                        }
                        r
                    }
                };
                self.report(r);
                Next::Close
            }
        }
    }

    fn sheet_blend(&mut self, ui: &mut Ui, full: ERect, phone: bool, comp: Option<&CompView>) -> Next {
        let Some(l) = comp.and_then(|c| self.selected(c)).cloned() else { return Next::Close };
        let items: Vec<Item> = BLEND_MODES.iter().map(|m| Item::new(*m).selected(*m == l.blend)).collect();
        match self.list_sheet(ui, full, phone, "Blend mode", &items, None) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                let r = self.editor.set_blend_mode(l.id, BLEND_MODES[i]);
                self.report(r);
                Next::Close
            }
        }
    }

    fn sheet_parent(&mut self, ui: &mut Ui, full: ERect, phone: bool, comp: Option<&CompView>) -> Next {
        let (Some(c), Some(l)) = (comp, comp.and_then(|c| self.selected(c)).cloned()) else { return Next::Close };
        let others: Vec<&LayerRow> = c.layers.iter().filter(|x| x.id != l.id).collect();
        let mut items = vec![Item::new("None").selected(l.parent.is_none())];
        items.extend(others.iter().map(|o| Item::new(format!("{}  {}", o.index, o.name)).swatch(col(label_color(&o.label))).selected(l.parent == Some(o.id))));
        match self.list_sheet(ui, full, phone, "Parent", &items, None) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                let parent = if i == 0 { None } else { others.get(i - 1).map(|o| o.id) };
                let r = self.editor.set_parent(l.id, parent);
                self.report(r);
                Next::Close
            }
        }
    }

    fn sheet_label(&mut self, ui: &mut Ui, full: ERect, phone: bool, comp: Option<&CompView>) -> Next {
        let Some(l) = comp.and_then(|c| self.selected(c)).cloned() else { return Next::Close };
        let items: Vec<Item> = LABELS.iter().map(|n| Item::new(*n).swatch(col(label_color(n))).selected(*n == l.label)).collect();
        match self.list_sheet(ui, full, phone, "Label colour", &items, None) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                let r = self.editor.set_label(l.id, LABELS[i]);
                self.report(r);
                Next::Close
            }
        }
    }

    fn sheet_file(&mut self, ui: &mut Ui, full: ERect, phone: bool, comp: Option<&CompView>, ctx: &egui::Context) -> Next {
        let has_comp = comp.is_some();
        let path = self.editor.project_path().ok().flatten();
        let dirty = self.editor.is_dirty().unwrap_or(false);
        let items = vec![
            Item::new("New project").icon(Icon::Plus),
            Item::new("Open demo project").sub("The EffectCraft intro, with 3D and effects").icon(Icon::Film),
            Item::new("Open project\u{2026}").sub("Saved in the projects folder").icon(Icon::Folder),
            Item::new("Save").icon(Icon::Floppy).sub(match (&path, dirty) {
                (Some(p), true) => {
                    format!("Unsaved changes \u{00b7} {}", Path::new(p).file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()))
                }
                (Some(p), false) => format!("Saved \u{00b7} {}", Path::new(p).file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())),
                (None, _) => "Not saved yet".to_string(),
            }),
            Item::new("Save as\u{2026}").icon(Icon::Copy),
            Item::new("Export and render\u{2026}").sub("PNG frame, MP4 or GIF").icon(Icon::Export).enabled(has_comp),
            Item::new("Composition settings\u{2026}").sub("Size, frame rate, duration").icon(Icon::Sliders).enabled(has_comp),
        ];
        let footer = format!("Files live in {}", self.storage.display());
        match self.list_sheet(ui, full, phone, "File", &items, Some(&footer)) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => match i {
                0 => {
                    let r = self.editor.new_project();
                    if self.report(r).is_some() {
                        self.forget_selection_caches();
                        self.playing = false;
                        return Next::Open(Sheet::NewComp(CompForm::default_new()));
                    }
                    Next::Close
                }
                1 => {
                    let r = self.editor.open_demo();
                    if self.report(r).is_some() {
                        self.forget_selection_caches();
                        self.notify(ctx, "Demo project opened");
                    }
                    Next::Close
                }
                2 => Next::Open(Sheet::OpenProject),
                3 => {
                    let target = match path {
                        Some(p) => p,
                        None => unique(&self.dir("projects"), "Untitled", "ecproj").to_string_lossy().into_owned(),
                    };
                    let r = self.editor.save(Some(&target));
                    if let Some(p) = self.report(r) {
                        self.notify(ctx, format!("Saved {p}"));
                    }
                    Next::Close
                }
                4 => Next::Open(Sheet::Input {
                    kind: InputKind::SaveAs,
                    value: path
                        .as_deref()
                        .and_then(|p| Path::new(p).file_stem())
                        .map_or_else(|| "My project".to_string(), |s| s.to_string_lossy().into_owned()),
                    title: "Save project as".into(),
                    multiline: false,
                }),
                5 => Next::Open(Sheet::Export),
                _ => match comp {
                    Some(c) => Next::Open(Sheet::CompSettings(CompForm::from(c))),
                    None => Next::Close,
                },
            },
        }
    }

    fn sheet_open_project(&mut self, ui: &mut Ui, full: ERect, phone: bool, ctx: &egui::Context) -> Next {
        let files = list_files(&self.dir("projects"), &["ecproj"]);
        let mut items: Vec<Item> = files.iter().map(|(n, _)| Item::new(n.clone()).icon(Icon::Film)).collect();
        if items.is_empty() {
            items.push(Item::new("No saved projects yet").sub("Use File \u{25b8} Save, then it appears here").enabled(false));
        }
        let footer = format!("Folder: {}", self.dir("projects").display());
        match self.list_sheet(ui, full, phone, "Open project", &items, Some(&footer)) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                if let Some((name, path)) = files.get(i) {
                    let r = self.editor.open(&path.to_string_lossy());
                    if self.report(r).is_some() {
                        self.forget_selection_caches();
                        self.notify(ctx, format!("Opened {name}"));
                    }
                }
                Next::Close
            }
        }
    }

    fn sheet_import(&mut self, ui: &mut Ui, full: ERect, phone: bool, ctx: &egui::Context) -> Next {
        let dir = self.dir("media");
        let files = list_files(&dir, &MEDIA_EXT);
        let mut items: Vec<Item> = files.iter().map(|(n, _)| Item::new(n.clone()).icon(Icon::Film)).collect();
        if items.is_empty() {
            items.push(Item::new("No media files yet").sub("Copy images, video or audio into the folder below, then open this list again").enabled(false));
        }
        let footer = format!("Media folder: {}", dir.display());
        match self.list_sheet(ui, full, phone, "Add media as a layer", &items, Some(&footer)) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                if let Some((name, path)) = files.get(i) {
                    let r = self.editor.import_and_add(&path.to_string_lossy());
                    if self.report(r).is_some() {
                        self.notify(ctx, format!("{name} added as a layer"));
                    }
                }
                Next::Close
            }
        }
    }

    fn sheet_export(&mut self, ui: &mut Ui, full: ERect, phone: bool, comp: Option<&CompView>, ctx: &egui::Context) -> Next {
        let Some(c) = comp else { return Next::Close };
        let busy = self.job.is_some();
        let frames = (c.duration * c.frame_rate).round() as u64;
        let items = vec![
            Item::new("Current frame as PNG").sub(format!("{} \u{00d7} {} at {}", c.width, c.height, timecode(c.time, c.frame_rate))).icon(Icon::Export),
            Item::new("Render MP4, half size")
                .sub(format!("{frames} frames, quicker, {} \u{00d7} {}", c.width / 2, c.height / 2))
                .icon(Icon::Film)
                .enabled(!busy),
            Item::new("Render MP4, full size").sub(format!("{frames} frames at {} \u{00d7} {}", c.width, c.height)).icon(Icon::Film).enabled(!busy),
            Item::new("Render GIF, half size").sub("Looping animation").icon(Icon::Film).enabled(!busy),
        ];
        let footer = if busy { "A render is already running".to_string() } else { format!("Saved to {}", self.dir("exports").display()) };
        match self.list_sheet(ui, full, phone, "Export", &items, Some(&footer)) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(0) => {
                let out = unique(&self.dir("exports"), &safe_name(&c.name), "png");
                let r = self.editor.export_frame(&out.to_string_lossy());
                if let Some(p) = self.report(r) {
                    self.notify(ctx, format!("Saved {p}"));
                }
                Next::Close
            }
            Pick::Row(i) => {
                let (fmt, half, label) = match i {
                    1 => (RenderFormat::Mp4, true, "Rendering MP4"),
                    2 => (RenderFormat::Mp4, false, "Rendering MP4"),
                    _ => (RenderFormat::Gif, true, "Rendering GIF"),
                };
                self.start_render(ctx, c, fmt, half, label);
                Next::Close
            }
        }
    }

    /// Saves a copy of the project and renders it on its own session in a background thread.
    fn start_render(&mut self, ctx: &egui::Context, c: &CompView, fmt: RenderFormat, half: bool, label: &str) {
        let tmp = self.dir("tmp").join(format!("render-{}.ecproj", c.id));
        let copy = self.editor.command("file.saveCopy", serde_json::json!({ "path": tmp.to_string_lossy() }));
        if let Err(e) = copy {
            self.status = Some(e.to_string());
            return;
        }
        let ext = if fmt == RenderFormat::Mp4 { "mp4" } else { "gif" };
        let out = unique(&self.dir("exports"), &safe_name(&c.name), ext);
        let (tx, rx) = std::sync::mpsc::channel();
        let (project, out_s, comp_id) = (tmp.to_string_lossy().into_owned(), out.to_string_lossy().into_owned(), c.id);
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("effectcraft-render".into()).spawn(move || {
            let mut session = effectcraft_host::session();
            let r = crate::ops::render_saved_project(&mut session, &project, comp_id, &out_s, fmt, half).map_err(|e| e.to_string());
            let _ = std::fs::remove_file(&project);
            let _ = tx.send(r);
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => self.job = Some(RenderJob { rx, label: label.to_string() }),
            Err(e) => self.status = Some(format!("could not start the render: {e}")),
        }
    }

    pub(super) fn poll_render(&mut self, ctx: &egui::Context) {
        use std::sync::mpsc::TryRecvError;
        let Some(job) = &self.job else { return };
        match job.rx.try_recv() {
            Ok(Ok(path)) => {
                self.job = None;
                self.notify(ctx, format!("Saved {path}"));
            }
            Ok(Err(e)) => {
                self.job = None;
                self.status = Some(format!("Render failed: {e}"));
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                self.status = Some("The render stopped unexpectedly".into());
            }
        }
    }

    fn sheet_comps(&mut self, ui: &mut Ui, full: ERect, phone: bool, comp: Option<&CompView>, ctx: &egui::Context) -> Next {
        let comps = self.editor.comps().unwrap_or_default();
        let mut items: Vec<Item> = comps
            .iter()
            .map(|c| {
                Item::new(c.name.clone())
                    .sub(format!("{} \u{00d7} {} \u{00b7} {:.1} s \u{00b7} {} layers", c.width, c.height, c.duration, c.layers))
                    .icon(Icon::Film)
                    .selected(c.active)
            })
            .collect();
        items.push(Item::new("New composition\u{2026}").icon(Icon::Plus));
        items.push(Item::new("Composition settings\u{2026}").icon(Icon::Sliders).enabled(comp.is_some()));
        let n = comps.len();
        match self.list_sheet(ui, full, phone, "Compositions", &items, None) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) if i < n => {
                let r = self.editor.open_comp(comps[i].id);
                if self.report(r).is_some() {
                    self.forget_selection_caches();
                    self.playing = false;
                    self.notify(ctx, format!("Opened {}", comps[i].name));
                }
                Next::Close
            }
            Pick::Row(i) if i == n => Next::Open(Sheet::NewComp(CompForm::default_new())),
            Pick::Row(_) => match comp {
                Some(c) => Next::Open(Sheet::CompSettings(CompForm::from(c))),
                None => Next::Close,
            },
        }
    }

    fn sheet_comp_form(&mut self, ui: &mut Ui, full: ERect, phone: bool, form: &mut CompForm, creating: bool, ctx: &egui::Context) -> Next {
        let (body, closed) = frame(ui, full, if creating { "New composition" } else { "Composition settings" }, 400.0, phone, true);
        let mut next = Next::Stay;
        self.scoped(ui, body.shrink2(vec2(tokens::SPACE_4, tokens::SPACE_2)), |_, ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            if creating {
                ui.label(egui::RichText::new("Name").size(12.0).color(col(DARK.text_secondary)));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut form.name)
                        .desired_width(f32::INFINITY)
                        .font(FontId::new(15.0, FontFamily::Proportional))
                        .margin(vec2(10.0, 10.0)),
                );
                r.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, true, "Composition name"));
            }
            ui.label(egui::RichText::new("Preset").size(12.0).color(col(DARK.text_secondary)));
            ui.horizontal_wrapped(|ui| {
                for (label, w, h) in [
                    ("HD 1920\u{00d7}1080", 1920, 1080),
                    ("Vertical 1080\u{00d7}1920", 1080, 1920),
                    ("Square 1080", 1080, 1080),
                    ("4K 3840\u{00d7}2160", 3840, 2160),
                    ("720p", 1280, 720),
                ] {
                    let on = form.width == w && form.height == h;
                    if ui.add(egui::Button::new(egui::RichText::new(label).size(12.5)).selected(on).min_size(vec2(0.0, 36.0))).clicked() {
                        form.width = w;
                        form.height = h;
                    }
                }
            });
            ui.horizontal(|ui| {
                for (label, v) in [("Width", &mut form.width), ("Height", &mut form.height)] {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(label).size(12.0).color(col(DARK.text_secondary)));
                        let mut x = *v;
                        let r = ui.add_sized(vec2(110.0, 40.0), egui::DragValue::new(&mut x).range(16..=8192).speed(2.0).suffix(" px"));
                        r.widget_info(|| WidgetInfo::labeled(WidgetType::DragValue, true, label));
                        *v = x;
                    });
                }
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Frame rate").size(12.0).color(col(DARK.text_secondary)));
                    let r = ui.add_sized(vec2(90.0, 40.0), egui::DragValue::new(&mut form.fps).range(1.0..=120.0).speed(0.1).suffix(" fps"));
                    r.widget_info(|| WidgetInfo::labeled(WidgetType::DragValue, true, "Frame rate"));
                });
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Duration").size(12.0).color(col(DARK.text_secondary)));
                    let r = ui.add_sized(vec2(110.0, 40.0), egui::DragValue::new(&mut form.duration).range(0.1..=3600.0).speed(0.1).suffix(" s"));
                    r.widget_info(|| WidgetInfo::labeled(WidgetType::DragValue, true, "Duration"));
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Frame rate presets").size(12.0).color(col(DARK.text_secondary)));
                    ui.horizontal(|ui| {
                        for fps in [24.0, 30.0, 60.0] {
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new(format!("{fps}")).size(12.5))
                                        .selected((form.fps - fps).abs() < 0.01)
                                        .min_size(vec2(44.0, 36.0)),
                                )
                                .clicked()
                            {
                                form.fps = fps;
                            }
                        }
                    });
                });
            });
            let go = ui.add_sized(
                vec2(ui.available_width(), 46.0),
                egui::Button::new(egui::RichText::new(if creating { "Create" } else { "Apply" }).size(15.0).color(col(DARK.on_accent))).fill(col(DARK.accent)),
            );
            go.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, if creating { "Create composition" } else { "Apply settings" }));
            if go.clicked() {
                next = Next::Close;
            }
        });
        if closed {
            return Next::Close;
        }
        if matches!(next, Next::Close) {
            let r = if creating {
                self.editor.new_comp(&form.name, form.width, form.height, form.fps, form.duration).map(drop)
            } else {
                self.editor.comp_settings(form.width, form.height, form.fps, form.duration)
            };
            if self.report(r).is_some() {
                self.forget_selection_caches();
                self.notify(ctx, if creating { "Composition created" } else { "Composition updated" });
            } else {
                return Next::Stay;
            }
        }
        next
    }

    fn sheet_effects(&mut self, ui: &mut Ui, full: ERect, phone: bool, query: &mut String, comp: Option<&CompView>, ctx: &egui::Context) -> Next {
        let Some(layer) = comp.and_then(|c| self.selected(c)).cloned() else { return Next::Close };
        let catalog = self.catalog();
        let needle = query.trim().to_lowercase();
        let hits: Vec<&EffectInfo> =
            catalog.iter().filter(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle) || e.category.to_lowercase().contains(&needle)).collect();
        let (body, closed) = frame(ui, full, &format!("Add effect to {}", layer.name), 2000.0, phone, false);
        let search = ERect::from_min_size(pos2(body.left() + tokens::SPACE_4, body.top() + 8.0), vec2(body.width() - 2.0 * tokens::SPACE_4, 44.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(search));
        let r = child.add_sized(
            search.size(),
            egui::TextEdit::singleline(query).hint_text("Search effects").font(FontId::new(15.0, FontFamily::Proportional)).margin(vec2(12.0, 11.0)),
        );
        r.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, true, "Search effects"));
        let list = ERect::from_min_max(pos2(body.left(), search.bottom() + 8.0), body.max);
        let mut chosen: Option<String> = None;
        // Rows: a category header whenever the category changes, then the effect.
        let mut rows: Vec<(Option<&str>, Option<&EffectInfo>)> = Vec::new();
        let mut last = "";
        for e in &hits {
            if e.category != last {
                rows.push((Some(e.category.as_str()), None));
                last = e.category.as_str();
            }
            rows.push((None, Some(e)));
        }
        self.scoped(ui, list, |_, ui| {
            egui::ScrollArea::vertical().id_salt("effects-list").auto_shrink([false, false]).show_rows(ui, 44.0, rows.len(), |ui, range| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for i in range {
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
                    match rows[i] {
                        (Some(cat), _) => {
                            ui.painter().rect_filled(rect, 0.0, col(DARK.header));
                            ui.painter().text(
                                pos2(rect.left() + tokens::SPACE_4, rect.center().y),
                                Align2::LEFT_CENTER,
                                cat.to_uppercase(),
                                FontId::new(11.0, semibold()),
                                col(DARK.text_secondary),
                            );
                        }
                        (_, Some(e)) => {
                            let it = Item::new(e.name.clone());
                            let rr = ERect::from_min_size(rect.min, vec2(rect.width(), 44.0));
                            if item_row(ui, rr, Id::new(("fx-pick", &e.id)), &it).clicked() {
                                chosen = Some(e.id.clone());
                            }
                        }
                        _ => {}
                    }
                }
                if rows.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| ui.label(egui::RichText::new("No effect matches").color(col(DARK.text_secondary))));
                }
            });
        });
        if closed {
            return Next::Close;
        }
        if let Some(id) = chosen {
            let r = self.editor.apply_effect(layer.id, &id);
            if let Some(path) = self.report(r) {
                let name = catalog.iter().find(|e| e.id == id).map_or("Effect", |e| e.name.as_str()).to_string();
                self.expanded.insert(path);
                self.panel_tab = PanelTab::Effects;
                self.layout.inspector_open = true;
                self.notify(ctx, format!("{name} applied"));
            }
            return Next::Close;
        }
        Next::Stay
    }

    #[allow(clippy::too_many_arguments)]
    fn sheet_input(
        &mut self,
        ui: &mut Ui,
        full: ERect,
        phone: bool,
        kind: &InputKind,
        value: &mut String,
        title: &str,
        multiline: bool,
        comp: Option<&CompView>,
        ctx: &egui::Context,
    ) -> Next {
        let _ = comp;
        let (body, closed) = frame(ui, full, title, if multiline { 220.0 } else { 150.0 }, phone, true);
        let mut ok = false;
        let mut cancel = false;
        self.scoped(ui, body.shrink2(vec2(tokens::SPACE_4, tokens::SPACE_2)), |_, ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 10.0);
            let te = if multiline {
                egui::TextEdit::multiline(value).desired_rows(4).desired_width(f32::INFINITY)
            } else {
                egui::TextEdit::singleline(value).desired_width(f32::INFINITY)
            };
            let r = ui.add(te.font(FontId::new(16.0, FontFamily::Proportional)).margin(vec2(12.0, 11.0)));
            r.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, true, "Value"));
            if !r.has_focus() && ui.ctx().memory(|m| m.focused().is_none()) {
                r.request_focus();
            }
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.0) / 2.0;
                if ui.add_sized(vec2(w, 46.0), egui::Button::new(egui::RichText::new("Cancel").size(15.0))).clicked() {
                    cancel = true;
                }
                let go = ui.add_sized(vec2(w, 46.0), egui::Button::new(egui::RichText::new("OK").size(15.0).color(col(DARK.on_accent))).fill(col(DARK.accent)));
                go.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "OK"));
                if go.clicked() {
                    ok = true;
                }
            });
        });
        if closed || cancel {
            return Next::Close;
        }
        if !ok {
            return Next::Stay;
        }
        let r = match kind {
            InputKind::Rename(l) => self.editor.rename_layer(*l, value),
            InputKind::Text(l) => self.editor.set_text(*l, value),
            InputKind::Str(l, path) => self.editor.set_str(*l, path, value),
            InputKind::SaveAs => {
                let out = self.dir("projects").join(format!("{}.ecproj", safe_name(value)));
                match self.editor.save(Some(&out.to_string_lossy())) {
                    Ok(p) => {
                        self.notify(ctx, format!("Saved {p}"));
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
        };
        if self.report(r).is_some() { Next::Close } else { Next::Stay }
    }

    #[allow(clippy::too_many_arguments)]
    fn sheet_choice(&mut self, ui: &mut Ui, full: ERect, phone: bool, layer: u64, path: &str, title: &str, options: &[(i64, String)], current: i64) -> Next {
        let items: Vec<Item> = options.iter().map(|(v, l)| Item::new(l.clone()).selected(*v == current)).collect();
        match self.list_sheet(ui, full, phone, title, &items, None) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                if let Some((v, _)) = options.get(i) {
                    let r = self.editor.set_index(layer, path, *v);
                    self.report(r);
                }
                Next::Close
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sheet_prop(&mut self, ui: &mut Ui, full: ERect, phone: bool, layer: u64, path: &str, name: &str, animated: bool, ctx: &egui::Context) -> Next {
        let items = vec![
            Item::new("Reset to default").sub("Removes keyframes and expressions on this property"),
            Item::new(if animated { "Add or remove keyframe at playhead" } else { "Add keyframe at playhead" }).icon(Icon::Diamond),
            Item::new("Previous keyframe").icon(Icon::KeyPrev).enabled(animated),
            Item::new("Next keyframe").icon(Icon::KeyNext).enabled(animated),
        ];
        match self.list_sheet(ui, full, phone, name, &items, None) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                let r = match i {
                    0 => self.editor.reset_prop(layer, path),
                    1 => self.editor.toggle_key(layer, path),
                    2 => self.editor.go_to_prop_key(layer, path, false),
                    _ => self.editor.go_to_prop_key(layer, path, true),
                };
                if self.report(r).is_some() && i == 0 {
                    self.notify(ctx, format!("{name} reset"));
                }
                Next::Close
            }
        }
    }
}
