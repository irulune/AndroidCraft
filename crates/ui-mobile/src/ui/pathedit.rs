//! Editing the points of a mask or shape path on the picture.
//!
//! While a path is being edited the picture shows its outline, a handle on every point and, for the
//! selected point, its two curve handles. Dragging a point or a handle moves it; tapping an edge
//! adds a point there; tapping a point selects it; the bar at the bottom deletes the point, turns it
//! smooth or into a corner, opens or closes the path, and says when the work is done. Every change
//! is an engine command (`mask.setVertex`, `mask.insertVertex`, ...), so it is undoable and a drag
//! is one undo step.

use effectcraft_engine::geom::vec2 as gvec2;

use super::*;
use crate::ops::PathData;
use crate::pathedit::{self, Hit, P};

/// How close a finger has to be to a point, handle or edge, in points.
const REACH: f64 = 26.0;

/// What a drag in progress holds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Vertex(usize),
    Handle { vertex: usize, out: bool },
}

/// The path being edited.
pub(super) struct PathEdit {
    layer: u64,
    /// The mask's or shape path's id (what the point commands take).
    group: u64,
    /// Address of the path property (`@id`), for reading its points.
    prop: String,
    selected: Option<usize>,
    grab: Option<Grab>,
    /// The points as last read, with the edit and time they belong to.
    cache: Option<((u64, i64), PathData)>,
}

impl MobileApp {
    /// Starts editing the points of a path. `group` is the mask's or shape path's id and `prop` the id of its
    /// path property. Selects the layer, so the picture shows it.
    pub fn edit_points(&mut self, layer: u64, group: u64, prop: u64) -> bool {
        let r = self.editor.select_layer(layer);
        if self.report(r).is_none() {
            return false;
        }
        self.sheet = None;
        self.path_edit = Some(PathEdit { layer, group, prop: format!("@{prop}"), selected: None, grab: None, cache: None });
        true
    }

    /// True while a path's points are being edited.
    pub fn path_editing(&self) -> bool {
        self.path_edit.is_some()
    }

    /// Leaves point editing.
    pub fn stop_editing_points(&mut self) {
        self.path_edit = None;
    }

    /// The path's points now, read again only after an edit or a change of time.
    fn path_points(&mut self, time: f64) -> Option<PathData> {
        let stamp = (self.editor.session().revision, (time * 1000.0).round() as i64);
        let edit = self.path_edit.as_ref()?;
        if let Some((s, d)) = &edit.cache
            && *s == stamp
        {
            return Some(d.clone());
        }
        let (layer, prop) = (edit.layer, edit.prop.clone());
        let data = self.editor.path_data(layer, &prop).ok()?;
        if let Some(e) = self.path_edit.as_mut() {
            e.cache = Some((stamp, data.clone()));
        }
        Some(data)
    }

