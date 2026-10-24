//! The ease editor: the curve between a selected keyframe and its neighbour, with a draggable
//! handle at each end. Drag a handle and the engine's `keys.setEase` follows; presets set both
//! handles at once. Works on any animated property, one dimension or several.

use super::*;
use crate::curve::{self, Pt};
use crate::ops::KeyEase;

/// The part of the unit square's surroundings shown, as `(low, high)` progress.
const VIEW_Y: (f64, f64) = (-0.5, 1.5);

/// Which two keys the editor shapes and how a drag is being merged into one undo step.
pub(in crate::ui) struct EaseState {
    layer: u64,
    path: String,
    prop: u64,
    /// The earlier and later key of the segment, in layer seconds.
    a: f64,
    b: f64,
    drags: u32,
}

/// The segment as the editor sees it: both keys, the length, and the change per dimension.
struct Segment {
    a: KeyEase,
    b: KeyEase,
    dur: f64,
    /// Signed value change per eased dimension (the path length for a spatial property).
    dv: Vec<f64>,
    lead: usize,
}

impl Segment {
    fn read(app: &mut MobileApp, st: &EaseState) -> Option<Segment> {
        let a = app.editor.key_ease(st.layer, st.prop, st.a).ok()?;
        let b = app.editor.key_ease(st.layer, st.prop, st.b).ok()?;
        let dims = a.dims();
        let dv: Vec<f64> = if a.spatial {
            let d = a.value.iter().zip(&b.value).map(|(x, y)| (y - x) * (y - x)).sum::<f64>().sqrt();
            vec![d]
        } else {
            (0..dims).map(|i| b.value.get(i).copied().unwrap_or(0.0) - a.value.get(i).copied().unwrap_or(0.0)).collect()
        };
        let lead = dv.iter().enumerate().max_by(|x, y| x.1.abs().total_cmp(&y.1.abs())).map(|(i, _)| i).unwrap_or(0);
        let dur = b.time - a.time;
        Some(Segment { a, b, dur, dv, lead })
    }

    /// Both handles in the unit square, or `None` when the keys have the same value.
    fn handles(&self) -> Option<(Pt, Pt)> {
        let lead = self.lead;
        let (si, ii) = *self.a.out_ease.get(lead)?;
        let (so, io) = *self.b.in_ease.get(lead)?;
        let dv = *self.dv.get(lead)?;
        Some((curve::out_handle(ii, si, self.dur, dv)?, curve::in_handle(io, so, self.dur, dv)?))
    }
}

fn clamp_view(p: Pt) -> Pt {
    Pt { x: p.x.clamp(0.0, 1.0), y: p.y.clamp(VIEW_Y.0, VIEW_Y.1) }
}

/// A preset as the two handle positions it puts in the unit square.
struct Preset {
    label: &'static str,
    out: Pt,
    into: Pt,
}

const PRESETS: [Preset; 4] = [
    Preset { label: "Easy Ease", out: Pt { x: 1.0 / 3.0, y: 0.0 }, into: Pt { x: 2.0 / 3.0, y: 1.0 } },
    Preset { label: "Linear", out: Pt { x: 1.0 / 3.0, y: 1.0 / 3.0 }, into: Pt { x: 2.0 / 3.0, y: 2.0 / 3.0 } },
    Preset { label: "Ease in", out: Pt { x: 0.5, y: 0.0 }, into: Pt { x: 0.8, y: 0.8 } },
    Preset { label: "Ease out", out: Pt { x: 0.2, y: 0.2 }, into: Pt { x: 0.5, y: 1.0 } },
];

impl MobileApp {
    /// Opens the ease editor on the selected keyframe and the one after it (or before, for the last key).
    pub(in crate::ui) fn open_ease_editor(&mut self) {
        let Some(sel) = self.sel_key.clone() else { return };
        let Ok(tree) = self.editor.layer_tree(sel.layer, 8) else { return };
        let Some(node) = tree.find(&sel.path) else { return };
        let mut keys = node.keys.clone();
        keys.sort_by(|x, y| x.total_cmp(y));
        let Some(i) = keys.iter().enumerate().min_by(|x, y| (x.1 - sel.time).abs().total_cmp(&(y.1 - sel.time).abs())).map(|(i, _)| i) else { return };
        let (a, b) = match (keys.get(i), keys.get(i + 1), i.checked_sub(1).and_then(|p| keys.get(p))) {
            (Some(a), Some(b), _) => (*a, *b),
            (Some(b), None, Some(a)) => (*a, *b),
            _ => {
                self.status = Some("Add a second keyframe to shape the curve between them".into());
                return;
            }
        };
        self.sheet = Some(Sheet::Ease(EaseState { layer: sel.layer, path: sel.path.clone(), prop: node.uid, a, b, drags: 0 }));
    }

