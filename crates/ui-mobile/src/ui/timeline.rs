//! The timeline: action bar, ruler with zoom and pan, layer rows with draggable bars, and the
//! selected layer's animated properties with draggable keyframes.

use super::sheets::Sheet;
use super::*;

const ACTION_H: f32 = 44.0;
const KEY_ROW_H: f32 = 34.0;
const OVERVIEW_H: f32 = 12.0;

/// The visible time range. `span <= 0` means "fit the whole composition".
#[derive(Clone, Copy, Default, Debug)]
pub(super) struct View {
    pub start: f64,
    pub span: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum DragKind {
    Move,
    TrimIn,
    TrimOut,
    Key,
}

/// A drag in progress. The engine snaps every change to whole frames, so the finger's motion is
/// accumulated here and only whole-frame steps are sent.
#[derive(Clone, Debug)]
pub(super) struct Drag {
    layer: u64,
    kind: DragKind,
    acc: f64,
    applied: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Act {
    Add,
    Duplicate,
    Split,
    Delete,
    TrimIn,
    TrimOut,
    KeyPrev,
    KeyNext,
    More,
    Track,
    ZoomIn,
    ZoomOut,
    Fit,
    EaseKey,
    LinearKey,
    HoldKey,
    CurveKey,
    DeleteKey,
    DeselectKey,
}

struct Chip {
    icon: Icon,
    label: &'static str,
    enabled: bool,
    act: Act,
    accent: bool,
}

/// One animated property of the selected layer, for the keyframe sub-rows.
struct AnimRow {
    path: String,
    name: String,
    keys: Vec<f64>,
}

fn collect_animated(node: &PropNode, trail: &str, out: &mut Vec<AnimRow>) {
    // Name a property after its group, except Transform (whose properties speak for themselves)
    // and the layer root.
    let here = if node.is_group() && !node.path.is_empty() && node.path != "transform" { node.name.clone() } else { trail.to_string() };
    if !node.is_group() && !node.keys.is_empty() {
        let name = if here.is_empty() { node.name.clone() } else { format!("{here} \u{00b7} {}", node.name) };
        out.push(AnimRow { path: node.path.clone(), name, keys: node.keys.clone() });
    }
    for c in &node.children {
        collect_animated(c, &here, out);
    }
}

fn chip(ui: &mut Ui, id: Id, c: &Chip) -> Response {
    let galley = ui.painter().layout_no_wrap(c.label.to_string(), FontId::new(12.5, medium()), col(DARK.text_primary));
    let w = 12.0 + 18.0 + 6.0 + galley.size().x + 12.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
    let sense = if c.enabled { Sense::click() } else { Sense::hover() };
    let r = ui.interact(rect, id, sense);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, c.enabled, c.label));
    let fill = if c.accent {
        col(DARK.accent)
    } else if c.enabled && r.is_pointer_button_down_on() {
        col(DARK.row_selected)
    } else {
        col(DARK.surface_raised)
    };
    ui.painter().rect_filled(rect, 18.0, fill);
    let tint = if !c.enabled {
        col(DARK.text_disabled)
    } else if c.accent {
        col(DARK.on_accent)
    } else {
        col(DARK.icon)
    };
    paint_icon(ui.painter(), c.icon, pos2(rect.left() + 12.0 + 9.0, rect.center().y), 18.0, tint);
    let text_color = if !c.enabled {
        col(DARK.text_disabled)
    } else if c.accent {
        col(DARK.on_accent)
    } else {
        col(DARK.text_primary)
    };
    ui.painter().text(pos2(rect.left() + 12.0 + 18.0 + 6.0, rect.center().y), Align2::LEFT_CENTER, c.label, FontId::new(12.5, medium()), text_color);
    r
}