    /// The outline, points and handles, and what a finger does to them.
    pub(super) fn path_overlay(&mut self, ui: &mut Ui, img: ERect, comp: &CompView) {
        let Some((layer, group)) = self.path_edit.as_ref().map(|e| (e.layer, e.group)) else { return };
        // The layer was deselected or the path deleted: nothing left to edit.
        let pm = if self.selected(comp).map(|l| l.id) == Some(layer) { crate::pick::path_to_comp(self.editor.session(), layer, group) } else { None };
        let (Some(pm), Some(path)) = (pm, self.path_points(comp.time)) else {
            self.path_edit = None;
            return;
        };
        let Some(inv) = pm.inverse() else { return };
        let (sx, sy) = (f64::from(img.width()) / f64::from(comp.width.max(1)), f64::from(img.height()) / f64::from(comp.height.max(1)));
        let (ox, oy) = (f64::from(img.left()), f64::from(img.top()));
        let to_screen = |p: P| {
            let c = pm.apply(gvec2(p[0], p[1]));
            [ox + c.x * sx, oy + c.y * sy]
        };
        let to_path = |s: P| {
            let q = inv.apply(gvec2((s[0] - ox) / sx, (s[1] - oy) / sy));
            [q.x, q.y]
        };

        let resp = ui.interact(img, Id::new("path-edit"), Sense::click_and_drag());
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Path editor"));
        let selected = self.path_edit.as_ref().and_then(|e| e.selected);
        if resp.drag_started() {
            let origin = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos());
            let grab = origin.and_then(|o| match pathedit::hit(&path, selected, [f64::from(o.x), f64::from(o.y)], REACH, &to_screen) {
                Some(Hit::Vertex(i)) => Some(Grab::Vertex(i)),
                Some(Hit::Handle { vertex, out }) => Some(Grab::Handle { vertex, out }),
                _ => None,
            });
            if let Some(e) = self.path_edit.as_mut() {
                e.grab = grab;
                if let Some(Grab::Vertex(i)) = grab {
                    e.selected = Some(i);
                }
            }
            if grab.is_some() {
                self.editor.begin_gesture();
            }
        }
        let grab = self.path_edit.as_ref().and_then(|e| e.grab);
        if resp.dragged()
            && let Some(grab) = grab
            && let Some(at) = resp.interact_pointer_pos()
        {
            let r = match grab {
                Grab::Vertex(i) => {
                    let d = resp.drag_delta();
                    let dp = inv.apply_vec(gvec2(f64::from(d.x) / sx, f64::from(d.y) / sy));
                    match path.vertices.get(i) {
                        Some(v) => self.editor.set_vertex(layer, group, i, Some([v[0] + dp.x, v[1] + dp.y]), None),
                        None => Ok(()),
                    }
                }
                Grab::Handle { vertex, out } => match path.vertices.get(vertex) {
                    Some(v) => {
                        let target = to_path([f64::from(at.x), f64::from(at.y)]);
                        let t = [target[0] - v[0], target[1] - v[1]];
                        let (cur_in, cur_out) = (path.ins.get(vertex).copied().unwrap_or([0.0; 2]), path.outs.get(vertex).copied().unwrap_or([0.0; 2]));
                        let mirrored = pathedit::handles_mirrored(&path, vertex);
                        let opposite = [-t[0], -t[1]];
                        let (new_in, new_out) = match (out, mirrored) {
                            (true, true) => (opposite, t),
                            (true, false) => (cur_in, t),
                            (false, true) => (t, opposite),
                            (false, false) => (t, cur_out),
                        };
                        self.editor.set_vertex(layer, group, vertex, None, Some((new_in, new_out)))
                    }
                    None => Ok(()),
                },
            };
            self.report(r);
        }
        if resp.drag_stopped() {
            if grab.is_some() {
                self.editor.end_gesture();
            }
            if let Some(e) = self.path_edit.as_mut() {
                e.grab = None;
            }
        }
        if resp.clicked()
            && let Some(at) = resp.interact_pointer_pos()
        {
            match pathedit::hit(&path, selected, [f64::from(at.x), f64::from(at.y)], REACH, &to_screen) {
                Some(Hit::Vertex(i)) => self.select_point(Some(i)),
                Some(Hit::Edge { segment, t }) => {
                    let r = self.editor.insert_vertex(layer, group, segment, t);
                    if let Some(i) = self.report(r) {
                        self.select_point(Some(i));
                    }
                }
                Some(Hit::Handle { .. }) => {}
                None => self.select_point(None),
            }
        }

