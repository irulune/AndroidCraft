//! The gradient editor: the bar with its colour stops below and opacity stops above. Tap the bar's
//! edge rows to add a stop, tap a stop to select it, drag it to move it; the selected stop's
//! location, opacity or colour is edited underneath, and it can be deleted (never the last of its
//! row). Each change is one `prop.set` of the whole gradient, so a drag is one undo step.

use super::*;
use crate::gradient::{GradientData, Track};

/// The gradient being edited and the selected stop.
pub(in crate::ui) struct GradientState {
    layer: u64,
    /// Address of the gradient property (`@id`).
    path: String,
    name: String,
    selected: Option<(Track, usize)>,
}

/// Paints a gradient over a checkerboard, so transparency shows.
pub(in crate::ui) fn paint_gradient(p: &Painter, rect: ERect, g: &GradientData) {
    let cell = 8.0;
    let (cols, rows) = ((rect.width() / cell).ceil() as i32, (rect.height() / cell).ceil() as i32);
    let clip = p.with_clip_rect(rect);
    for r in 0..rows {
        for c in 0..cols {
            let shade = if (r + c) % 2 == 0 { 70 } else { 105 };
            let cr = ERect::from_min_size(pos2(rect.left() + c as f32 * cell, rect.top() + r as f32 * cell), vec2(cell, cell));
            clip.rect_filled(cr, 0.0, Color32::from_gray(shade));
        }
    }
    let n = ((rect.width() / 3.0).ceil() as usize).clamp(8, 160);
    for i in 0..n {
        let t = (i as f64 + 0.5) / n as f64;
        let c = g.sample(t);
        let slice = ERect::from_min_max(
            pos2(rect.left() + rect.width() * i as f32 / n as f32, rect.top()),
            pos2(rect.left() + rect.width() * (i + 1) as f32 / n as f32 + 0.5, rect.bottom()),
        );
        clip.rect_filled(slice, 0.0, Color32::from_rgba_unmultiplied((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (c[3] * 255.0) as u8));
    }
}

fn rgb32(c: [f32; 4]) -> Color32 {
    Color32::from_rgb((c[0].clamp(0.0, 1.0) * 255.0).round() as u8, (c[1].clamp(0.0, 1.0) * 255.0).round() as u8, (c[2].clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl MobileApp {
    /// Opens the gradient editor on a gradient property (`path` is its address, such as `@12`).
    pub fn edit_gradient(&mut self, layer: u64, path: &str, name: &str) {
        self.sheet = Some(Sheet::Gradient(GradientState { layer, path: path.to_string(), name: name.to_string(), selected: None }));
    }

    pub(super) fn sheet_gradient(&mut self, ui: &mut Ui, full: ERect, phone: bool, st: &mut GradientState) -> Next {
        let width = if phone { full.width() } else { 480.0_f32.min(full.width() - 32.0) };
        let (body, closed) = frame(ui, full, &st.name.clone(), 330.0, phone, false);
        let Ok(mut g) = self.editor.gradient(st.layer, &st.path) else { return Next::Close };
        let pad = tokens::SPACE_4 + 8.0;
        let left = body.center().x - width / 2.0 + pad;
        let bar_w = width - 2.0 * pad;
        let top = body.top() + 8.0;
        let (op_row, bar, col_row) = (
            ERect::from_min_size(pos2(left, top), vec2(bar_w, 34.0)),
            ERect::from_min_size(pos2(left, top + 34.0), vec2(bar_w, 40.0)),
            ERect::from_min_size(pos2(left, top + 74.0), vec2(bar_w, 38.0)),
        );
        paint_gradient(ui.painter(), bar, &g);
        ui.painter().rect_stroke(bar, 4.0, Stroke::new(1.0, col(DARK.border)), StrokeKind::Outside);
        let to_pos = |x: f32| f64::from(((x - left) / bar_w.max(1.0)).clamp(0.0, 1.0));
        let mut changed = false;

        for (track, row, label) in [(Track::Opacity, op_row, "Add opacity stop"), (Track::Color, col_row, "Add colour stop")] {
            let r = ui.interact(row, Id::new(("gradient-track", label)), Sense::click());
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
            if r.clicked() {
                // A tap lands where the finger is; an assistive click has no position and lands in the middle.
                let at = r.interact_pointer_pos().map_or(0.5, |p| to_pos(p.x));
                let i = g.add(track, at);
                st.selected = Some((track, i));
                changed = true;
            }
        }

        for track in [Track::Opacity, Track::Color] {
            for i in 0..g.len(track) {
                let Some(pos) = g.position(track, i) else { continue };
                let x = left + bar_w * pos as f32;
                let on = st.selected == Some((track, i));
                let up = track == Track::Color;
                let (y0, y1) = if up { (bar.bottom() + 1.0, bar.bottom() + 17.0) } else { (bar.top() - 1.0, bar.top() - 17.0) };
                let fill = match track {
                    Track::Color => rgb32(g.colors.get(i).map_or([1.0; 4], |s| s.1)),
                    Track::Opacity => Color32::from_gray((g.opacities.get(i).map_or(1.0, |s| s.1).clamp(0.0, 1.0) * 255.0) as u8),
                };
                let outline = Stroke::new(if on { 2.5 } else { 1.5 }, if on { col(DARK.accent) } else { Color32::WHITE });
                let tri = vec![pos2(x, y0), pos2(x - 9.0, y1), pos2(x + 9.0, y1)];
                ui.painter().add(Shape::convex_polygon(tri, fill, outline));
                let hit = ERect::from_center_size(pos2(x, (y0 + y1) / 2.0), vec2(44.0, 44.0));
                let name = format!("{} stop {}", if up { "Colour" } else { "Opacity" }, i + 1);
                let r = ui.interact(hit, Id::new(("gradient-stop", up, i)), Sense::click_and_drag());
                r.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, name.clone()));
                if r.clicked() || r.drag_started() {
                    st.selected = Some((track, i));
                }
                if r.drag_started() {
                    self.editor.begin_gesture();
                }
                if r.dragged()
                    && let Some(p) = r.interact_pointer_pos()
                    && st.selected.map(|s| s.0) == Some(track)
                {
                    let from = st.selected.map_or(i, |s| s.1);
                    let now = g.move_to(track, from, to_pos(p.x));
                    st.selected = Some((track, now));
                    changed = true;
                }
                if r.drag_stopped() {
                    self.editor.end_gesture();
                }
            }
        }

        // The selected stop's details.
        let detail_top = col_row.bottom() + 14.0;
        let sel = st.selected.filter(|(t, i)| *i < g.len(*t));
        let rows_left = left;
        match sel {
            None => {
                let rect = ERect::from_min_size(pos2(rows_left, detail_top), vec2(bar_w, 60.0));
                let hint = "Tap above or below the bar to add a stop. Tap a stop to edit it, drag it to move it.";
                let galley = ui.painter().layout(hint.to_string(), FontId::new(13.0, FontFamily::Proportional), col(DARK.text_secondary), rect.width());
                ui.painter().galley(rect.min, galley, col(DARK.text_secondary));
            }
            Some((track, i)) => {
                let field = |ui: &mut Ui, row: ERect, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str| -> bool {
                    ui.painter().text(
                        pos2(row.left(), row.center().y),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::new(14.0, FontFamily::Proportional),
                        col(DARK.text_secondary),
                    );
                    let rect = ERect::from_center_size(pos2(row.right() - 48.0, row.center().y), vec2(96.0, 40.0));
                    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                    let s = child.style_mut();
                    s.visuals.override_text_color = Some(col(DARK.value));
                    s.visuals.widgets.inactive.bg_fill = col(DARK.field);
                    s.visuals.widgets.inactive.weak_bg_fill = col(DARK.field);
                    s.spacing.interact_size = vec2(96.0, 40.0);
                    let r = child.add_sized(rect.size(), egui::DragValue::new(value).speed(0.5).range(range).fixed_decimals(0).suffix(suffix));
                    r.widget_info(|| WidgetInfo::labeled(WidgetType::DragValue, true, format!("Stop {}", label.to_lowercase())));
                    r.changed()
                };
                let row1 = ERect::from_min_size(pos2(rows_left, detail_top), vec2(bar_w, 44.0));
                let mut loc = g.position(track, i).unwrap_or(0.0) * 100.0;
                if field(ui, row1, "Location", &mut loc, 0.0..=100.0, "%") {
                    let now = g.move_to(track, i, loc / 100.0);
                    st.selected = Some((track, now));
                    changed = true;
                }
                let row2 = ERect::from_min_size(pos2(rows_left, detail_top + 48.0), vec2(bar_w, 44.0));
                match track {
                    Track::Opacity => {
                        let mut a = f64::from(g.opacities.get(i).map_or(1.0, |s| s.1)) * 100.0;
                        if field(ui, row2, "Opacity", &mut a, 0.0..=100.0, "%") {
                            g.set_opacity(i, (a / 100.0) as f32);
                            changed = true;
                        }
                    }
                    Track::Color => {
                        ui.painter().text(
                            pos2(row2.left(), row2.center().y),
                            Align2::LEFT_CENTER,
                            "Colour",
                            FontId::new(14.0, FontFamily::Proportional),
                            col(DARK.text_secondary),
                        );
                        let cur = g.colors.get(i).map_or([1.0; 4], |s| s.1);
                        let c = rgb32(cur);
                        let mut rgb = [c.r(), c.g(), c.b()];
                        let rect = ERect::from_center_size(pos2(row2.right() - 36.0, row2.center().y), vec2(72.0, 36.0));
                        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                        child.spacing_mut().interact_size = vec2(72.0, 36.0);
                        let r = child.color_edit_button_srgb(&mut rgb);
                        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Stop colour"));
                        if r.changed() {
                            g.set_rgb(i, [f32::from(rgb[0]) / 255.0, f32::from(rgb[1]) / 255.0, f32::from(rgb[2]) / 255.0]);
                            changed = true;
                        }
                    }
                }
                let del = ERect::from_min_size(pos2(rows_left, detail_top + 100.0), vec2(bar_w, 44.0));
                let can = g.len(track) > 1;
                let r = ui.interact(del, Id::new("gradient-delete"), if can { Sense::click() } else { Sense::hover() });
                r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, can, "Delete stop"));
                ui.painter().rect_filled(del, tokens::RADIUS_MD, if r.is_pointer_button_down_on() { col(DARK.row_selected) } else { col(DARK.header) });
                ui.painter().text(
                    del.center(),
                    Align2::CENTER_CENTER,
                    "Delete stop",
                    FontId::new(14.0, medium()),
                    if can { col(DARK.danger) } else { col(DARK.text_disabled) },
                );
                if r.clicked() && can && g.remove(track, i) {
                    st.selected = None;
                    changed = true;
                }
            }
        }

        if changed {
            let r = self.editor.set_gradient(st.layer, &st.path, &g);
            self.report(r);
        }
        if closed { Next::Close } else { Next::Stay }
    }
}