    /// Moves one handle of the open ease editor to `(x, y)` in the unit square, as engine commands.
    fn move_ease_handle(&mut self, st: &mut EaseState, out: bool, p: Pt) -> bool {
        let Some(seg) = Segment::read(self, st) else { return false };
        let Some(&lead_dv) = seg.dv.get(seg.lead) else { return false };
        let p = clamp_view(p);
        let Some((influence, _)) = (if out { curve::out_ease(p, seg.dur, lead_dv) } else { curve::in_ease(p, seg.dur, lead_dv) }) else { return false };
        let (x, y) = if out { (influence / 100.0, p.y) } else { (influence / 100.0, 1.0 - p.y) };
        let (key, time) = if out { (&seg.a, st.a) } else { (&seg.b, st.b) };
        let merge = format!("ease-{}-{}", st.prop, st.drags);
        let multi = key.dims() > 1;
        let mut ok = true;
        for (d, dv) in seg.dv.iter().enumerate() {
            // A dimension with no change has no speed to speak of: leave it as it is.
            if dv.abs() < 1e-9 {
                continue;
            }
            let speed = y * dv / (x * seg.dur);
            let r = self.editor.set_key_ease(st.layer, &st.path, time, out, if multi { Some(d) } else { None }, speed, influence, &merge);
            ok &= self.report(r).is_some();
        }
        ok
    }

    fn apply_preset(&mut self, st: &mut EaseState, preset: &Preset) {
        st.drags += 1;
        self.move_ease_handle(st, true, preset.out);
        self.move_ease_handle(st, false, preset.into);
    }

    /// Test and automation hook: drag a handle of the open ease editor to a point of the unit square.
    pub fn drag_ease_handle(&mut self, out: bool, x: f64, y: f64) -> bool {
        let Some(Sheet::Ease(mut st)) = self.sheet.take() else { return false };
        st.drags += 1;
        let done = self.move_ease_handle(&mut st, out, Pt { x, y });
        self.sheet = Some(Sheet::Ease(st));
        done
    }

    /// Where the open ease editor's handles are, as `(outgoing, incoming)` in the unit square.
    pub fn ease_handles(&mut self) -> Option<((f64, f64), (f64, f64))> {
        let Some(Sheet::Ease(st)) = self.sheet.take() else { return None };
        let seg = Segment::read(self, &st);
        self.sheet = Some(Sheet::Ease(st));
        seg?.handles().map(|(o, i)| ((o.x, o.y), (i.x, i.y)))
    }

