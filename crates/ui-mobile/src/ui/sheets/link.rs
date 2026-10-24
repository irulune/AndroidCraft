//! Linking: choosing the layer a layer-reference property points at, and the pick-whip that ties a
//! property to another layer's property with an expression (`prop.pickWhip`). Both are plain
//! lists, since a phone has no cursor to drag a whip with: first the layer, then the property.

use super::*;

/// What a link row shows: the layer picked, or "None".
pub(in crate::ui) fn layer_label(comp: &CompView, id: Option<u64>) -> String {
    id.and_then(|id| comp.layers.iter().find(|l| l.id == id)).map_or_else(|| "None".to_string(), |l| format!("{}. {}", l.index, l.name))
}

/// Every property of a layer that can be the far end of a link, as `(menu text, id)`.
pub(in crate::ui) fn linkable(tree: &PropNode) -> Vec<(String, u64)> {
    fn walk(n: &PropNode, trail: &str, out: &mut Vec<(String, u64)>) {
        let here = if n.path.is_empty() || n.name.is_empty() {
            trail.to_string()
        } else if trail.is_empty() {
            n.name.clone()
        } else {
            format!("{trail} \u{25b8} {}", n.name)
        };
        if !n.is_group() {
            if matches!(n.kind.as_str(), "scalar" | "vec2" | "vec3" | "color" | "bool" | "enum") && n.uid != 0 {
                out.push((here, n.uid));
            }
            return;
        }
        for c in &n.children {
            walk(c, &here, out);
        }
    }
    let mut out = Vec::new();
    // Transform first, as in the property list: it is what most links are made to.
    let mut groups: Vec<&PropNode> = tree.children.iter().collect();
    groups.sort_by_key(|g| g.path != "transform");
    for c in groups {
        walk(c, "", &mut out);
    }
    out
}

impl MobileApp {
    /// Picks the layer a layer-reference property points at.
    pub(super) fn sheet_layer_link(&mut self, ui: &mut Ui, full: ERect, phone: bool, layer: u64, path: &str, name: &str, comp: Option<&CompView>) -> Next {
        let Some(c) = comp else { return Next::Close };
        let current = self.editor.layer_tree(layer, 8).ok().and_then(|t| t.find(path).and_then(|n| n.value.as_u64()));
        let mut items = vec![Item::new("None").selected(current.is_none())];
        items.extend(c.layers.iter().map(|l| Item::new(format!("{}  {}", l.index, l.name)).swatch(col(label_color(&l.label))).selected(current == Some(l.id))));
        match self.list_sheet(ui, full, phone, name, &items, None) {
            Pick::Close => Next::Close,
            Pick::Nothing => Next::Stay,
            Pick::Row(i) => {
                let target = if i == 0 { None } else { c.layers.get(i - 1).map(|l| l.id) };
                let r = self.editor.set_layer_link(layer, path, target);
                self.report(r);
                Next::Close
            }
        }
    }

    /// The pick-whip as two lists: the target layer, then its property.
    pub(super) fn sheet_link(&mut self, ui: &mut Ui, full: ERect, phone: bool, link: &LinkTo, comp: Option<&CompView>, ctx: &egui::Context) -> Next {
        let Some(c) = comp else { return Next::Close };
        match link.target {
            None => {
                let items: Vec<Item> = c.layers.iter().map(|l| Item::new(format!("{}  {}", l.index, l.name)).swatch(col(label_color(&l.label)))).collect();
                match self.list_sheet(ui, full, phone, &format!("Link {} to\u{2026}", link.name), &items, Some("Pick the layer, then the property to follow")) {
                    Pick::Close => Next::Close,
                    Pick::Nothing => Next::Stay,
                    Pick::Row(i) => match c.layers.get(i) {
                        Some(l) => Next::Open(Sheet::Link(LinkTo { target: Some(l.id), ..link.clone() })),
                        None => Next::Close,
                    },
                }
            }
            Some(target) => {
                let props = self.editor.layer_tree(target, 8).map(|t| linkable(&t)).unwrap_or_default();
                let layer_name = c.layers.iter().find(|l| l.id == target).map_or_else(String::new, |l| l.name.clone());
                let items: Vec<Item> = props.iter().map(|(label, _)| Item::new(label.clone())).collect();
                match self.list_sheet(ui, full, phone, &layer_name, &items, Some("The property follows this one with an expression")) {
                    Pick::Close => Next::Close,
                    Pick::Nothing => Next::Stay,
                    Pick::Row(i) => {
                        if let Some((_, uid)) = props.get(i) {
                            let r = self.editor.pick_whip(link.layer, &link.path, target, *uid);
                            if self.report(r).is_some() {
                                self.notify(ctx, format!("{} linked", link.name));
                            }
                        }
                        Next::Close
                    }
                }
            }
        }
    }
}