impl MobileApp {
    /// The visible range clamped to the composition.
    fn view_range(&self, dur: f64, fps: f64) -> (f64, f64) {
        let min_span = (4.0 / fps.max(1.0)).max(0.1).min(dur);
        let span = if self.view.span <= 0.0 { dur } else { self.view.span.clamp(min_span, dur) };
        let start = self.view.start.clamp(0.0, (dur - span).max(0.0));
        (start, span)
    }

    fn set_view(&mut self, start: f64, span: f64, dur: f64, fps: f64) {
        let min_span = (4.0 / fps.max(1.0)).max(0.1).min(dur);
        let span = span.clamp(min_span, dur);
        self.view = if span >= dur - 1e-9 { View::default() } else { View { start: start.clamp(0.0, dur - span), span } };
    }

    fn timeline_actions(&self, comp: &CompView) -> Vec<Chip> {
        let sel = self.selected(comp).is_some();
        let mut v = Vec::new();
        if self.sel_key.is_some() {
            v.push(Chip { icon: Icon::Diamond, label: "Easy Ease", enabled: true, act: Act::EaseKey, accent: true });
            v.push(Chip { icon: Icon::Diamond, label: "Linear", enabled: true, act: Act::LinearKey, accent: false });
            v.push(Chip { icon: Icon::Diamond, label: "Hold", enabled: true, act: Act::HoldKey, accent: false });
            v.push(Chip { icon: Icon::Sliders, label: "Curve", enabled: true, act: Act::CurveKey, accent: false });
            v.push(Chip { icon: Icon::Trash, label: "Delete key", enabled: true, act: Act::DeleteKey, accent: false });
            v.push(Chip { icon: Icon::Close, label: "Done", enabled: true, act: Act::DeselectKey, accent: false });
        }
        v.push(Chip { icon: Icon::Plus, label: "Add", enabled: true, act: Act::Add, accent: self.sel_key.is_none() });
        v.push(Chip { icon: Icon::Track, label: "Track", enabled: sel, act: Act::Track, accent: self.tracking().is_some() });
        v.push(Chip { icon: Icon::Copy, label: "Duplicate", enabled: sel, act: Act::Duplicate, accent: false });
        v.push(Chip { icon: Icon::Scissors, label: "Split", enabled: sel, act: Act::Split, accent: false });
        v.push(Chip { icon: Icon::Trash, label: "Delete", enabled: sel, act: Act::Delete, accent: false });
        v.push(Chip { icon: Icon::TrimIn, label: "Trim in", enabled: sel, act: Act::TrimIn, accent: false });
        v.push(Chip { icon: Icon::TrimOut, label: "Trim out", enabled: sel, act: Act::TrimOut, accent: false });
        v.push(Chip { icon: Icon::KeyPrev, label: "Prev key", enabled: sel, act: Act::KeyPrev, accent: false });
        v.push(Chip { icon: Icon::KeyNext, label: "Next key", enabled: sel, act: Act::KeyNext, accent: false });
        v.push(Chip { icon: Icon::More, label: "More", enabled: sel, act: Act::More, accent: false });
        v.push(Chip { icon: Icon::ZoomOut, label: "Zoom out", enabled: true, act: Act::ZoomOut, accent: false });
        v.push(Chip { icon: Icon::ZoomIn, label: "Zoom in", enabled: true, act: Act::ZoomIn, accent: false });
        v.push(Chip { icon: Icon::Fit, label: "Fit", enabled: true, act: Act::Fit, accent: false });
        v
    }