        // Draw what is there now (the edits above may have changed it).
        let path = self.path_points(comp.time).unwrap_or(path);
        let selected = self.path_edit.as_ref().and_then(|e| e.selected).filter(|i| *i < path.vertices.len());
        let screen = |p: P| {
            let s = to_screen(p);
            pos2(s[0] as f32, s[1] as f32)
        };
        let clip = ui.painter().with_clip_rect(img);
        let line: Vec<Pos2> = pathedit::polyline(&path, &to_screen).into_iter().map(|s| pos2(s[0] as f32, s[1] as f32)).collect();
        if line.len() > 1 {
            clip.add(Shape::line(line.clone(), Stroke::new(4.0, Color32::from_black_alpha(160))));
            clip.add(Shape::line(line, Stroke::new(1.8, col(DARK.accent))));
        }
        if let Some(i) = selected
            && let Some(v) = path.vertices.get(i)
        {
            for (tg, name) in [(path.ins.get(i), "in"), (path.outs.get(i), "out")] {
                let Some(tg) = tg.filter(|t| **t != [0.0; 2]) else { continue };
                let (a, b) = (screen(*v), screen([v[0] + tg[0], v[1] + tg[1]]));
                clip.line_segment([a, b], Stroke::new(1.5, col(DARK.accent)));
                clip.circle_filled(b, 6.0, Color32::WHITE);
                clip.circle_stroke(b, 6.0, Stroke::new(1.5, col(DARK.accent)));
                // Named for tests and screen readers.
                let r = ui.interact(ERect::from_center_size(b, vec2(28.0, 28.0)), Id::new(("path-handle", name)), Sense::hover());
                r.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, format!("Handle {name}")));
            }
        }
        for (i, v) in path.vertices.iter().enumerate() {
            let at = screen(*v);
            let on = selected == Some(i);
            let size = if on { 8.0 } else { 6.0 };
            if path.is_smooth(i) {
                clip.circle_filled(at, size, if on { col(DARK.accent) } else { Color32::WHITE });
                clip.circle_stroke(at, size, Stroke::new(1.5, if on { Color32::WHITE } else { Color32::from_black_alpha(200) }));
            } else {
                let r = ERect::from_center_size(at, vec2(size * 2.0, size * 2.0));
                clip.rect_filled(r, 1.0, if on { col(DARK.accent) } else { Color32::WHITE });
                clip.rect_stroke(r, 1.0, Stroke::new(1.5, if on { Color32::WHITE } else { Color32::from_black_alpha(200) }), StrokeKind::Outside);
            }
            let r = ui.interact(ERect::from_center_size(at, vec2(28.0, 28.0)), Id::new(("path-point", i)), Sense::hover());
            r.widget_info(|| WidgetInfo::selected(WidgetType::Other, true, on, format!("Point {}", i + 1)));
        }
    }

    fn select_point(&mut self, i: Option<usize>) {
        if let Some(e) = self.path_edit.as_mut() {
            e.selected = i;
        }
    }

    /// Delete, smooth or corner, open or close, and Done: the bar over the bottom of the picture.
    pub(super) fn path_toolbar(&mut self, ui: &mut Ui, board: ERect, comp: &CompView) {
        let Some((layer, group, selected)) = self.path_edit.as_ref().map(|e| (e.layer, e.group, e.selected)) else { return };
        let path = self.path_points(comp.time).unwrap_or_default();
        let selected = selected.filter(|i| *i < path.vertices.len());
        let smooth = selected.is_some_and(|i| path.is_smooth(i));
        // (shown, spoken, enabled)
        let buttons = [
            ("Done", "Done editing points".to_string(), true),
            ("Delete", "Delete point".to_string(), selected.is_some() && path.vertices.len() > 1),
            (if smooth { "Corner" } else { "Smooth" }, if smooth { "Make corner" } else { "Make smooth" }.to_string(), selected.is_some()),
            (if path.closed { "Open" } else { "Close" }, if path.closed { "Open path" } else { "Close path" }.to_string(), path.vertices.len() > 2),
        ];
        let seg = vec2(76.0, tokens::MIN_TOUCH_TARGET);
        let w = seg.x * buttons.len() as f32;
        let bar = ERect::from_min_size(pos2(board.center().x - w / 2.0, board.bottom() - tokens::SPACE_2 - seg.y), vec2(w, seg.y));
        ui.painter().rect_filled(bar, seg.y / 2.0, col(DARK.surface_overlay).gamma_multiply(0.94));
        ui.painter().rect_stroke(bar, seg.y / 2.0, Stroke::new(1.0, col(DARK.border)), StrokeKind::Inside);
        let mut pressed = None;
        for (i, (shown, spoken, enabled)) in buttons.iter().enumerate() {
            let rect = ERect::from_min_size(bar.min + vec2(i as f32 * seg.x, 0.0), seg);
            let r = ui.interact(rect, Id::new(("path-tool", i)), if *enabled { Sense::click() } else { Sense::hover() });
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, *enabled, spoken.clone()));
            if i == 0 {
                ui.painter().rect_filled(rect.shrink(3.0), (seg.y - 6.0) / 2.0, col(DARK.accent));
            } else if *enabled && r.is_pointer_button_down_on() {
                ui.painter().rect_filled(rect.shrink(3.0), (seg.y - 6.0) / 2.0, col(DARK.row_selected));
            }
            let ink = if i == 0 {
                col(DARK.on_accent)
            } else if *enabled {
                col(DARK.text_primary)
            } else {
                col(DARK.text_disabled)
            };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, *shown, FontId::new(13.0, medium()), ink);
            if r.clicked() {
                pressed = Some(i);
            }
        }
        let result = match (pressed, selected) {
            (Some(0), _) => {
                self.path_edit = None;
                return;
            }
            (Some(1), Some(i)) => {
                self.select_point(None);
                Some(self.editor.delete_vertex(layer, group, i))
            }
            (Some(2), Some(i)) => Some(self.editor.smooth_vertex(layer, group, i, !smooth)),
            (Some(3), _) => Some(self.editor.set_path_closed(layer, group, !path.closed)),
            _ => None,
        };
        if let Some(r) = result {
            self.report(r);
        }
    }
}
