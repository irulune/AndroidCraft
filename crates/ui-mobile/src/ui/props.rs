//! The properties panel: the selected layer's whole property tree (every group and every property
//! type) and its effects, with keyframe stopwatches and touch-sized value editors.

use super::sheets::{InputKind, Sheet};
use super::*;
use crate::ops::ParamInfo;

const PROP_H: f32 = 48.0;
const GROUP_H: f32 = 40.0;

/// How a scalar is edited: drag speed, allowed range, digits and unit.
struct ScalarStyle {
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &'static str,
}

fn scalar_style(path: &str, meta: Option<&ParamInfo>, value: f64) -> ScalarStyle {
    let last = path.rsplit('/').next().unwrap_or(path);
    if path.starts_with("transform/") {
        return match last {
            "opacity" => ScalarStyle { speed: 0.5, range: 0.0..=100.0, decimals: 1, suffix: "%" },
            "scale" => ScalarStyle { speed: 0.5, range: -10000.0..=10000.0, decimals: 1, suffix: "%" },
            "rotation" | "rotationX" | "rotationY" | "rotationZ" | "orientation" => {
                ScalarStyle { speed: 0.5, range: f64::MIN..=f64::MAX, decimals: 1, suffix: "\u{00b0}" }
            }
            _ => ScalarStyle { speed: 1.0, range: f64::MIN..=f64::MAX, decimals: 1, suffix: "" },
        };
    }
    if let Some(m) = meta {
        let lo = m.min.unwrap_or(f64::MIN);
        let hi = m.max.unwrap_or(f64::MAX);
        let span = match (m.slider_min, m.slider_max) {
            (Some(a), Some(b)) if b > a => b - a,
            _ => 100.0,
        };
        return ScalarStyle { speed: (span / 300.0).max(0.01), range: lo..=hi, decimals: m.decimals.min(4), suffix: "" };
    }
    ScalarStyle { speed: (value.abs() * 0.01).max(0.05), range: f64::MIN..=f64::MAX, decimals: 2, suffix: "" }
}

fn color32(c: &[f64]) -> Color32 {
    let b = |i: usize| (c.get(i).copied().unwrap_or(1.0).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(b(0), b(1), b(2), b(3))
}

fn switch(ui: &mut Ui, rect: ERect, id: Id, on: bool, label: &str) -> Response {
    let r = ui.interact(rect, id, Sense::click());
    r.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, on, label));
    let track = ERect::from_center_size(rect.center(), vec2(40.0, 24.0));
    ui.painter().rect_filled(track, 12.0, if on { col(DARK.accent) } else { col(DARK.field) });
    ui.painter().rect_stroke(track, 12.0, Stroke::new(1.0, col(DARK.border)), StrokeKind::Inside);
    let knob_x = if on { track.right() - 12.0 } else { track.left() + 12.0 };
    ui.painter().circle_filled(pos2(knob_x, track.center().y), 9.0, if on { col(DARK.on_accent) } else { col(DARK.icon) });
    r
}

/// A flat, touch-sized button that shows a value and opens an editor.
fn value_button(ui: &mut Ui, rect: ERect, id: Id, text: &str, label: &str) -> Response {
    let r = ui.interact(rect, id, Sense::click());
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    let fill = if r.is_pointer_button_down_on() { col(DARK.row_selected) } else { col(DARK.field) };
    ui.painter().rect_filled(rect, tokens::RADIUS_SM, fill);
    let clip = ui.painter().with_clip_rect(rect.shrink2(vec2(8.0, 0.0)));
    clip.text(pos2(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::new(13.0, FontFamily::Proportional), col(DARK.value));
    paint_icon(ui.painter(), Icon::ChevronDown, pos2(rect.right() - 12.0, rect.center().y), 14.0, col(DARK.text_secondary));
    r
}

fn pill(ui: &mut Ui, id: Id, rect: ERect, label: &str, value: &str) -> Response {
    let r = ui.interact(rect, id, Sense::click());
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("{label}: {value}")));
    let fill = if r.is_pointer_button_down_on() { col(DARK.row_selected) } else { col(DARK.surface_raised) };
    ui.painter().rect_filled(rect, 8.0, fill);
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.top() + 12.0),
        Align2::LEFT_CENTER,
        label,
        FontId::new(10.5, FontFamily::Proportional),
        col(DARK.text_secondary),
    );
    let clip = ui.painter().with_clip_rect(rect.shrink2(vec2(8.0, 0.0)));
    clip.text(pos2(rect.left() + 10.0, rect.bottom() - 13.0), Align2::LEFT_CENTER, value, FontId::new(13.0, medium()), col(DARK.text_primary));
    paint_icon(ui.painter(), Icon::ChevronDown, pos2(rect.right() - 12.0, rect.center().y), 14.0, col(DARK.text_secondary));
    r
}