    fn do_action(&mut self, ctx: &egui::Context, act: Act, comp: &CompView) {
        let sel = self.selected(comp).map(|l| l.id);
        let (dur, fps) = (comp.duration.max(1e-6), comp.frame_rate);
        let (v0, span) = self.view_range(dur, fps);
        match act {
            Act::Add => self.sheet = Some(Sheet::Add),
            Act::More => self.sheet = Some(Sheet::Layer),
            Act::Track => self.sheet = Some(Sheet::Track),
            Act::Duplicate => {
                let r = self.editor.duplicate_selected();
                if self.report(r).is_some() {
                    self.notify(ctx, "Layer duplicated");
                }
            }
            Act::Split => {
                let r = self.editor.split_selected();
                if self.report(r).is_some() {
                    self.notify(ctx, "Split at the playhead");
                }
            }
            Act::Delete => {
                let r = self.editor.delete_selected();
                if self.report(r).is_some() {
                    self.sel_key = None;
                    self.notify(ctx, "Layer deleted");
                }
            }
            Act::TrimIn => {
                if let Some(id) = sel {
                    let r = self.editor.trim_in_to_time(id);
                    self.report(r);
                }
            }
            Act::TrimOut => {
                if let Some(id) = sel {
                    let r = self.editor.trim_out_to_time(id);
                    self.report(r);
                }
            }
            Act::KeyPrev | Act::KeyNext => {
                if let Some(id) = sel {
                    self.playing = false;
                    let r = self.editor.go_to_layer_key(id, act == Act::KeyNext);
                    self.report(r);
                }
            }
            Act::ZoomIn | Act::ZoomOut => {
                let f = if act == Act::ZoomIn { 0.5 } else { 2.0 };
                let centre = comp.time.clamp(v0, v0 + span);
                let frac = (centre - v0) / span;
                let new_span = (span * f).clamp(0.0, dur);
                self.set_view(centre - frac * new_span, new_span, dur, fps);
            }
            Act::Fit => self.view = View::default(),
            Act::EaseKey => {
                let r = self.editor.easy_ease_keys();
                self.report(r);
            }
            Act::LinearKey => {
                let r = self.editor.key_interpolation("linear");
                self.report(r);
            }
            Act::HoldKey => {
                let r = self.editor.key_interpolation("hold");
                self.report(r);
            }
            Act::CurveKey => self.open_ease_editor(),
            Act::DeleteKey => {
                let r = self.editor.delete_keys();
                if self.report(r).is_some() {
                    self.sel_key = None;
                }
            }
            Act::DeselectKey => {
                self.sel_key = None;
                let r = self.editor.deselect_keys();
                self.report(r);
            }
        }
    }

    /// Sends the whole frames the finger has travelled since the last step.
    fn drag_step(&mut self, dx_secs: f64, fps: f64) {
        let Some(d) = self.drag.as_mut() else { return };
        d.acc += dx_secs;
        let fd = 1.0 / fps.max(1.0);
        let target = (d.acc / fd).round() * fd;
        let step = target - d.applied;
        if step.abs() < fd * 0.5 {
            return;
        }
        d.applied = target;
        let (layer, kind) = (d.layer, d.kind.clone());
        let r = match kind {
            DragKind::Move => self.editor.shift_layer(layer, step),
            DragKind::TrimIn => self.editor.trim_in_by(layer, step),
            DragKind::TrimOut => self.editor.trim_out_by(layer, step),
            DragKind::Key => self.editor.move_keys(step),
        };
        if self.report(r).is_some()
            && kind == DragKind::Key
            && let Some(k) = self.sel_key.as_mut()
        {
            k.time += step;
        }
    }

    fn drag_begin(&mut self, layer: u64, kind: DragKind) {
        if self.editor.select_layer(layer).is_err() {
            return;
        }
        self.editor.begin_gesture();
        self.drag = Some(Drag { layer, kind, acc: 0.0, applied: 0.0 });
    }

    fn drag_end(&mut self) {
        self.editor.end_gesture();
        self.drag = None;
    }