    pub(super) fn sheet_ease(&mut self, ui: &mut Ui, full: ERect, phone: bool, st: &mut EaseState) -> Next {
        let width = if phone { full.width() } else { 480.0_f32.min(full.width() - 32.0) };
        let side = (width - 2.0 * tokens::SPACE_4).clamp(160.0, 320.0);
        let body_h = side + 16.0 + 56.0 + 64.0;
        let (body, closed) = frame(ui, full, "Ease", body_h, phone, false);
        let seg = Segment::read(self, st);
        let handles = seg.as_ref().and_then(Segment::handles);
        let plot = ERect::from_center_size(pos2(body.center().x, body.top() + 8.0 + side / 2.0), vec2(side, side));
        ui.painter().rect_filled(plot, tokens::RADIUS_MD, col(DARK.header));
        // Unit square -> screen, and back.
        let span = (VIEW_Y.1 - VIEW_Y.0) as f32;
        let sx = |x: f64| plot.left() + 24.0 + x as f32 * (plot.width() - 48.0);
        let sy = |y: f64| plot.bottom() - 12.0 - ((y - VIEW_Y.0) as f32 / span) * (plot.height() - 24.0);
        let unit = |pos: egui::Pos2| Pt {
            x: ((pos.x - (plot.left() + 24.0)) / (plot.width() - 48.0)) as f64,
            y: VIEW_Y.0 + ((plot.bottom() - 12.0 - pos.y) / (plot.height() - 24.0)) as f64 * span as f64,
        };
        let grid = Stroke::new(1.0, col(DARK.border));
        for y in [0.0, 1.0] {
            ui.painter().line_segment([pos2(sx(0.0), sy(y)), pos2(sx(1.0), sy(y))], grid);
        }
        for x in [0.0, 1.0] {
            ui.painter().line_segment([pos2(sx(x), sy(VIEW_Y.0)), pos2(sx(x), sy(VIEW_Y.1))], grid);
        }
        let mut readout = String::from("These keyframes have the same value, so there is no curve to shape.");
        match (&seg, handles) {
            (Some(seg), Some((p1, p2))) => {
                let pts: Vec<egui::Pos2> = (0..=48).map(|i| curve::sample(p1, p2, i as f64 / 48.0)).map(|p| pos2(sx(p.x), sy(p.y))).collect();
                ui.painter().add(egui::Shape::line(pts, Stroke::new(3.0, col(DARK.accent))));
                let (a, b) = (pos2(sx(0.0), sy(0.0)), pos2(sx(1.0), sy(1.0)));
                let (h1, h2) = (pos2(sx(p1.x), sy(p1.y)), pos2(sx(p2.x), sy(p2.y)));
                let guide = Stroke::new(1.5, col(DARK.text_secondary));
                ui.painter().line_segment([a, h1], guide);
                ui.painter().line_segment([b, h2], guide);
                for end in [a, b] {
                    ui.painter().circle_filled(end, 5.0, col(DARK.text_primary));
                }
                for (out, h, label) in [(true, h1, "Outgoing handle"), (false, h2, "Incoming handle")] {
                    let hit = ERect::from_center_size(h, vec2(52.0, 52.0));
                    let r = ui.interact(hit, Id::new(("ease-handle", out)), Sense::drag());
                    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
                    let live = r.dragged() || r.is_pointer_button_down_on();
                    ui.painter().circle_filled(h, if live { 14.0 } else { 11.0 }, col(DARK.accent));
                    ui.painter().circle_stroke(h, if live { 14.0 } else { 11.0 }, Stroke::new(2.0, col(DARK.text_primary)));
                    if r.drag_started() {
                        st.drags += 1;
                    }
                    if r.dragged()
                        && let Some(pos) = r.interact_pointer_pos()
                    {
                        self.move_ease_handle(st, out, unit(pos));
                    }
                }
                let (si, ii) = seg.a.out_ease.get(seg.lead).copied().unwrap_or_default();
                let (so, io) = seg.b.in_ease.get(seg.lead).copied().unwrap_or_default();
                let units = if seg.a.units.is_empty() { "units/sec".to_string() } else { seg.a.units.clone() };
                readout = format!("Out {ii:.0}% at {si:.1} {units}   \u{00b7}   In {io:.0}% at {so:.1} {units}");
            }
            _ => {
                ui.painter().text(
                    plot.center(),
                    Align2::CENTER_CENTER,
                    "No curve to shape",
                    FontId::new(13.0, FontFamily::Proportional),
                    col(DARK.text_secondary),
                );
            }
        }
        let info = ERect::from_min_size(pos2(body.left() + tokens::SPACE_4, plot.bottom() + 6.0), vec2(width - 2.0 * tokens::SPACE_4, 40.0));
        let galley = ui.painter().layout(readout, FontId::new(12.0, FontFamily::Proportional), col(DARK.text_secondary), info.width());
        ui.painter().galley(pos2(info.left(), info.center().y - galley.size().y / 2.0), galley, col(DARK.text_secondary));
        // Presets: a row of equal buttons.
        let row_top = info.bottom() + 8.0;
        let gap = 8.0;
        let w = (width - 2.0 * tokens::SPACE_4 - 3.0 * gap) / 4.0;
        for (i, p) in PRESETS.iter().enumerate() {
            let rect = ERect::from_min_size(pos2(body.left() + tokens::SPACE_4 + i as f32 * (w + gap), row_top), vec2(w, 44.0));
            let r = ui.interact(rect, Id::new(("ease-preset", i)), Sense::click());
            let label = format!("Curve: {}", p.label);
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, handles.is_some(), label.clone()));
            let fill = if r.is_pointer_button_down_on() { col(DARK.row_selected) } else { col(DARK.header) };
            ui.painter().rect_filled(rect, tokens::RADIUS_MD, fill);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                p.label,
                FontId::new(12.5, medium()),
                if handles.is_some() { col(DARK.text_primary) } else { col(DARK.text_disabled) },
            );
            if r.clicked() && handles.is_some() {
                self.apply_preset(st, p);
            }
        }
        if closed { Next::Close } else { Next::Stay }
    }
}