impl MobileApp {
    /// The properties / effects panel for the selected layer.
    pub(super) fn properties(&mut self, ui: &mut Ui, rect: ERect, comp: &CompView, sheet: bool) {
        let radius = if sheet { CornerRadius { nw: tokens::RADIUS_LG as u8, ne: tokens::RADIUS_LG as u8, sw: 0, se: 0 } } else { CornerRadius::ZERO };
        ui.painter().rect_filled(rect, radius, col(DARK.surface));
        ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, col(DARK.border)), StrokeKind::Inside);
        let head = ERect::from_min_size(rect.min, vec2(rect.width(), TAB_H + if sheet { 4.0 } else { 0.0 }));
        let active = if self.panel_tab == PanelTab::Properties { 0 } else { 1 };
        if let Some(i) = self.tab_strip(ui, head, &["Properties", "Effects"], active) {
            self.panel_tab = if i == 0 { PanelTab::Properties } else { PanelTab::Effects };
        }
        if sheet {
            let b = ERect::from_center_size(pos2(rect.right() - 24.0, head.center().y), vec2(44.0, 44.0));
            if icon_button(ui, Id::new("close-props"), b, Icon::Close, col(DARK.icon), "Close properties", true).clicked() {
                self.layout.inspector_open = false;
            }
        }
        let body = ERect::from_min_max(pos2(rect.left(), head.bottom()), rect.max);
        let Some(layer) = self.selected(comp).cloned() else {
            ui.painter().text(
                body.center(),
                Align2::CENTER_CENTER,
                "Select a layer to edit it",
                FontId::new(13.0, FontFamily::Proportional),
                col(DARK.text_secondary),
            );
            return;
        };
        let Some(tree) = self.tree_of(layer.id, comp.time) else {
            ui.painter().text(
                body.center(),
                Align2::CENTER_CENTER,
                "This layer has no properties",
                FontId::new(13.0, FontFamily::Proportional),
                col(DARK.text_secondary),
            );
            return;
        };
        let tab = self.panel_tab;
        self.scoped(ui, body, |s, ui| {
            egui::ScrollArea::vertical().id_salt(("props", tab == PanelTab::Effects)).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                match tab {
                    PanelTab::Properties => s.properties_tab(ui, &layer, &tree),
                    PanelTab::Effects => s.effects_tab(ui, &layer, &tree),
                }
                ui.add_space(32.0);
            });
        });
    }

    fn properties_tab(&mut self, ui: &mut Ui, layer: &LayerRow, tree: &PropNode) {
        // Layer card: name, kind, blend mode, parent, and the text for text layers.
        let w = ui.available_width();
        let (card, _) = ui.allocate_exact_size(vec2(w, 94.0), Sense::hover());
        ui.painter().text(
            pos2(card.left() + tokens::SPACE_3, card.top() + 18.0),
            Align2::LEFT_CENTER,
            &layer.name,
            FontId::new(15.0, semibold()),
            col(DARK.text_primary),
        );
        let kind = ui.painter().layout_no_wrap(layer.kind.clone(), FontId::new(12.0, FontFamily::Proportional), col(DARK.text_secondary));
        let name_w = ui.painter().layout_no_wrap(layer.name.clone(), FontId::new(15.0, semibold()), col(DARK.text_primary)).size().x;
        ui.painter().galley(pos2(card.left() + tokens::SPACE_3 + name_w + 8.0, card.top() + 18.0 - kind.size().y / 2.0), kind, col(DARK.text_secondary));
        let half = (w - 3.0 * tokens::SPACE_3) / 2.0;
        let y = card.top() + 36.0;
        let blend = ERect::from_min_size(pos2(card.left() + tokens::SPACE_3, y), vec2(half, 46.0));
        let parent_name = layer
            .parent
            .and_then(|p| self.editor.comp().ok().flatten().and_then(|c| c.layers.iter().find(|l| l.id == p).map(|l| l.name.clone())))
            .unwrap_or_else(|| "None".into());
        let parent = ERect::from_min_size(pos2(blend.right() + tokens::SPACE_3, y), vec2(half, 46.0));
        if pill(ui, Id::new("blend-pill"), blend, "Blend mode", &layer.blend).clicked() {
            self.sheet = Some(Sheet::Blend);
        }
        if pill(ui, Id::new("parent-pill"), parent, "Parent", &parent_name).clicked() {
            self.sheet = Some(Sheet::Parent);
        }
        if let Some(t) = tree.find("text/sourceText") {
            let current = t.value.as_str().unwrap_or_default().to_string();
            let (row, _) = ui.allocate_exact_size(vec2(w, PROP_H), Sense::hover());
            ui.painter().text(
                pos2(row.left() + tokens::SPACE_3, row.center().y),
                Align2::LEFT_CENTER,
                "Text",
                FontId::new(13.0, FontFamily::Proportional),
                col(DARK.text_secondary),
            );
            let field = ERect::from_min_max(pos2(row.left() + 76.0, row.top() + 6.0), pos2(row.right() - tokens::SPACE_3, row.bottom() - 6.0));
            let shown = current.lines().next().unwrap_or_default().to_string();
            if value_button(ui, field, Id::new("text-edit"), &shown, "Edit text").clicked() {
                self.sheet = Some(Sheet::Input { kind: InputKind::Text(layer.id), value: current, title: "Edit text".into(), multiline: true });
            }
        }
        // Transform first (the most used group), then the rest in the engine's order.
        let mut groups: Vec<&PropNode> = tree.children.iter().filter(|g| g.path != "effects").collect();
        groups.sort_by_key(|g| g.path != "transform");
        for g in groups {
            if g.children.is_empty() {
                continue;
            }
            self.node_rows(ui, layer.id, g, 0, None);
        }
    }

    fn effects_tab(&mut self, ui: &mut Ui, layer: &LayerRow, tree: &PropNode) {
        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, 60.0), Sense::hover());
        let btn = ERect::from_min_max(pos2(row.left() + tokens::SPACE_3, row.top() + 8.0), pos2(row.right() - tokens::SPACE_3, row.bottom() - 8.0));
        let r = ui.interact(btn, Id::new("add-effect"), Sense::click());
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Add effect"));
        ui.painter().rect_filled(btn, tokens::RADIUS_MD, col(if r.is_pointer_button_down_on() { DARK.focus_ring } else { DARK.accent }));
        paint_icon(ui.painter(), Icon::Plus, pos2(btn.center().x - 52.0, btn.center().y), 18.0, col(DARK.on_accent));
        ui.painter().text(pos2(btn.center().x - 36.0, btn.center().y), Align2::LEFT_CENTER, "Add effect", FontId::new(14.0, semibold()), col(DARK.on_accent));
        if r.clicked() {
            self.sheet = Some(Sheet::Effects { query: String::new() });
        }
        let Some(fx) = tree.find("effects") else { return };
        if fx.children.is_empty() {
            let (r, _) = ui.allocate_exact_size(vec2(w, 90.0), Sense::hover());
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                "No effects on this layer yet",
                FontId::new(13.0, FontFamily::Proportional),
                col(DARK.text_secondary),
            );
            return;
        }
        let count = fx.children.len();
        let catalog = self.catalog();
        for (i, e) in fx.children.iter().enumerate() {
            let info = e.effect.as_ref().and_then(|id| catalog.iter().find(|c| &c.id == id));
            let open = self.expanded.contains(&e.path);
            let (head, _) = ui.allocate_exact_size(vec2(w, PROP_H), Sense::hover());
            ui.painter().rect_filled(head, 0.0, col(DARK.surface_raised));
            ui.painter().line_segment([head.left_top(), head.right_top()], Stroke::new(1.0, col(DARK.border)));
            let sw = ERect::from_min_size(head.min, vec2(52.0, PROP_H));
            if switch(ui, sw, Id::new(("fx-on", &e.path)), e.enabled, &format!("Enable {}", e.name)).clicked() {
                let r = self.editor.toggle_effect(layer.id, &e.path, !e.enabled);
                self.report(r);
            }
            let b = 40.0;
            let del = ERect::from_min_size(pos2(head.right() - b - 4.0, head.top()), vec2(b, PROP_H));
            let down = del.translate(vec2(-b, 0.0));
            let up = down.translate(vec2(-b, 0.0));
            let label = ERect::from_min_max(pos2(sw.right(), head.top()), pos2(up.left(), head.bottom()));
            let lr = ui.interact(label, Id::new(("fx-name", &e.path)), Sense::click());
            lr.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Effect {}", e.name)));
            let color = if e.enabled { col(DARK.text_primary) } else { col(DARK.text_disabled) };
            ui.painter().with_clip_rect(label).text(pos2(label.left(), head.center().y), Align2::LEFT_CENTER, &e.name, FontId::new(14.0, medium()), color);
            if lr.clicked() {
                if open {
                    self.expanded.remove(&e.path);
                } else {
                    self.expanded.insert(e.path.clone());
                }
            }
            if icon_button(ui, Id::new(("fx-up", &e.path)), up, Icon::ChevronUp, col(DARK.icon), &format!("Move {} up", e.name), i > 0).clicked() {
                let r = self.editor.reorder_effect(layer.id, &e.path, i.saturating_sub(1));
                self.report(r);
            }
            if icon_button(ui, Id::new(("fx-down", &e.path)), down, Icon::ChevronDown, col(DARK.icon), &format!("Move {} down", e.name), i + 1 < count)
                .clicked()
            {
                let r = self.editor.reorder_effect(layer.id, &e.path, i + 1);
                self.report(r);
            }
            if icon_button(ui, Id::new(("fx-del", &e.path)), del, Icon::Trash, col(DARK.danger), &format!("Remove {}", e.name), true).clicked() {
                let r = self.editor.remove_effect(layer.id, &e.path);
                if self.report(r).is_some() {
                    self.expanded.remove(&e.path);
                }
            }
            if open {
                for c in &e.children {
                    self.node_rows(ui, layer.id, c, 1, info);
                }
            }
        }
    }

    /// A group header (tap to expand) with its rows, or a single property row.
    fn node_rows(&mut self, ui: &mut Ui, layer: u64, node: &PropNode, depth: usize, effect: Option<&EffectInfo>) {
        if !node.is_group() {
            let meta = effect.and_then(|e| e.params.iter().find(|p| node.path.rsplit('/').next() == Some(p.id.as_str())));
            self.prop_row(ui, layer, node, depth, meta);
            return;
        }
        if node.children.is_empty() {
            return;
        }
        let open = self.expanded.contains(&node.path);
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), GROUP_H), Sense::hover());
        let r = ui.interact(row, Id::new(("group", &node.path)), Sense::click());
        r.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, open, node.name.clone()));
        let fill = if r.is_pointer_button_down_on() { col(DARK.row_selected) } else { col(DARK.header) };
        ui.painter().rect_filled(row, 0.0, fill);
        ui.painter().line_segment([row.left_bottom(), row.right_bottom()], Stroke::new(1.0, col(DARK.border)));
        let x = row.left() + tokens::SPACE_3 + depth as f32 * 14.0;
        paint_icon(ui.painter(), if open { Icon::ChevronDown } else { Icon::ChevronRight }, pos2(x + 6.0, row.center().y), 16.0, col(DARK.icon));
        ui.painter().text(pos2(x + 22.0, row.center().y), Align2::LEFT_CENTER, &node.name, FontId::new(13.5, medium()), col(DARK.text_primary));
        if r.clicked() {
            if open {
                self.expanded.remove(&node.path);
            } else {
                self.expanded.insert(node.path.clone());
            }
        }
        if open {
            for c in &node.children {
                self.node_rows(ui, layer, c, depth + 1, effect);
            }
        }
    }

    /// One property: stopwatch, name, and the editor for its type.
    fn prop_row(&mut self, ui: &mut Ui, layer: u64, node: &PropNode, depth: usize, meta: Option<&ParamInfo>) {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), PROP_H), Sense::hover());
        let animatable = matches!(node.kind.as_str(), "scalar" | "vec2" | "vec3" | "color");
        let x0 = row.left() + 4.0 + depth as f32 * 14.0;
        let mut name_left = x0 + 8.0;
        if animatable {
            let stop = ERect::from_min_size(pos2(x0, row.top()), vec2(40.0, PROP_H));
            let tint = if node.animated() { col(DARK.value) } else { col(DARK.icon) };
            if icon_button(ui, Id::new(("key", layer, &node.path)), stop, Icon::Stopwatch, tint, &format!("Keyframe {}", node.name), true).clicked() {
                let r = self.editor.toggle_key(layer, &node.path);
                self.report(r);
            }
            name_left = stop.right() + 2.0;
        }
        let nums = node.numbers();
        let avail = row.right() - tokens::SPACE_3 - name_left;
        let field_w = 66.0;
        // Width the editor wants, so the name gets the rest.
        let want = match node.kind.as_str() {
            "scalar" => 96.0,
            "vec2" => 2.0 * (field_w + 4.0),
            "vec3" => {
                if node.path.ends_with("position") || node.path.ends_with("anchor") || node.path.ends_with("scale") {
                    2.0 * (field_w + 4.0)
                } else {
                    3.0 * 60.0
                }
            }
            "color" => 64.0,
            "bool" => 52.0,
            "enum" if meta.is_some_and(|m| !m.options.is_empty()) => (avail * 0.55).clamp(110.0, 190.0),
            "enum" => 80.0,
            "text" | "string" => (avail * 0.55).clamp(110.0, 200.0),
            _ => 70.0,
        };
        let ctl_left = (row.right() - tokens::SPACE_3 - want).max(name_left + 70.0);
        let name_rect = ERect::from_min_max(pos2(name_left, row.top()), pos2(ctl_left - 6.0, row.bottom()));
        let nr = ui.interact(name_rect, Id::new(("pname", layer, &node.path)), Sense::click());
        nr.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("{} options", node.name)));
        let label = if node.expression { format!("{} =", node.name) } else { node.name.clone() };
        let color = if node.expression { col(DARK.warning) } else { col(DARK.text_secondary) };
        ui.painter().with_clip_rect(name_rect).text(
            pos2(name_rect.left(), row.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::new(13.0, FontFamily::Proportional),
            color,
        );
        if nr.secondary_clicked() || nr.long_touched() || nr.clicked() {
            self.sheet = Some(Sheet::Prop { layer, path: node.path.clone(), name: node.name.clone(), animated: node.animated() });
        }

        let area = ERect::from_min_max(pos2(ctl_left, row.top()), pos2(row.right() - tokens::SPACE_3, row.bottom()));
        match node.kind.as_str() {
            "scalar" | "vec2" | "vec3" => {
                let count = match node.kind.as_str() {
                    "scalar" => 1,
                    "vec2" => 2,
                    _ if node.path.ends_with("position") || node.path.ends_with("anchor") || node.path.ends_with("scale") => 2,
                    _ => 3,
                }
                .min(nums.len());
                self.number_fields(ui, layer, node, &nums, count, area, meta);
            }
            "color" => {
                let mut c = color32(&nums);
                let rect = ERect::from_center_size(pos2(area.right() - 30.0, row.center().y), vec2(56.0, 32.0));
                let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                child.spacing_mut().interact_size = vec2(56.0, 32.0);
                let r = child.color_edit_button_srgba(&mut c);
                r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("{} colour", node.name)));
                if r.changed() {
                    let a = c.to_array();
                    let rgba = [f64::from(a[0]) / 255.0, f64::from(a[1]) / 255.0, f64::from(a[2]) / 255.0, f64::from(a[3]) / 255.0];
                    let res = self.editor.set_color(layer, &node.path, rgba);
                    self.report(res);
                }
            }
            "bool" => {
                let on = node.value.as_bool().unwrap_or(false);
                let rect = ERect::from_center_size(pos2(area.right() - 26.0, row.center().y), vec2(52.0, PROP_H));
                if switch(ui, rect, Id::new(("bool", layer, &node.path)), on, &node.name).clicked() {
                    let r = self.editor.set_bool(layer, &node.path, !on);
                    self.report(r);
                }
            }
            "enum" => {
                let cur = nums.first().copied().unwrap_or(0.0).round() as i64;
                match meta.filter(|m| !m.options.is_empty()) {
                    Some(m) => {
                        let text = m.options.iter().find(|(v, _)| *v == cur).map_or_else(|| format!("Option {cur}"), |(_, l)| l.clone());
                        let rect = ERect::from_min_max(pos2(area.left(), row.top() + 7.0), pos2(area.right(), row.bottom() - 7.0));
                        if value_button(ui, rect, Id::new(("enum", layer, &node.path)), &text, &node.name).clicked() {
                            self.sheet =
                                Some(Sheet::Choice { layer, path: node.path.clone(), title: node.name.clone(), options: m.options.clone(), current: cur });
                        }
                    }
                    None => {
                        let mut v = cur as f64;
                        let rect = ERect::from_center_size(pos2(area.right() - field_w / 2.0, row.center().y), vec2(field_w, 36.0));
                        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                        style_field(&mut child, field_w);
                        let r = child.add_sized(rect.size(), egui::DragValue::new(&mut v).speed(0.05).range(0.0..=999.0).fixed_decimals(0));
                        r.widget_info(|| WidgetInfo::labeled(WidgetType::DragValue, true, node.name.clone()));
                        if r.changed() {
                            let res = self.editor.set_index(layer, &node.path, v.round() as i64);
                            self.report(res);
                        }
                    }
                }
            }
            "text" | "string" => {
                let current = node.value.as_str().unwrap_or_default().to_string();
                let rect = ERect::from_min_max(pos2(area.left(), row.top() + 7.0), pos2(area.right(), row.bottom() - 7.0));
                let shown = current.lines().next().unwrap_or_default().to_string();
                if value_button(ui, rect, Id::new(("str", layer, &node.path)), &shown, &node.name).clicked() {
                    let kind = if node.path == "text/sourceText" { InputKind::Text(layer) } else { InputKind::Str(layer, node.path.clone()) };
                    self.sheet = Some(Sheet::Input { kind, value: current, title: node.name.clone(), multiline: node.kind == "text" });
                }
            }
            other => {
                let what = match other {
                    "path" => "Shape path",
                    "gradient" => "Gradient",
                    "layer" => "Layer link",
                    _ => "Not editable here",
                };
                ui.painter().text(
                    pos2(area.right(), row.center().y),
                    Align2::RIGHT_CENTER,
                    what,
                    FontId::new(12.0, FontFamily::Proportional),
                    col(DARK.text_disabled),
                );
            }
        }
        ui.painter()
            .line_segment([pos2(row.left() + 12.0, row.bottom()), pos2(row.right(), row.bottom())], Stroke::new(1.0, col(DARK.border).gamma_multiply(0.5)));
    }

    #[allow(clippy::too_many_arguments)]
    fn number_fields(&mut self, ui: &mut Ui, layer: u64, node: &PropNode, nums: &[f64], count: usize, area: ERect, meta: Option<&ParamInfo>) {
        let style = scalar_style(&node.path, meta, nums.first().copied().unwrap_or(0.0));
        let mut values = nums.to_vec();
        let mut changed = false;
        let gap = 4.0;
        let fw = ((area.width() - gap * (count.saturating_sub(1)) as f32) / count.max(1) as f32).clamp(48.0, 96.0);
        let mut x = area.right() - count as f32 * fw - (count.saturating_sub(1)) as f32 * gap;
        for k in 0..count {
            let rect = ERect::from_min_size(pos2(x, area.center().y - 18.0), vec2(fw, 36.0));
            x += fw + gap;
            let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
            style_field(&mut child, fw);
            let Some(v) = values.get_mut(k) else { continue };
            let r = child.add_sized(
                rect.size(),
                egui::DragValue::new(v).speed(style.speed).range(style.range.clone()).fixed_decimals(style.decimals).suffix(style.suffix),
            );
            r.widget_info(|| WidgetInfo::labeled(WidgetType::DragValue, true, if count == 1 { node.name.clone() } else { format!("{} {}", node.name, k + 1) }));
            if r.drag_started() {
                self.editor.begin_gesture();
            }
            changed |= r.changed();
            if changed {
                // Apply inside the drag so the viewer follows the finger.
                let res =
                    if values.len() == 1 { self.editor.set_scalar(layer, &node.path, values[0]) } else { self.editor.set_vec(layer, &node.path, &values) };
                self.report(res);
                changed = false;
            }
            if r.drag_stopped() {
                self.editor.end_gesture();
            }
        }
    }
}

fn style_field(child: &mut Ui, w: f32) {
    let st = child.style_mut();
    st.visuals.override_text_color = Some(col(DARK.value));
    st.visuals.widgets.inactive.bg_fill = col(DARK.field);
    st.visuals.widgets.inactive.weak_bg_fill = col(DARK.field);
    st.spacing.interact_size = vec2(w, 36.0);
}