    /// The timeline: layer rows on the left (eye, label colour, name), a ruler and label-coloured
    /// layer bars on the right, keyframes for the selected layer, a playhead across both.
    pub(super) fn timeline_panel(&mut self, ui: &mut Ui, rect: ERect, comp: &CompView, wide: bool) {
        let ctx = ui.ctx().clone();
        let n = comp.layers.len();
        self.strip(ui, ERect::from_min_size(rect.min, vec2(rect.width(), TAB_H)), &comp.name, &format!("{n} layer{}", if n == 1 { "" } else { "s" }));

        // Action bar: horizontally scrolling chips.
        let bar = ERect::from_min_size(pos2(rect.left(), rect.top() + TAB_H), vec2(rect.width(), ACTION_H));
        ui.painter().rect_filled(bar, 0.0, col(DARK.surface));
        ui.painter().line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(1.0, col(DARK.border)));
        let chips = self.timeline_actions(comp);
        let mut fired = None;
        self.scoped(ui, bar, |_, ui| {
            egui::ScrollArea::horizontal()
                .id_salt("timeline-actions")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                        ui.add_space(tokens::SPACE_3);
                        for (i, c) in chips.iter().enumerate() {
                            if chip(ui, Id::new(("chip", i, c.label)), c).clicked() {
                                fired = Some(c.act);
                            }
                        }
                        ui.add_space(tokens::SPACE_3);
                    });
                });
        });
        if let Some(a) = fired {
            self.do_action(&ctx, a, comp);
        }

        let body = ERect::from_min_max(pos2(rect.left(), bar.bottom()), rect.max);
        ui.painter().rect_filled(body, 0.0, col(DARK.timeline_bg));
        let name_w = if wide { 232.0 } else { 148.0 };
        let bars_l = body.left() + name_w;
        let bars_r = body.right() - tokens::SPACE_3;
        let bw = (bars_r - bars_l).max(1.0);
        let (dur, fps) = (comp.duration.max(1e-6), comp.frame_rate);

        // Pinch to zoom and two-finger / wheel pan, anywhere over the timeline.
        if ui.rect_contains_pointer(body) || ctx.input(|i| i.multi_touch().is_some()) {
            let (zoom, pan_x, touch_pan) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta().x, i.multi_touch().map_or(0.0, |m| m.translation_delta.x)));
            let (v0, span) = self.view_range(dur, fps);
            if (zoom - 1.0).abs() > 1e-3 {
                let anchor_frac = ctx.input(|i| i.pointer.hover_pos()).map_or(0.5, |p| f64::from(((p.x - bars_l) / bw).clamp(0.0, 1.0)));
                let focus = v0 + anchor_frac * span;
                let new_span = span / f64::from(zoom);
                self.set_view(focus - anchor_frac * new_span, new_span, dur, fps);
            }
            let pan = f64::from(pan_x + touch_pan);
            if pan.abs() > 0.0 {
                let (v0, span) = self.view_range(dur, fps);
                self.set_view(v0 - pan / f64::from(bw) * span, span, dur, fps);
            }
        }
        // Keep the playhead in view while playing.
        if self.playing {
            let (v0, span) = self.view_range(dur, fps);
            if comp.time < v0 || comp.time > v0 + span {
                self.set_view(comp.time - span * 0.1, span, dur, fps);
            }
        }
        let (v0, span) = self.view_range(dur, fps);
        let x_of = move |t: f64| bars_l + ((t - v0) / span) as f32 * bw;

        // Ruler (pinned above the scrolling rows); dragging it scrubs time.
        let ruler = ERect::from_min_max(pos2(bars_l, body.top()), pos2(body.right(), body.top() + RULER_H));
        ui.painter().rect_filled(ERect::from_min_max(body.min, pos2(body.right(), ruler.bottom())), 0.0, col(DARK.surface));
        const STEPS: [f64; 13] = [0.04, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0];
        let step = STEPS.into_iter().find(|s| (s / span) as f32 * bw >= 56.0).unwrap_or(300.0);
        {
            let clip = ui.painter().with_clip_rect(ERect::from_min_max(pos2(bars_l - 1.0, body.top()), pos2(body.right(), ruler.bottom())));
            let mut t = (v0 / step).floor() * step;
            while t <= v0 + span + step {
                let x = x_of(t);
                clip.line_segment([pos2(x, ruler.bottom() - 9.0), pos2(x, ruler.bottom())], Stroke::new(1.0, col(DARK.ruler_tick)));
                let whole = t.max(0.0);
                let label = if step < 1.0 {
                    format!("{}:{:04.1}", (whole as u64) / 60, whole % 60.0)
                } else {
                    format!("{}:{:02}", (whole as u64) / 60, (whole as u64) % 60)
                };
                clip.text(pos2(x + 3.0, ruler.top() + 9.0), Align2::LEFT_CENTER, label, FontId::new(10.0, FontFamily::Proportional), col(DARK.text_secondary));
                t += step;
            }
        }
        ui.painter().line_segment([pos2(body.left(), ruler.bottom()), pos2(body.right(), ruler.bottom())], Stroke::new(1.0, col(DARK.border)));
        let scrub =
            ui.interact(ERect::from_min_max(pos2(bars_l - 12.0, body.top()), pos2(body.right(), ruler.bottom())), Id::new("scrub"), Sense::click_and_drag());
        scrub.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, "Time"));
        if let Some(p) = scrub.interact_pointer_pos().filter(|_| scrub.dragged() || scrub.clicked() || scrub.is_pointer_button_down_on()) {
            let frac = f64::from(((p.x - bars_l) / bw).clamp(0.0, 1.0));
            self.playing = false;
            let r = self.editor.set_time(v0 + frac * span);
            self.report(r);
        }

        // Overview strip (only when zoomed in): drag to pan.
        let mut rows_top = ruler.bottom() + 1.0;
        if span < dur - 1e-9 {
            let strip = ERect::from_min_max(pos2(bars_l, rows_top), pos2(bars_r, rows_top + OVERVIEW_H));
            ui.painter().rect_filled(strip.shrink2(vec2(0.0, 4.0)), 2.0, col(DARK.field));
            let thumb = ERect::from_min_max(
                pos2(strip.left() + (v0 / dur) as f32 * bw, strip.top() + 2.0),
                pos2(strip.left() + ((v0 + span) / dur) as f32 * bw, strip.bottom() - 2.0),
            );
            ui.painter().rect_filled(thumb, 3.0, col(DARK.accent).gamma_multiply(0.8));
            let r = ui.interact(strip.expand2(vec2(0.0, 10.0)), Id::new("overview"), Sense::drag());
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, "Visible time range"));
            if r.dragged() {
                let d = f64::from(r.drag_delta().x / bw) * dur;
                self.set_view(v0 + d, span, dur, fps);
            }
            rows_top += OVERVIEW_H;
        }

        // Rows.
        let rows = ERect::from_min_max(pos2(body.left(), rows_top), body.max);
        let mut picked = None;
        let mut toggled = None;
        let mut long_pressed = None;
        let secs_per_px = span / f64::from(bw);
        self.scoped(ui, rows, |s, ui| {
            egui::ScrollArea::vertical().id_salt("timeline-rows").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for (i, l) in comp.layers.iter().enumerate() {
                    let (row, resp) = ui.allocate_exact_size(vec2(rows.width(), ROW_H), Sense::click());
                    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Layer {}, {}", l.index, l.name)));
                    row_paint(ui.painter(), row, l, i, name_w, bars_l, bars_r, &x_of, wide);
                    if resp.secondary_clicked() || resp.long_touched() {
                        long_pressed = Some(l.id);
                    } else if resp.clicked() {
                        picked = Some(l.id);
                    }
                    let eye = ERect::from_min_size(row.min, vec2(36.0, ROW_H));
                    let er = ui.interact(eye, Id::new(("eye", l.id)), Sense::click());
                    er.widget_info(|| WidgetInfo::labeled(WidgetType::Checkbox, true, format!("Show {}", l.name)));
                    paint_icon(ui.painter(), if l.visible { Icon::Eye } else { Icon::EyeOff }, eye.center(), tokens::ICON_SM, col(DARK.icon));
                    if er.clicked() {
                        toggled = Some((l.id, !l.visible));
                    }

                    // Keyframes of every property, as small diamonds on the bar.
                    let tree = s.keys_of(l.id);
                    if let Some(t) = &tree {
                        let clip = ui.painter().with_clip_rect(ERect::from_min_max(pos2(bars_l, row.top()), pos2(bars_r + 6.0, row.bottom())));
                        for k in t.all_keys() {
                            let x = x_of(l.start + k);
                            if x >= bars_l - 4.0 && x <= bars_r + 4.0 {
                                let c = pos2(x, row.center().y);
                                clip.add(Shape::convex_polygon(
                                    vec![pos2(c.x, c.y - 4.5), pos2(c.x + 4.5, c.y), pos2(c.x, c.y + 4.5), pos2(c.x - 4.5, c.y)],
                                    Color32::from_white_alpha(235),
                                    Stroke::new(1.0, Color32::from_black_alpha(120)),
                                ));
                            }
                        }
                    }

                    // Bar: drag to move, drag either end to trim.
                    let (ux0, ux1) = (x_of(l.in_point), x_of(l.out_point));
                    let vis = ERect::from_min_max(pos2(ux0.max(bars_l), row.top() + 6.0), pos2(ux1.min(bars_r).max(ux0.max(bars_l) + 2.0), row.bottom() - 6.0));
                    if ux1 > bars_l && ux0 < bars_r && !l.locked {
                        let hw = if vis.width() > 64.0 { 20.0 } else { 0.0 };
                        let body_r = ERect::from_min_max(
                            pos2(vis.left() + if ux0 >= bars_l { hw } else { 0.0 }, vis.top()),
                            pos2(vis.right() - if ux1 <= bars_r { hw } else { 0.0 }, vis.bottom()),
                        );
                        let br = ui.interact(body_r, Id::new(("bar", l.id)), Sense::click_and_drag());
                        br.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, format!("Move {}", l.name)));
                        if br.clicked() {
                            picked = Some(l.id);
                        }
                        if br.drag_started() {
                            s.drag_begin(l.id, DragKind::Move);
                        }
                        if br.dragged() {
                            s.drag_step(f64::from(br.drag_delta().x) * secs_per_px, fps);
                        }
                        if br.drag_stopped() {
                            s.drag_end();
                        }
                        if hw > 0.0 {
                            for (is_in, rect) in [
                                (true, ERect::from_min_size(vis.min, vec2(hw, vis.height()))),
                                (false, ERect::from_min_max(pos2(vis.right() - hw, vis.top()), vis.max)),
                            ] {
                                if (is_in && ux0 < bars_l) || (!is_in && ux1 > bars_r) {
                                    continue;
                                }
                                let hr = ui.interact(rect, Id::new(("trim", l.id, is_in)), Sense::drag());
                                hr.widget_info(|| {
                                    WidgetInfo::labeled(WidgetType::Other, true, format!("Trim {} {}", if is_in { "in" } else { "out" }, l.name))
                                });
                                if hr.drag_started() {
                                    s.drag_begin(l.id, if is_in { DragKind::TrimIn } else { DragKind::TrimOut });
                                }
                                if hr.dragged() {
                                    s.drag_step(f64::from(hr.drag_delta().x) * secs_per_px, fps);
                                }
                                if hr.drag_stopped() {
                                    s.drag_end();
                                }
                                // Grip lines show where the handle is.
                                let gx = if is_in { rect.left() + 5.0 } else { rect.right() - 5.0 };
                                ui.painter().line_segment(
                                    [pos2(gx, rect.center().y - 6.0), pos2(gx, rect.center().y + 6.0)],
                                    Stroke::new(2.0, Color32::from_black_alpha(140)),
                                );
                            }
                        }
                    }

                    // The selected layer's animated properties, with draggable keyframes.
                    if l.selected
                        && let Some(t) = &tree
                    {
                        let mut animated = Vec::new();
                        collect_animated(t, "", &mut animated);
                        for a in animated {
                            s.key_row(ui, l, &a, rows.width(), name_w, bars_l, bars_r, &x_of, secs_per_px, fps);
                        }
                    }
                }
                ui.add_space(24.0);
            });
        });
        if let Some((id, on)) = toggled {
            let r = self.editor.set_switch(id, "video", on);
            self.report(r);
        }
        if let Some(id) = long_pressed {
            if self.editor.select_layer(id).is_ok() {
                self.sheet = Some(Sheet::Layer);
            }
        } else if let Some(id) = picked {
            self.sel_key = None;
            let r = self.editor.select_layer(id);
            self.report(r);
        }

        // Playhead: a line over the rows and a handle in the ruler.
        if comp.time >= v0 - 1e-9 && comp.time <= v0 + span + 1e-9 {
            let px = x_of(comp.time);
            let clip = ui.painter().with_clip_rect(ERect::from_min_max(pos2(bars_l - 8.0, body.top()), body.max));
            clip.line_segment([pos2(px, ruler.top() + 12.0), pos2(px, body.bottom())], Stroke::new(tokens::PLAYHEAD_WIDTH, col(DARK.playhead)));
            clip.add(Shape::convex_polygon(
                vec![
                    pos2(px - 6.0, ruler.top() + 2.0),
                    pos2(px + 6.0, ruler.top() + 2.0),
                    pos2(px + 6.0, ruler.top() + 12.0),
                    pos2(px, ruler.top() + 19.0),
                    pos2(px - 6.0, ruler.top() + 12.0),
                ],
                col(DARK.playhead),
                Stroke::NONE,
            ));
        }
        ui.painter().line_segment([pos2(bars_l, body.top()), pos2(bars_l, body.bottom())], Stroke::new(1.0, col(DARK.border)));
    }

    /// One animated property of the selected layer: its name and a draggable diamond per keyframe.
    #[allow(clippy::too_many_arguments)]
    fn key_row(
        &mut self,
        ui: &mut Ui,
        l: &LayerRow,
        a: &AnimRow,
        width: f32,
        name_w: f32,
        bars_l: f32,
        bars_r: f32,
        x_of: &dyn Fn(f64) -> f32,
        secs_per_px: f64,
        fps: f64,
    ) {
        let (row, _) = ui.allocate_exact_size(vec2(width, KEY_ROW_H), Sense::hover());
        ui.painter().rect_filled(row, 0.0, col(DARK.surface_raised).gamma_multiply(0.6));
        ui.painter().rect_filled(ERect::from_min_size(row.min, vec2(3.0, row.height())), 0.0, col(label_color(&l.label)));
        let name = ui.painter().layout_no_wrap(a.name.clone(), FontId::new(12.0, FontFamily::Proportional), col(DARK.text_secondary));
        ui.painter().with_clip_rect(ERect::from_min_max(pos2(row.left() + 30.0, row.top()), pos2(row.left() + name_w - 6.0, row.bottom()))).galley(
            pos2(row.left() + 30.0, row.center().y - name.size().y / 2.0),
            name,
            col(DARK.text_secondary),
        );
        // Line between the first and last key.
        if let (Some(f), Some(e)) = (a.keys.first(), a.keys.last()) {
            let (x0, x1) = (x_of(l.start + f).max(bars_l), x_of(l.start + e).min(bars_r));
            if x1 > x0 {
                ui.painter()
                    .with_clip_rect(ERect::from_min_max(pos2(bars_l, row.top()), pos2(bars_r + 6.0, row.bottom())))
                    .line_segment([pos2(x0, row.center().y), pos2(x1, row.center().y)], Stroke::new(1.0, col(DARK.keyframe).gamma_multiply(0.4)));
            }
        }
        for (i, k) in a.keys.iter().enumerate() {
            let x = x_of(l.start + k);
            if x < bars_l - 6.0 || x > bars_r + 6.0 {
                continue;
            }
            let c = pos2(x, row.center().y);
            let selected = self.sel_key.as_ref().is_some_and(|s| s.layer == l.id && s.path == a.path && (s.time - k).abs() < 1e-4);
            let hit = ERect::from_center_size(c, vec2(32.0, KEY_ROW_H));
            let r = ui.interact(hit, Id::new(("key", l.id, a.path.clone(), i)), Sense::click_and_drag());
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Keyframe {} at {:.2} s", a.name, k)));
            let (rad, fill) = if selected { (7.5, col(DARK.keyframe_selected)) } else { (6.0, col(DARK.keyframe)) };
            ui.painter().with_clip_rect(ERect::from_min_max(pos2(bars_l - 8.0, row.top()), pos2(bars_r + 8.0, row.bottom()))).add(Shape::convex_polygon(
                vec![pos2(c.x, c.y - rad), pos2(c.x + rad, c.y), pos2(c.x, c.y + rad), pos2(c.x - rad, c.y)],
                fill,
                Stroke::new(1.0, Color32::from_black_alpha(140)),
            ));
            if r.clicked() || r.drag_started() {
                let sel = self.editor.select_key(l.id, &a.path, *k);
                if self.report(sel).is_some() {
                    self.sel_key = Some(SelKey { layer: l.id, path: a.path.clone(), time: *k });
                }
            }
            if r.drag_started() && self.sel_key.is_some() {
                self.drag_begin(l.id, DragKind::Key);
            }
            if r.dragged() {
                self.drag_step(f64::from(r.drag_delta().x) * secs_per_px, fps);
            }
            if r.drag_stopped() {
                self.drag_end();
            }
        }
    }
}

/// One timeline row: striped background, eye column space, label square, name, layer bar.
#[allow(clippy::too_many_arguments)]
fn row_paint(p: &Painter, row: ERect, l: &LayerRow, i: usize, name_w: f32, bars_l: f32, bars_r: f32, x_of: &dyn Fn(f64) -> f32, wide: bool) {
    let bg = if l.selected {
        DARK.row_selected
    } else if i.is_multiple_of(2) {
        DARK.surface
    } else {
        DARK.row_alt
    };
    p.rect_filled(row, 0.0, col(bg));
    let label = label_color(&l.label);
    let mut x = row.left() + 36.0;
    p.rect_filled(ERect::from_center_size(pos2(x + 5.0, row.center().y), vec2(10.0, 14.0)), 2.0, col(label));
    x += 18.0;
    if wide {
        p.text(pos2(x, row.center().y), Align2::LEFT_CENTER, l.index.to_string(), FontId::new(12.0, FontFamily::Monospace), col(DARK.text_secondary));
        x += 22.0;
    }
    let mut right = row.left() + name_w - 4.0;
    if l.locked {
        paint_icon(p, Icon::Lock, pos2(right - 8.0, row.center().y), 14.0, col(DARK.text_secondary));
        right -= 18.0;
    }
    let name = p.layout_no_wrap(l.name.clone(), FontId::new(13.5, if l.selected { medium() } else { FontFamily::Proportional }), col(DARK.text_primary));
    let ny = row.center().y - name.size().y.min(ROW_H) / 2.0;
    p.with_clip_rect(ERect::from_min_max(pos2(x, row.top()), pos2(right, row.bottom()))).galley(pos2(x, ny), name, col(DARK.text_primary));
    // Layer bar, in the label colour; dimmed when the layer is hidden.
    let (x0, x1) = (x_of(l.in_point), x_of(l.out_point));
    if x1 <= bars_l || x0 >= bars_r {
        return;
    }
    let bar = ERect::from_min_max(pos2(x0.max(bars_l), row.top() + 6.0), pos2(x1.min(bars_r).max(x0.max(bars_l) + 2.0), row.bottom() - 6.0));
    let alpha = if l.visible { 0.92 } else { 0.35 };
    p.rect_filled(bar, tokens::RADIUS_SM, col(label).gamma_multiply(alpha));
    if l.selected {
        p.rect_stroke(bar, tokens::RADIUS_SM, Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
    }
}
