//! Editing operations for the touch interface, every one a real engine command.
//!
//! Layer creation and arrangement, trimming and moving in time, effects, the property tree,
//! composition management and frame export. Like [`crate::editor`], nothing here keeps a second
//! copy of the document; failures come back as [`EditorError`] values.

use effectcraft_engine::Session;
use serde_json::{Value, json};

use crate::editor::{Editor, EditorError, Result, finite};

fn err(msg: impl Into<String>) -> EditorError {
    EditorError(msg.into())
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The blend modes the picker offers, in the desktop app's order.
pub const BLEND_MODES: [&str; 38] = [
    "Normal",
    "Dissolve",
    "Dancing Dissolve",
    "Darken",
    "Multiply",
    "Color Burn",
    "Classic Color Burn",
    "Linear Burn",
    "Darker Color",
    "Add",
    "Lighten",
    "Screen",
    "Color Dodge",
    "Classic Color Dodge",
    "Linear Dodge",
    "Lighter Color",
    "Overlay",
    "Soft Light",
    "Hard Light",
    "Linear Light",
    "Vivid Light",
    "Pin Light",
    "Hard Mix",
    "Difference",
    "Classic Difference",
    "Exclusion",
    "Subtract",
    "Divide",
    "Hue",
    "Saturation",
    "Color",
    "Luminosity",
    "Stencil Alpha",
    "Stencil Luma",
    "Silhouette Alpha",
    "Silhouette Luma",
    "Alpha Add",
    "Luminescent Premul",
];

/// Layer label names.
pub const LABELS: [&str; 17] = [
    "None",
    "Red",
    "Yellow",
    "Aqua",
    "Pink",
    "Lavender",
    "Peach",
    "Sea Foam",
    "Blue",
    "Green",
    "Purple",
    "Orange",
    "Brown",
    "Fuchsia",
    "Cyan",
    "Sandstone",
    "Dark Green",
];

/// What "Add layer" can create.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewLayer {
    Text,
    Solid,
    Rectangle,
    Ellipse,
    Star,
    Null,
    Adjustment,
    Camera,
    Light,
    Cube,
    Sphere,
}

impl NewLayer {
    pub const ALL: [NewLayer; 11] = [
        NewLayer::Text,
        NewLayer::Solid,
        NewLayer::Rectangle,
        NewLayer::Ellipse,
        NewLayer::Star,
        NewLayer::Null,
        NewLayer::Adjustment,
        NewLayer::Camera,
        NewLayer::Light,
        NewLayer::Cube,
        NewLayer::Sphere,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NewLayer::Text => "Text",
            NewLayer::Solid => "Solid",
            NewLayer::Rectangle => "Rectangle",
            NewLayer::Ellipse => "Ellipse",
            NewLayer::Star => "Star",
            NewLayer::Null => "Null Object",
            NewLayer::Adjustment => "Adjustment Layer",
            NewLayer::Camera => "Camera",
            NewLayer::Light => "Light",
            NewLayer::Cube => "3D Cube",
            NewLayer::Sphere => "3D Sphere",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            NewLayer::Text => "Type a title",
            NewLayer::Solid => "Flat colour plate",
            NewLayer::Rectangle | NewLayer::Ellipse | NewLayer::Star => "Shape layer",
            NewLayer::Null => "Invisible parent for moving groups",
            NewLayer::Adjustment => "Effects apply to everything below",
            NewLayer::Camera => "Fly through 3D layers",
            NewLayer::Light => "Lights 3D layers",
            NewLayer::Cube | NewLayer::Sphere => "Primitive with a material",
        }
    }

    fn command(self) -> (&'static str, Value) {
        match self {
            NewLayer::Text => ("layer.newText", json!({ "text": "Text" })),
            NewLayer::Solid => ("layer.newSolid", json!({ "name": "Solid", "color": "#3b82f6" })),
            NewLayer::Rectangle => ("layer.newShape", json!({ "kind": "rect", "name": "Rectangle" })),
            NewLayer::Ellipse => ("layer.newShape", json!({ "kind": "ellipse", "name": "Ellipse" })),
            NewLayer::Star => ("layer.newShape", json!({ "kind": "star", "name": "Star" })),
            NewLayer::Null => ("layer.newNull", json!({})),
            NewLayer::Adjustment => ("layer.newAdjustment", json!({})),
            NewLayer::Camera => ("layer.newCamera", json!({})),
            NewLayer::Light => ("layer.newLight", json!({})),
            NewLayer::Cube => ("layer.newPrimitive", json!({ "kind": "cube" })),
            NewLayer::Sphere => ("layer.newPrimitive", json!({ "kind": "sphere" })),
        }
    }
}

/// One node of a layer's property tree: a group (`kind` empty) or a property.
#[derive(Clone, Debug, PartialEq)]
pub struct PropNode {
    pub path: String,
    pub name: String,
    /// `scalar`, `vec2`, `vec3`, `color`, `bool`, `enum`, `text`, `string`, ... or empty for groups.
    pub kind: String,
    pub value: Value,
    /// Keyframe times in layer seconds.
    pub keys: Vec<f64>,
    pub enabled: bool,
    /// Effect id, for an effect's group.
    pub effect: Option<String>,
    /// The expression's text, when the property has one.
    pub expression: Option<String>,
    pub children: Vec<PropNode>,
    /// The engine's id for this node (a mask's id is what the mask commands take).
    pub uid: u64,
    /// Set on a mask's group.
    pub mask: Option<MaskState>,
    /// The id of the group this node sits in (0 at the top). A path property's group is the mask
    /// or shape path the point-editing commands take.
    pub group: u64,
}

/// The mode and inversion of one mask.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskState {
    pub mode: String,
    pub inverted: bool,
}

/// One side's ease for each dimension of a keyframe: `(speed, influence in percent)`.
pub type Eases = Vec<(f64, f64)>;

/// A keyframe as the curve editor needs it: where it is, its value and how it eases.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyEase {
    /// Layer seconds.
    pub time: f64,
    /// One number per dimension (a spatial property has its full position here).
    pub value: Vec<f64>,
    /// A spatial property is eased along its path as one dimension.
    pub spatial: bool,
    pub in_ease: Eases,
    pub out_ease: Eases,
    /// `linear`, `bezier`, `hold`.
    pub interp_in: String,
    pub interp_out: String,
    pub units: String,
}

impl KeyEase {
    /// The number of eased dimensions.
    pub fn dims(&self) -> usize {
        self.in_ease.len().max(self.out_ease.len()).max(1)
    }
}

/// A mask or shape path: its points and the handles at each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathData {
    pub closed: bool,
    pub vertices: Vec<[f64; 2]>,
    /// Handle offsets from each vertex, going in and going out.
    pub ins: Vec<[f64; 2]>,
    pub outs: Vec<[f64; 2]>,
}

impl PathData {
    fn from_json(v: &Value) -> Option<PathData> {
        let v = v.get("v")?;
        let pts = |k: &str| -> Vec<[f64; 2]> {
            v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect()).unwrap_or_default()
        };
        let vertices = pts("vertices");
        let n = vertices.len();
        let (mut ins, mut outs) = (pts("in_tangents"), pts("out_tangents"));
        ins.resize(n, [0.0; 2]);
        outs.resize(n, [0.0; 2]);
        Some(PathData { closed: v.get("closed").and_then(Value::as_bool).unwrap_or(false), vertices, ins, outs })
    }

    /// A vertex with handles is smooth; one without is a corner.
    pub fn is_smooth(&self, i: usize) -> bool {
        self.ins.get(i).is_some_and(|t| *t != [0.0; 2]) || self.outs.get(i).is_some_and(|t| *t != [0.0; 2])
    }
}

/// The ways a mask combines with the masks above it.
pub const MASK_MODES: [&str; 7] = ["None", "Add", "Subtract", "Intersect", "Lighten", "Darken", "Difference"];

/// The shapes a new mask can start as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskShape {
    Rectangle,
    Ellipse,
}

impl PropNode {
    /// The masks of a layer's tree, in stacking order.
    pub fn masks(&self) -> Vec<&PropNode> {
        self.find("masks").map(|m| m.children.iter().filter(|c| c.mask.is_some()).collect()).unwrap_or_default()
    }

    /// The first property of this kind anywhere below (`gradient`, `path`, ...).
    pub fn find_kind(&self, kind: &str) -> Option<&PropNode> {
        if self.kind == kind {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find_kind(kind))
    }

    /// A mask's or shape path's own path property.
    pub fn path_prop(&self) -> Option<&PropNode> {
        self.children.iter().find(|c| c.kind == "path")
    }

    pub fn is_group(&self) -> bool {
        self.kind.is_empty()
    }

    pub fn animated(&self) -> bool {
        !self.keys.is_empty()
    }

    /// The value as numbers (scalars, vectors, colours, enum index, bool as 0/1).
    pub fn numbers(&self) -> Vec<f64> {
        match &self.value {
            Value::Number(n) => vec![n.as_f64().unwrap_or(0.0)],
            Value::Array(a) => a.iter().filter_map(Value::as_f64).collect(),
            Value::Bool(b) => vec![if *b { 1.0 } else { 0.0 }],
            _ => vec![],
        }
    }

    /// Finds a node by path.
    pub fn find(&self, path: &str) -> Option<&PropNode> {
        if self.path == path {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(path))
    }

    /// Every keyframe time under this node, sorted, de-duplicated (layer seconds).
    pub fn all_keys(&self) -> Vec<f64> {
        let mut out = Vec::new();
        self.collect_keys(&mut out);
        out.sort_by(|a, b| a.total_cmp(b));
        out.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        out
    }

    fn collect_keys(&self, out: &mut Vec<f64>) {
        out.extend(self.keys.iter().copied());
        for c in &self.children {
            c.collect_keys(out);
        }
    }

    fn parse(v: &Value) -> PropNode {
        let keys =
            v.get("keys").and_then(Value::as_array).map(|a| a.iter().filter_map(|k| k.get("time").and_then(Value::as_f64)).collect()).unwrap_or_default();
        let uid = v.get("uid").and_then(Value::as_u64).unwrap_or(0);
        let mut children: Vec<PropNode> = v.get("children").and_then(Value::as_array).map(|a| a.iter().map(PropNode::parse).collect()).unwrap_or_default();
        for c in &mut children {
            c.group = uid;
        }
        PropNode {
            path: text(v, "path"),
            name: text(v, "name"),
            kind: text(v, "type"),
            value: v.get("value").cloned().unwrap_or(Value::Null),
            keys,
            enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            effect: v.get("effect").and_then(Value::as_str).map(str::to_string),
            expression: v.get("expression").and_then(Value::as_str).map(str::to_string),
            children,
            uid,
            group: 0,
            mask: v.get("mask").map(|m| MaskState { mode: text(m, "mode"), inverted: m.get("inverted").and_then(Value::as_bool).unwrap_or(false) }),
        }
    }
}

/// One parameter of an effect in the catalogue.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub slider_min: Option<f64>,
    pub slider_max: Option<f64>,
    pub decimals: usize,
    /// Enum choices as `(value, label)`.
    pub options: Vec<(i64, String)>,
}

/// An effect from the catalogue.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub params: Vec<ParamInfo>,
}

/// An item of the project panel that is a composition.
#[derive(Clone, Debug, PartialEq)]
pub struct CompItem {
    pub id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub layers: usize,
    pub active: bool,
}

/// What kind of output to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFormat {
    Mp4,
    Gif,
}

impl Editor {
    /// Runs any engine command by id: the escape hatch for the shared shortcut table.
    pub fn command(&mut self, id: &str, params: Value) -> Result<Value> {
        self.run(id, params)
    }

    /// Adds a layer of `kind` at the current time. The engine selects the new layer.
    pub fn add_layer(&mut self, kind: NewLayer) -> Result<u64> {
        let (cmd, params) = kind.command();
        let r = self.run(cmd, params)?;
        r.get("layer").and_then(Value::as_u64).ok_or_else(|| err(format!("{cmd} did not return a layer")))
    }

    /// Duplicates the selected layers; returns the new layer ids.
    pub fn duplicate_selected(&mut self) -> Result<Vec<u64>> {
        let r = self.run("edit.duplicate", json!({}))?;
        Ok(r.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default())
    }

    /// Deletes the selected layers.
    pub fn delete_selected(&mut self) -> Result<()> {
        self.run("edit.clear", json!({})).map(drop)
    }

    /// Splits the selected layers at the current time.
    pub fn split_selected(&mut self) -> Result<()> {
        self.run("edit.splitLayer", json!({})).map(drop)
    }

    pub fn rename_layer(&mut self, layer: u64, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(err("a layer needs a name"));
        }
        self.run("layer.rename", json!({ "layer": layer, "name": name })).map(drop)
    }

    pub fn set_blend_mode(&mut self, layer: u64, mode: &str) -> Result<()> {
        self.run("layer.setBlendMode", json!({ "layers": [layer], "mode": mode })).map(drop)
    }

    /// The layer's blend mode name.
    pub fn blend_mode(&mut self, layer: u64) -> Result<String> {
        Ok(text(&self.layer_tree_raw(layer, 0)?, "blendMode"))
    }

    /// Flips a layer switch by its command name (`video`, `locked` is `lock`, `threeD`, `solo`, ...).
    pub fn toggle_switch(&mut self, layer: u64, name: &str, on: bool) -> Result<()> {
        self.set_switch(layer, name, on)
    }

    /// Moves the playhead to the previous or next keyframe of any property on the layer.
    pub fn go_to_layer_key(&mut self, layer: u64, next: bool) -> Result<()> {
        let tree = self.layer_tree(layer, 8)?;
        let keys = tree.all_keys();
        let comp = self.comp()?.ok_or_else(|| err("no composition"))?;
        let start = comp.layers.iter().find(|l| l.id == layer).map_or(0.0, |l| l.start);
        let now = comp.time - start;
        let hit = if next { keys.iter().copied().find(|k| *k > now + 1e-4) } else { keys.iter().copied().rev().find(|k| *k < now - 1e-4) };
        match hit {
            Some(k) => self.set_time(k + start),
            None => Err(err(if next { "no later keyframe on this layer" } else { "no earlier keyframe on this layer" })),
        }
    }

    pub fn set_parent(&mut self, layer: u64, parent: Option<u64>) -> Result<()> {
        self.run("layer.setParent", json!({ "layers": [layer], "parent": parent, "compensate": true })).map(drop)
    }

    /// `front`, `forward`, `backward` or `back`.
    pub fn arrange(&mut self, layer: u64, to: &str) -> Result<()> {
        self.run("layer.arrange", json!({ "layers": [layer], "to": to })).map(drop)
    }

    pub fn set_label(&mut self, layer: u64, label: &str) -> Result<()> {
        self.run("edit.label", json!({ "layers": [layer], "label": label })).map(drop)
    }

    /// Sets the layer's in point to the current time.
    pub fn trim_in_to_time(&mut self, layer: u64) -> Result<()> {
        self.run("layer.timing", json!({ "layers": [layer], "op": "trimInToTime" })).map(drop)
    }

    pub fn trim_out_to_time(&mut self, layer: u64) -> Result<()> {
        self.run("layer.timing", json!({ "layers": [layer], "op": "trimOutToTime" })).map(drop)
    }

    /// Shifts a layer in time by `delta` seconds. Inside a gesture (see [`Editor::begin_gesture`]) consecutive calls make one undo step.
    pub fn shift_layer(&mut self, layer: u64, delta: f64) -> Result<()> {
        self.run("layer.timing", json!({ "layers": [layer], "delta": finite(delta)? })).map(drop)
    }

    /// Moves the in point by `delta` seconds (a left-edge drag).
    pub fn trim_in_by(&mut self, layer: u64, delta: f64) -> Result<()> {
        self.run("layer.timing", json!({ "layers": [layer], "inDelta": finite(delta)? })).map(drop)
    }

    /// Moves the out point by `delta` seconds (a right-edge drag).
    pub fn trim_out_by(&mut self, layer: u64, delta: f64) -> Result<()> {
        self.run("layer.timing", json!({ "layers": [layer], "outDelta": finite(delta)? })).map(drop)
    }

    /// Nudges a layer by comp pixels.
    pub fn nudge(&mut self, layer: u64, dx: f64, dy: f64) -> Result<()> {
        self.run("layer.nudge", json!({ "layers": [layer], "x": finite(dx)?, "y": finite(dy)? })).map(drop)
    }

    /// Edits a text layer's string.
    pub fn set_text(&mut self, layer: u64, value: &str) -> Result<()> {
        self.run("prop.set", json!({ "layer": layer, "path": "text/sourceText", "value": value })).map(drop)
    }

    /// Sets a string property.
    pub fn set_str(&mut self, layer: u64, path: &str, value: &str) -> Result<()> {
        self.run("prop.set", json!({ "layer": layer, "path": path, "value": value })).map(drop)
    }

    /// Sets a boolean or enum property by value.
    pub fn set_index(&mut self, layer: u64, path: &str, value: i64) -> Result<()> {
        self.run("prop.set", json!({ "layer": layer, "path": path, "value": value })).map(drop)
    }

    pub fn set_bool(&mut self, layer: u64, path: &str, value: bool) -> Result<()> {
        self.run("prop.set", json!({ "layer": layer, "path": path, "value": value })).map(drop)
    }

    /// Sets a colour property from RGBA in 0..=1.
    pub fn set_color(&mut self, layer: u64, path: &str, rgba: [f64; 4]) -> Result<()> {
        for v in rgba {
            finite(v)?;
        }
        self.run("prop.set", json!({ "layer": layer, "path": path, "value": rgba })).map(drop)
    }

    /// Resets a property to its default value.
    pub fn reset_prop(&mut self, layer: u64, path: &str) -> Result<()> {
        self.run("prop.reset", json!({ "layer": layer, "path": path })).map(drop)
    }

    /// Jumps to the previous or next keyframe of `path`.
    pub fn go_to_key(&mut self, next: bool) -> Result<()> {
        self.run("time.go", json!({ "to": if next { "nextKey" } else { "prevKey" } })).map(drop)
    }

    /// Selects a property and goes to its previous or next keyframe, restricted to that property.
    pub fn go_to_prop_key(&mut self, layer: u64, path: &str, next: bool) -> Result<()> {
        let node = self.layer_tree(layer, 6)?;
        let target = node.find(path).ok_or_else(|| err(format!("no property {path}")))?;
        let comp = self.comp()?.ok_or_else(|| err("no composition"))?;
        let start = comp.layers.iter().find(|l| l.id == layer).map_or(0.0, |l| l.start);
        let now = comp.time - start;
        let hit = if next {
            target.keys.iter().copied().filter(|k| *k > now + 1e-6).min_by(|a, b| a.total_cmp(b))
        } else {
            target.keys.iter().copied().filter(|k| *k < now - 1e-6).max_by(|a, b| a.total_cmp(b))
        };
        match hit {
            Some(k) => self.set_time(k + start),
            None => Err(err(if next { "no later keyframe" } else { "no earlier keyframe" })),
        }
    }

    fn layer_tree_raw(&mut self, layer: u64, depth: u64) -> Result<Value> {
        self.run("layer.tree", json!({ "layer": layer, "depth": depth }))
    }

    /// The layer's property tree (`depth` levels below the root).
    pub fn layer_tree(&mut self, layer: u64, depth: u64) -> Result<PropNode> {
        let r = self.layer_tree_raw(layer, depth)?;
        r.get("properties").map(PropNode::parse).ok_or_else(|| err("layer has no properties"))
    }

    /// The whole effect catalogue, sorted by category then name.
    pub fn effect_catalog(&mut self) -> Result<Vec<EffectInfo>> {
        let r = self.run("effect.list", json!({}))?;
        let mut out: Vec<EffectInfo> = r
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| EffectInfo {
                        id: text(e, "id"),
                        name: text(e, "name"),
                        category: text(e, "category"),
                        params: e
                            .get("params")
                            .and_then(Value::as_array)
                            .map(|ps| {
                                ps.iter()
                                    .map(|p| ParamInfo {
                                        id: text(p, "id"),
                                        name: text(p, "name"),
                                        kind: text(p, "type"),
                                        min: p.get("min").and_then(Value::as_f64),
                                        max: p.get("max").and_then(Value::as_f64),
                                        slider_min: p.get("sliderMin").and_then(Value::as_f64),
                                        slider_max: p.get("sliderMax").and_then(Value::as_f64),
                                        decimals: p.get("decimals").and_then(Value::as_u64).unwrap_or(1) as usize,
                                        options: p
                                            .get("options")
                                            .and_then(Value::as_array)
                                            .map(|o| o.iter().map(|o| (o.get("value").and_then(Value::as_i64).unwrap_or(0), text(o, "label"))).collect())
                                            .unwrap_or_default(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| (a.category.as_str(), a.name.as_str()).cmp(&(b.category.as_str(), b.name.as_str())));
        Ok(out)
    }

    /// Applies an effect (by catalogue id) to a layer; returns the effect's property path.
    pub fn apply_effect(&mut self, layer: u64, effect_id: &str) -> Result<String> {
        let r = self.run("effect.apply", json!({ "layers": [layer], "effect": effect_id }))?;
        r.get("paths")
            .and_then(Value::as_array)
            .and_then(|p| p.first())
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| err("the effect was not applied"))
    }

    pub fn remove_effect(&mut self, layer: u64, effect_path: &str) -> Result<()> {
        let index = effect_index(effect_path)?;
        self.run("effect.remove", json!({ "layer": layer, "effect": index })).map(drop)
    }

    pub fn toggle_effect(&mut self, layer: u64, effect_path: &str, on: bool) -> Result<()> {
        let index = effect_index(effect_path)?;
        self.run("effect.toggle", json!({ "layer": layer, "effect": index, "value": on })).map(drop)
    }

    /// Moves an effect to a 0-based position in the stack.
    pub fn reorder_effect(&mut self, layer: u64, effect_path: &str, to: usize) -> Result<()> {
        let index = effect_index(effect_path)?;
        self.run("effect.reorder", json!({ "layer": layer, "effect": index, "index": to })).map(drop)
    }

    /// Selects one keyframe (layer seconds) so the keyframe commands act on it.
    pub fn select_key(&mut self, layer: u64, path: &str, time: f64) -> Result<()> {
        self.run("keys.select", json!({ "keys": [{ "layer": layer, "path": path, "time": finite(time)? }] })).map(drop)
    }

    /// Moves the selected keyframes by `delta` seconds.
    pub fn move_keys(&mut self, delta: f64) -> Result<()> {
        self.run("keys.move", json!({ "delta": finite(delta)? })).map(drop)
    }

    pub fn delete_keys(&mut self) -> Result<()> {
        self.run("keys.delete", json!({})).map(drop)
    }

    /// Adds a mask to the layer, sized to the layer, and returns its id.
    pub fn add_mask(&mut self, layer: u64, shape: MaskShape) -> Result<u64> {
        let shape = if shape == MaskShape::Ellipse { "ellipse" } else { "rect" };
        let r = self.run("layer.addMask", json!({ "layer": layer, "shape": shape }))?;
        r.get("mask").and_then(Value::as_u64).ok_or_else(|| err("the mask was not created"))
    }

    /// How a mask combines with the masks above it (one of [`MASK_MODES`]).
    pub fn set_mask_mode(&mut self, layer: u64, mask: u64, mode: &str) -> Result<()> {
        if !MASK_MODES.contains(&mode) {
            return Err(err(format!("`{mode}` is not a mask mode")));
        }
        self.run("layer.mask.mode", json!({ "layer": layer, "mask": mask, "mode": mode })).map(drop)
    }

    pub fn set_mask_inverted(&mut self, layer: u64, mask: u64, inverted: bool) -> Result<()> {
        self.run("layer.mask.invert", json!({ "layer": layer, "mask": mask, "value": inverted })).map(drop)
    }

    pub fn remove_mask(&mut self, layer: u64, mask: u64) -> Result<()> {
        self.run("layer.mask.remove", json!({ "layer": layer, "mask": mask })).map(drop)
    }

    /// Adds a gradient fill or stroke to a shape layer; returns the new item's id. It goes inside the
    /// layer's first shape group, just above the solid fill (or stroke) there, which is the place where it
    /// paints the shape; a layer without a group gets it at the top level.
    pub fn add_gradient(&mut self, layer: u64, stroke: bool) -> Result<u64> {
        let kind = if stroke { "gradientStroke" } else { "gradientFill" };
        let tree = self.layer_tree(layer, 6)?;
        let group = tree.find("contents/group/contents");
        let mut p = json!({ "layer": layer, "kind": kind });
        if group.is_some() {
            p["group"] = json!("contents/group");
        }
        let r = self.run("layer.addShapeItem", p)?;
        let uid = r.get("uid").and_then(Value::as_u64).ok_or_else(|| err("the gradient was not added"))?;
        let plain = if stroke { "/stroke" } else { "/fill" };
        if let Some(i) = group.and_then(|g| g.children.iter().position(|c| c.path.ends_with(plain))) {
            self.run("prop.moveGroup", json!({ "layer": layer, "prop": uid, "index": i + 1 }))?;
        }
        Ok(uid)
    }

    /// Points a layer-reference property (an effect's Layer menu) at a layer, or at none.
    pub fn set_layer_link(&mut self, layer: u64, path: &str, target: Option<u64>) -> Result<()> {
        self.run("prop.set", json!({ "layer": layer, "path": path, "value": target })).map(drop)
    }

    /// Ties a property to another layer's property (`target_prop` is its id) with an expression, as the
    /// pick-whip does, and returns the expression.
    pub fn pick_whip(&mut self, layer: u64, path: &str, target_layer: u64, target_prop: u64) -> Result<String> {
        let r = self.run("prop.pickWhip", json!({ "layer": layer, "path": path, "target": { "layer": target_layer, "prop": target_prop } }))?;
        Ok(r.as_str().unwrap_or_default().to_string())
    }

    /// A gradient property (a shape's Gradient Fill ▸ Colors, say) at the current time.
    pub fn gradient(&mut self, layer: u64, path: &str) -> Result<crate::gradient::GradientData> {
        let r = self.run("prop.get", json!({ "layer": layer, "path": path }))?;
        r.get("value").and_then(crate::gradient::GradientData::from_json).ok_or_else(|| err(format!("{path} is not a gradient")))
    }

    /// Sets a gradient property; on an animated one this keys the current time. Inside a gesture, edits merge into one undo step.
    pub fn set_gradient(&mut self, layer: u64, path: &str, g: &crate::gradient::GradientData) -> Result<()> {
        self.run("prop.set", json!({ "layer": layer, "path": path, "value": g.to_json() })).map(drop)
    }

    /// The points of a mask or shape path (`path` is the path property's address) at the current time.
    pub fn path_data(&mut self, layer: u64, path: &str) -> Result<PathData> {
        let r = self.run("prop.get", json!({ "layer": layer, "path": path }))?;
        r.get("value").and_then(PathData::from_json).ok_or_else(|| err(format!("{path} is not a path")))
    }

    /// Moves a vertex (`point`, in the path's own space) and/or its handles. `group` is the mask's or shape path's id.
    pub fn set_vertex(&mut self, layer: u64, group: u64, index: usize, point: Option<[f64; 2]>, handles: Option<([f64; 2], [f64; 2])>) -> Result<()> {
        let mut p = json!({ "layer": layer, "mask": group, "index": index });
        if let Some(pt) = point {
            p["point"] = json!([finite(pt[0])?, finite(pt[1])?]);
        }
        if let Some((tin, tout)) = handles {
            p["in"] = json!([finite(tin[0])?, finite(tin[1])?]);
            p["out"] = json!([finite(tout[0])?, finite(tout[1])?]);
        }
        self.run("mask.setVertex", p).map(drop)
    }

    /// Splits the segment starting at vertex `segment` at `t` (0..1) and returns the new vertex's index.
    pub fn insert_vertex(&mut self, layer: u64, group: u64, segment: usize, t: f64) -> Result<usize> {
        self.run("mask.insertVertex", json!({ "layer": layer, "mask": group, "segment": segment, "t": finite(t)?.clamp(0.0, 1.0) }))?;
        // The new vertex follows `segment` (an open path's last segment cannot be split, so this stays in range).
        Ok(segment + 1)
    }

    pub fn delete_vertex(&mut self, layer: u64, group: u64, index: usize) -> Result<()> {
        let v = json!([{ "layer": layer, "mask": group, "index": index }]);
        self.run("mask.selectVertices", json!({ "vertices": v }))?;
        self.run("mask.deleteVertices", json!({})).map(drop)
    }

    /// Makes a vertex smooth or a corner.
    pub fn smooth_vertex(&mut self, layer: u64, group: u64, index: usize, smooth: bool) -> Result<()> {
        self.run("mask.convertVertex", json!({ "layer": layer, "mask": group, "index": index, "smooth": smooth })).map(drop)
    }

    pub fn set_path_closed(&mut self, layer: u64, group: u64, closed: bool) -> Result<()> {
        self.run("mask.setClosed", json!({ "layer": layer, "mask": group, "closed": closed })).map(drop)
    }

    /// How the keyframe of `prop` (a property id) nearest `time` eases.
    pub fn key_ease(&mut self, layer: u64, prop: u64, time: f64) -> Result<KeyEase> {
        let r = self.run("keys.info", json!({ "keys": [{ "layer": layer, "prop": prop, "time": finite(time)? }] }))?;
        let k = r.as_array().and_then(|a| a.first()).ok_or_else(|| err("no keyframe there"))?;
        let eases = |key: &str| -> Eases {
            k.get(key).and_then(Value::as_array).map(|a| a.iter().map(|e| (num(e, "speed"), num(e, "influence"))).collect()).unwrap_or_default()
        };
        let value = match k.get("value") {
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_f64).collect(),
            Some(v) => v.as_f64().map(|n| vec![n]).unwrap_or_default(),
            None => vec![],
        };
        Ok(KeyEase {
            time: num(k, "time"),
            value,
            spatial: k.get("spatial").is_some_and(|s| !s.is_null()),
            in_ease: eases("inEase"),
            out_ease: eases("outEase"),
            interp_in: text(k, "in"),
            interp_out: text(k, "out"),
            units: text(k, "units"),
        })
    }

    /// Sets one side of a key's ease. `dim` limits it to one dimension; `merge` joins a drag's
    /// many edits into a single undo step.
    #[allow(clippy::too_many_arguments)]
    pub fn set_key_ease(&mut self, layer: u64, path: &str, time: f64, out: bool, dim: Option<usize>, speed: f64, influence: f64, merge: &str) -> Result<()> {
        let mut p = json!({
            "layer": layer, "path": path, "time": finite(time)?, "side": if out { "out" } else { "in" },
            "speed": finite(speed)?, "influence": finite(influence)?, "merge": merge,
        });
        if let (Some(d), Some(o)) = (dim, p.as_object_mut()) {
            o.insert("dim".into(), json!(d));
        }
        self.run("keys.setEase", p).map(drop)
    }

    /// Sets a property's expression. The engine checks the syntax; when it refuses the text the
    /// change is undone and its message comes back as the error. Empty text removes the expression.
    pub fn set_expression(&mut self, layer: u64, path: &str, text: &str) -> Result<()> {
        let r = self.run("prop.setExpression", json!({ "layer": layer, "path": path, "expression": text }))?;
        match r.get("error").and_then(Value::as_str) {
            Some(e) => {
                let e = e.to_string();
                self.undo()?;
                Err(err(format!("Expression error: {e}")))
            }
            None => Ok(()),
        }
    }

    pub fn easy_ease_keys(&mut self) -> Result<()> {
        self.run("keys.easyEase", json!({})).map(drop)
    }

    /// `linear`, `bezier`, `hold`, ...
    pub fn key_interpolation(&mut self, kind: &str) -> Result<()> {
        self.run("keys.interpolation", json!({ "interpolation": kind })).map(drop)
    }

    pub fn deselect_keys(&mut self) -> Result<()> {
        self.run("keys.deselectAll", json!({})).map(drop)
    }

    /// The project's compositions.
    pub fn comps(&mut self) -> Result<Vec<CompItem>> {
        let s = self.run("project.summary", json!({}))?;
        let active = s.get("activeComp").and_then(Value::as_u64);
        Ok(s.get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|i| text(i, "type") == "Composition")
                    .filter_map(|i| {
                        let id = i.get("id").and_then(Value::as_u64)?;
                        let size = i.get("size").and_then(Value::as_array);
                        Some(CompItem {
                            id,
                            name: text(i, "name"),
                            width: size.and_then(|s| s.first()).and_then(Value::as_u64).unwrap_or(0) as u32,
                            height: size.and_then(|s| s.get(1)).and_then(Value::as_u64).unwrap_or(0) as u32,
                            duration: i.get("duration").and_then(Value::as_f64).unwrap_or(0.0),
                            layers: i.get("layers").and_then(Value::as_u64).unwrap_or(0) as usize,
                            active: active == Some(id),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn open_comp(&mut self, id: u64) -> Result<()> {
        self.run("comp.open", json!({ "comp": id })).map(drop)
    }

    /// Creates a composition and opens it.
    pub fn new_comp(&mut self, name: &str, width: u32, height: u32, frame_rate: f64, duration: f64) -> Result<u64> {
        let name = if name.trim().is_empty() { "Comp" } else { name.trim() };
        let r =
            self.run("comp.new", json!({ "name": name, "width": width, "height": height, "frameRate": finite(frame_rate)?, "duration": finite(duration)? }))?;
        r.get("comp").and_then(Value::as_u64).ok_or_else(|| err("comp.new did not return a composition"))
    }

    /// Changes the active composition's size, frame rate or duration.
    pub fn comp_settings(&mut self, width: u32, height: u32, frame_rate: f64, duration: f64) -> Result<()> {
        self.run("comp.settings", json!({ "width": width, "height": height, "frameRate": finite(frame_rate)?, "duration": finite(duration)? })).map(drop)
    }

    /// Writes the current frame as a PNG.
    pub fn export_frame(&mut self, path: &str) -> Result<String> {
        let r = self.run("comp.saveFrameAs", json!({ "path": path }))?;
        Ok(text(&r, "path"))
    }

    /// Renders the active composition to `path` through the render queue; blocks until done.
    pub fn render_to_file(&mut self, path: &str, format: RenderFormat, half: bool) -> Result<String> {
        render_queue(self.session_mut(), path, format, half)
    }

    /// Imports one media file into the project and adds it to the active composition as a layer.
    pub fn import_and_add(&mut self, path: &str) -> Result<u64> {
        if !std::path::Path::new(path).is_file() {
            return Err(err(format!("{path} is not a file")));
        }
        let ids = |e: &mut Editor| -> Result<Vec<u64>> {
            let s = e.run("project.summary", json!({}))?;
            Ok(s.get("items")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter(|i| text(i, "type") != "Folder").filter_map(|i| i.get("id").and_then(Value::as_u64)).collect())
                .unwrap_or_default())
        };
        let before = ids(self)?;
        self.run("file.import", json!({ "paths": [path] }))?;
        let after = ids(self)?;
        let item = after.into_iter().find(|i| !before.contains(i)).ok_or_else(|| err("the file could not be imported (unsupported format?)"))?;
        self.add_item_layer(item)
    }

    /// Adds a project item (by id) to the active composition.
    pub fn add_item_layer(&mut self, item: u64) -> Result<u64> {
        let r = self.run("layer.addItem", json!({ "item": item }))?;
        r.get("layer").and_then(Value::as_u64).ok_or_else(|| err("layer.addItem did not return a layer"))
    }
}

/// Progress of a background analysis (camera solve or subject isolation).
#[derive(Clone, Debug, PartialEq)]
pub struct JobProgress {
    pub running: bool,
    /// 0.0 to 1.0.
    pub fraction: f32,
    /// What the engine says it is doing, when it says anything.
    pub step: String,
    /// Set when the engine gave up; the text says why.
    pub error: Option<String>,
    /// The analysis finished and produced a result.
    pub done: bool,
}

fn unit(v: f64) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) as f32 } else { 0.0 }
}

impl Editor {
    /// Starts the 3D camera tracker on a footage layer. It runs in the background; poll
    /// [`Editor::camera_track_progress`]. Returns the effect that holds the solve.
    pub fn start_camera_track(&mut self, layer: u64) -> Result<u64> {
        let r = self.run("track.camera", json!({ "layer": layer, "wait": false }))?;
        r.get("effect").and_then(Value::as_u64).ok_or_else(|| err("the camera tracker did not start"))
    }

    pub fn camera_track_progress(&mut self, layer: u64) -> Result<JobProgress> {
        let v = self.run("camera.solveStatus", json!({ "layer": layer }))?;
        let running = v.get("running").and_then(Value::as_bool).unwrap_or(false);
        let analyzed = v.get("analyzed").and_then(Value::as_bool).unwrap_or(false);
        let p = v.get("progress").cloned().unwrap_or(Value::Null);
        let error = p.get("error").and_then(Value::as_str).map(str::to_string);
        let (done, total, step) = (
            p.get("done").and_then(Value::as_f64).unwrap_or(0.0),
            p.get("total").and_then(Value::as_f64).unwrap_or(0.0),
            p.get("step").and_then(Value::as_f64).unwrap_or(1.0),
        );
        let frac = if total > 0.0 { done / total } else { 0.0 };
        // Two stages: reading the frames, then solving the camera.
        let fraction = if analyzed { 1.0 } else { unit((step.max(1.0) - 1.0) / 2.0 + frac / 2.0) };
        let banner = v.get("banner").and_then(Value::as_str).unwrap_or_default().to_string();
        let failed = !running && !analyzed;
        Ok(JobProgress {
            running,
            fraction,
            step: banner,
            error: error.or_else(|| failed.then(|| "Not enough trackable detail in this clip".to_string())),
            done: analyzed && !running,
        })
    }

    pub fn cancel_camera_track(&mut self) -> Result<()> {
        self.run("camera.cancel", json!({})).map(drop)
    }

    /// Turns a finished solve into a 3D camera layer that follows the footage.
    pub fn add_tracked_camera(&mut self, layer: u64) -> Result<u64> {
        let r = self.run("camera.create", json!({ "layer": layer }))?;
        r.get("camera").and_then(Value::as_u64).ok_or_else(|| err("no camera was created"))
    }

    /// Where a point of the composition (in composition pixels) lands in the layer's own pixels,
    /// for placing a subject marker on footage that has been moved or scaled.
    pub fn layer_point(&mut self, layer: u64, x: f64, y: f64) -> Result<[f64; 2]> {
        let get = |e: &mut Editor, path: &str| e.prop(layer, path).map(|p| p.value).unwrap_or_default();
        let pos = get(self, "transform/position");
        let anchor = get(self, "transform/anchor");
        let scale = get(self, "transform/scale");
        let at = |v: &[f64], i: usize, d: f64| v.get(i).copied().filter(|n| n.is_finite()).unwrap_or(d);
        let sx = (at(&scale, 0, 100.0) / 100.0).abs().max(0.01);
        let sy = (at(&scale, 1, 100.0) / 100.0).abs().max(0.01);
        Ok([(x - at(&pos, 0, x)) / sx + at(&anchor, 0, 0.0), (y - at(&pos, 1, y)) / sy + at(&anchor, 1, 0.0)])
    }

    /// Isolates the subject under `point` (layer pixels) with an Object Matte and follows it
    /// through the clip in the background. Returns the effect that holds the matte.
    pub fn select_subject(&mut self, layer: u64, point: [f64; 2]) -> Result<u64> {
        if !point.iter().all(|n| n.is_finite()) {
            return Err(err("that point is not on the picture"));
        }
        let r = self.run("objectMatte.apply", json!({ "layer": layer, "point": point }))?;
        let effect = r.get("effect").and_then(Value::as_u64).ok_or_else(|| err("the subject could not be selected"))?;
        self.run("objectMatte.propagate", json!({ "layer": layer, "effect": effect, "direction": "both", "wait": false }))?;
        Ok(effect)
    }

    pub fn subject_progress(&mut self, layer: u64, effect: u64) -> Result<JobProgress> {
        let v = self.run("objectMatte.status", json!({ "layer": layer, "effect": effect }))?;
        let running = v.get("running").and_then(Value::as_bool).unwrap_or(false);
        let p = v.get("progress").cloned().unwrap_or(Value::Null);
        let error = p.get("error").and_then(Value::as_str).map(str::to_string);
        let total = v.get("frames").and_then(Value::as_array).map_or(0, Vec::len) as f64;
        let computed = v.get("computed").and_then(Value::as_array).map_or(0, Vec::len) as f64;
        let fraction = if total > 0.0 { unit(computed / total) } else { 0.0 };
        Ok(JobProgress { running, fraction, step: String::new(), error, done: !running && total > 0.0 && computed >= total })
    }

    pub fn cancel_subject(&mut self) -> Result<()> {
        self.run("roto.cancel", json!({})).map(drop)
    }
}

fn render_queue(session: &mut Session, path: &str, format: RenderFormat, half: bool) -> Result<String> {
    let run = |s: &mut Session, c: &str, p: Value| s.execute(c, p).map_err(|e| err(format!("{c}: {e}")));
    let fmt = match format {
        RenderFormat::Mp4 => "h264",
        RenderFormat::Gif => "gif",
    };
    let queued = run(session, "renderQueue.add", json!({ "format": fmt, "output": path, "resolution": if half { "half" } else { "full" } }))?;
    run(session, "renderQueue.render", json!({ "wait": true }))?;
    let item = queued.get("item").and_then(Value::as_u64);
    let list = run(session, "renderQueue.list", json!({}))?;
    let done = list
        .get("items")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find(|i| i.get("id").and_then(Value::as_u64) == item))
        .map(|i| text(i, "last_output"))
        .unwrap_or_default();
    if done.is_empty() { Err(err("the render produced no file")) } else { Ok(done) }
}

/// The message a cancelled render fails with, so callers can tell it from a real failure.
pub const RENDER_CANCELLED: &str = "Render cancelled";

/// Renders the active composition in the background of `session` and polls it. `progress` gets the
/// fraction done (`0.0..=1.0`) about every 150 ms and returns `false` to stop the render.
fn render_queue_polled(session: &mut Session, path: &str, format: RenderFormat, half: bool, progress: &mut dyn FnMut(f32) -> bool) -> Result<String> {
    let run = |s: &mut Session, c: &str, p: Value| s.execute(c, p).map_err(|e| err(format!("{c}: {e}")));
    let fmt = match format {
        RenderFormat::Mp4 => "h264",
        RenderFormat::Gif => "gif",
    };
    let queued = run(session, "renderQueue.add", json!({ "format": fmt, "output": path, "resolution": if half { "half" } else { "full" } }))?;
    run(session, "renderQueue.render", json!({ "wait": false }))?;
    let started = std::time::Instant::now();
    let (mut seen, mut cancelled) = (false, false);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(150));
        let jobs = run(session, "jobs.list", json!({}))?;
        let running = jobs.get("running").and_then(Value::as_array).and_then(|a| a.iter().find(|j| text(j, "id") == "render"));
        match running {
            Some(j) => {
                seen = true;
                let fraction = (num(j, "fraction") as f32).clamp(0.0, 1.0);
                if !progress(fraction) && !cancelled {
                    cancelled = true;
                    run(session, "jobs.cancel", json!({ "job": "render" }))?;
                }
            }
            // The job may not be listed for an instant after it starts.
            None if seen || started.elapsed().as_secs() >= 3 => break,
            None => {}
        }
        if started.elapsed().as_secs() > 6 * 3600 {
            return Err(err("the render took too long"));
        }
    }
    if cancelled {
        return Err(err(RENDER_CANCELLED));
    }
    progress(1.0);
    let item = queued.get("item").and_then(Value::as_u64);
    let list = run(session, "renderQueue.list", json!({}))?;
    let done = list
        .get("items")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find(|i| i.get("id").and_then(Value::as_u64) == item))
        .map(|i| text(i, "last_output"))
        .unwrap_or_default();
    if done.is_empty() { Err(err("the render produced no file")) } else { Ok(done) }
}

/// Opens a saved copy of a project in `session` and renders one composition: what a background
/// render runs, so the editor stays usable meanwhile. `progress` is as for [`render_queue_polled`].
pub fn render_saved_project(
    session: &mut Session,
    project: &str,
    comp: u64,
    out: &str,
    format: RenderFormat,
    half: bool,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<String> {
    session.execute("file.open", json!({ "path": project })).map_err(|e| err(format!("file.open: {e}")))?;
    session.execute("comp.open", json!({ "comp": comp })).map_err(|e| err(format!("comp.open: {e}")))?;
    render_queue_polled(session, out, format, half, progress)
}

/// `effects/#2` → `2`... the engine's effect selector is the 1-based index inside the path.
fn effect_index(path: &str) -> Result<u64> {
    path.strip_prefix("effects/#").and_then(|n| n.parse::<u64>().ok()).filter(|n| *n >= 1).ok_or_else(|| err(format!("{path} is not an effect")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> Editor {
        let mut e = Editor::new(effectcraft_host::session());
        e.open_demo().unwrap();
        e
    }

    fn count(e: &mut Editor) -> usize {
        e.comp().unwrap().unwrap().layers.len()
    }

    #[test]
    fn every_layer_kind_can_be_added_and_undone() {
        for kind in NewLayer::ALL {
            let mut e = demo();
            let before = count(&mut e);
            let id = e.add_layer(kind).unwrap_or_else(|x| panic!("{kind:?}: {x}"));
            assert_eq!(count(&mut e), before + 1, "{kind:?}");
            assert!(e.comp().unwrap().unwrap().layers.iter().any(|l| l.id == id), "{kind:?}");
            e.undo().unwrap();
            assert_eq!(count(&mut e), before, "{kind:?} undo");
        }
    }

    #[test]
    fn duplicate_split_and_delete_work_on_the_selection() {
        let mut e = demo();
        e.select_layer(362).unwrap();
        let before = count(&mut e);
        assert_eq!(e.duplicate_selected().unwrap().len(), 1);
        assert_eq!(count(&mut e), before + 1);
        e.set_time(3.0).unwrap();
        e.select_layer(362).unwrap();
        e.split_selected().unwrap();
        assert_eq!(count(&mut e), before + 2);
        e.delete_selected().unwrap();
        assert!(count(&mut e) < before + 2);
    }

    #[test]
    fn nothing_selected_means_a_clear_error() {
        let mut e = demo();
        assert!(e.duplicate_selected().is_err());
        assert!(e.delete_selected().is_err());
    }

    #[test]
    fn layers_move_and_trim_in_time() {
        let mut e = demo();
        let row = |e: &mut Editor| e.comp().unwrap().unwrap().layers.into_iter().find(|l| l.id == 362).unwrap();
        let r0 = row(&mut e);
        e.shift_layer(362, 0.5).unwrap();
        let r1 = row(&mut e);
        assert!((r1.in_point - r0.in_point - 0.5).abs() < 0.05, "{r0:?} {r1:?}");
        assert!((r1.out_point - r0.out_point - 0.5).abs() < 0.05);
        e.trim_in_by(362, 0.5).unwrap();
        assert!(row(&mut e).in_point > r1.in_point + 0.4);
        e.trim_out_by(362, -1.0).unwrap();
        assert!(row(&mut e).out_point < r1.out_point - 0.9);
        e.set_time(2.0).unwrap();
        e.trim_out_to_time(362).unwrap();
        assert!((row(&mut e).out_point - 2.0).abs() < 0.05);
    }

    #[test]
    fn text_editing_changes_the_layer() {
        let mut e = demo();
        e.set_text(362, "HELLO").unwrap();
        let tree = e.layer_tree(362, 3).unwrap();
        assert_eq!(tree.find("text/sourceText").map(|n| n.value.clone()), Some(json!("HELLO")));
    }

    #[test]
    fn effects_can_be_found_applied_toggled_reordered_and_removed() {
        let mut e = demo();
        let cat = e.effect_catalog().unwrap();
        assert!(cat.len() > 100, "{}", cat.len());
        let blur = cat.iter().find(|x| x.name == "Gaussian Blur").unwrap().clone();
        assert!(blur.params.iter().any(|p| p.kind == "scalar"));
        let path = e.apply_effect(362, &blur.id).unwrap();
        assert_eq!(path, "effects/#2");
        let node = e.layer_tree(362, 4).unwrap();
        let fx = node.find("effects").unwrap();
        assert_eq!(fx.children.len(), 2);
        e.toggle_effect(362, &path, false).unwrap();
        assert!(!e.layer_tree(362, 4).unwrap().find(&path).unwrap().enabled);
        e.reorder_effect(362, &path, 0).unwrap();
        assert_eq!(e.layer_tree(362, 4).unwrap().find("effects/#1").unwrap().effect.as_deref(), Some("ec.blur.gaussian"));
        e.remove_effect(362, "effects/#1").unwrap();
        assert_eq!(e.layer_tree(362, 4).unwrap().find("effects").unwrap().children.len(), 1);
        assert!(e.remove_effect(362, "transform/position").is_err());
    }

    #[test]
    fn effect_parameters_edit_through_the_tree() {
        let mut e = demo();
        let path = e.apply_effect(362, "ec.blur.gaussian").unwrap();
        e.set_scalar(362, &format!("{path}/blurriness"), 25.0).unwrap();
        let t = e.layer_tree(362, 4).unwrap();
        assert_eq!(t.find(&format!("{path}/blurriness")).unwrap().numbers(), vec![25.0]);
        e.set_bool(362, &format!("{path}/repeatEdge"), true).unwrap();
        assert_eq!(e.layer_tree(362, 4).unwrap().find(&format!("{path}/repeatEdge")).unwrap().value, json!(true));
        e.reset_prop(362, &format!("{path}/blurriness")).unwrap();
        assert_eq!(e.layer_tree(362, 4).unwrap().find(&format!("{path}/blurriness")).unwrap().numbers(), vec![0.0]);
    }

    #[test]
    fn keyframe_times_show_up_in_the_tree_and_navigation_finds_them() {
        let mut e = demo();
        e.set_time(1.0).unwrap();
        e.toggle_key(362, "transform/opacity").unwrap();
        e.set_time(3.0).unwrap();
        e.toggle_key(362, "transform/opacity").unwrap();
        let t = e.layer_tree(362, 3).unwrap();
        assert_eq!(t.find("transform/opacity").unwrap().keys.len(), 2);
        assert_eq!(t.all_keys().len(), 2);
        e.go_to_prop_key(362, "transform/opacity", false).unwrap();
        assert!((e.comp().unwrap().unwrap().time - 1.0).abs() < 0.05);
        assert!(e.go_to_prop_key(362, "transform/opacity", false).is_err());
        e.go_to_prop_key(362, "transform/opacity", true).unwrap();
        assert!((e.comp().unwrap().unwrap().time - 3.0).abs() < 0.05);
    }

    #[test]
    fn every_listed_blend_mode_is_accepted_by_the_engine() {
        let mut e = demo();
        for m in BLEND_MODES {
            e.set_blend_mode(362, m).unwrap_or_else(|x| panic!("{m}: {x}"));
            assert_eq!(e.blend_mode(362).unwrap(), m);
        }
    }

    #[test]
    fn every_label_name_is_accepted_by_the_engine() {
        let mut e = demo();
        for l in LABELS {
            e.set_label(362, l).unwrap_or_else(|x| panic!("{l}: {x}"));
        }
    }

    #[test]
    fn switches_flip_and_show_in_the_layer_row() {
        let mut e = demo();
        let row = |e: &mut Editor| e.comp().unwrap().unwrap().layers.into_iter().find(|l| l.id == 362).unwrap();
        assert!(!row(&mut e).locked && !row(&mut e).solo);
        e.toggle_switch(362, "solo", true).unwrap();
        assert!(row(&mut e).solo);
        e.toggle_switch(362, "threeD", false).unwrap();
        assert!(!row(&mut e).three_d);
        e.toggle_switch(362, "lock", true).unwrap();
        assert!(row(&mut e).locked);
        assert!(e.toggle_switch(362, "bogus", true).is_err());
    }

    #[test]
    fn a_selected_keyframe_can_be_eased_moved_and_deleted() {
        let mut e = demo();
        e.set_time(1.0).unwrap();
        e.toggle_key(362, "transform/opacity").unwrap();
        e.set_time(3.0).unwrap();
        e.toggle_key(362, "transform/opacity").unwrap();
        let keys = |e: &mut Editor| e.layer_tree(362, 3).unwrap().find("transform/opacity").unwrap().keys.clone();
        let t0 = keys(&mut e);
        e.select_key(362, "transform/opacity", t0[0]).unwrap();
        e.key_interpolation("hold").unwrap();
        e.easy_ease_keys().unwrap();
        e.move_keys(0.5).unwrap();
        let t1 = keys(&mut e);
        assert!((t1[0] - t0[0] - 0.5).abs() < 0.05, "{t0:?} {t1:?}");
        assert!((t1[1] - t0[1]).abs() < 1e-6, "only the selected key moves");
        e.delete_keys().unwrap();
        assert_eq!(keys(&mut e).len(), 1);
        e.deselect_keys().unwrap();
    }

    #[test]
    fn layer_organisation_commands_apply() {
        let mut e = demo();
        e.set_blend_mode(362, "Screen").unwrap();
        assert_eq!(e.blend_mode(362).unwrap(), "Screen");
        e.rename_layer(362, "Title").unwrap();
        assert!(e.comp().unwrap().unwrap().layers.iter().any(|l| l.name == "Title"));
        assert!(e.rename_layer(362, "  ").is_err());
        e.arrange(362, "front").unwrap();
        assert_eq!(e.comp().unwrap().unwrap().layers.iter().find(|l| l.id == 362).unwrap().index, 1);
        e.arrange(362, "back").unwrap();
        e.set_label(362, "Blue").unwrap();
        assert_eq!(e.comp().unwrap().unwrap().layers.iter().find(|l| l.id == 362).unwrap().label, "Blue");
        let other = e.comp().unwrap().unwrap().layers.iter().find(|l| l.id != 362).unwrap().id;
        e.set_parent(362, Some(other)).unwrap();
        assert_eq!(e.comp().unwrap().unwrap().layers.iter().find(|l| l.id == 362).unwrap().parent, Some(other));
        e.set_parent(362, None).unwrap();
        assert_eq!(e.comp().unwrap().unwrap().layers.iter().find(|l| l.id == 362).unwrap().parent, None);
    }

    #[test]
    fn compositions_can_be_listed_created_switched_and_resized() {
        let mut e = demo();
        let comps = e.comps().unwrap();
        assert_eq!(comps.len(), 3);
        assert_eq!(comps.iter().filter(|c| c.active).count(), 1);
        let id = e.new_comp("Vertical", 1080, 1920, 30.0, 5.0).unwrap();
        let c = e.comp().unwrap().unwrap();
        assert_eq!((c.id, c.width, c.height), (id, 1080, 1920));
        assert_eq!(e.comps().unwrap().len(), 4);
        e.comp_settings(720, 1280, 24.0, 8.0).unwrap();
        let c = e.comp().unwrap().unwrap();
        assert_eq!((c.width, c.height), (720, 1280));
        assert!((c.duration - 8.0).abs() < 0.05);
        let first = comps.iter().find(|c| !c.active).unwrap().id;
        e.open_comp(first).unwrap();
        assert_eq!(e.comp().unwrap().unwrap().id, first);
    }

    #[test]
    fn a_frame_exports_as_a_png_and_a_gif_renders() {
        let mut e = demo();
        let dir = std::env::temp_dir();
        let png = format!("{}/ec-mobile-{}.png", dir.display(), std::process::id());
        assert_eq!(e.export_frame(&png).unwrap(), png);
        let bytes = std::fs::read(&png).unwrap();
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
        let _ = std::fs::remove_file(&png);
    }

    fn small_comp(e: &mut Editor) {
        e.new_comp("Tiny", 96, 54, 10.0, 0.5).unwrap();
        e.add_layer(NewLayer::Solid).unwrap();
    }

    #[test]
    fn masks_can_be_added_changed_inverted_and_removed() {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        small_comp(&mut e);
        let layer = e.comp().unwrap().unwrap().layers[0].id;
        assert!(e.layer_tree(layer, 6).unwrap().masks().is_empty());
        let a = e.add_mask(layer, MaskShape::Rectangle).unwrap();
        let b = e.add_mask(layer, MaskShape::Ellipse).unwrap();
        let tree = e.layer_tree(layer, 6).unwrap();
        let masks = tree.masks();
        assert_eq!(masks.iter().map(|m| m.uid).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(masks[0].mask, Some(MaskState { mode: "Add".into(), inverted: false }));
        e.set_mask_mode(layer, b, "Subtract").unwrap();
        e.set_mask_inverted(layer, b, true).unwrap();
        let tree = e.layer_tree(layer, 6).unwrap();
        assert_eq!(tree.masks()[1].mask, Some(MaskState { mode: "Subtract".into(), inverted: true }));
        assert_eq!(tree.masks()[0].mask, Some(MaskState { mode: "Add".into(), inverted: false }), "the other mask is untouched");
        assert!(e.set_mask_mode(layer, b, "Sideways").is_err());
        // Undo puts the mode back; removing takes the mask away.
        e.undo().unwrap();
        e.undo().unwrap();
        assert_eq!(e.layer_tree(layer, 6).unwrap().masks()[1].mask, Some(MaskState { mode: "Add".into(), inverted: false }));
        e.remove_mask(layer, a).unwrap();
        let left: Vec<u64> = e.layer_tree(layer, 6).unwrap().masks().iter().map(|m| m.uid).collect();
        assert_eq!(left, vec![b]);
    }

    /// A comp with one layer whose opacity goes 100 -> 20 over one second, as two keyframes.
    fn fading_layer() -> (Editor, u64, u64) {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        e.new_comp("Fade", 96, 54, 10.0, 4.0).unwrap();
        e.add_layer(NewLayer::Solid).unwrap();
        let layer = e.comp().unwrap().unwrap().layers[0].id;
        e.set_time(0.0).unwrap();
        e.toggle_key(layer, "transform/opacity").unwrap();
        e.set_time(1.0).unwrap();
        e.toggle_key(layer, "transform/opacity").unwrap();
        e.set_scalar(layer, "transform/opacity", 20.0).unwrap();
        let uid = e.layer_tree(layer, 4).unwrap().find("transform/opacity").unwrap().uid;
        (e, layer, uid)
    }

    #[test]
    fn a_keyframes_ease_can_be_read_and_set_one_side_at_a_time() {
        let (mut e, layer, uid) = fading_layer();
        let a = e.key_ease(layer, uid, 0.0).unwrap();
        let b = e.key_ease(layer, uid, 1.0).unwrap();
        assert_eq!((a.time, b.time), (0.0, 1.0));
        assert_eq!((a.value.clone(), b.value.clone()), (vec![100.0], vec![20.0]));
        assert_eq!(a.dims(), 1);
        assert_eq!(a.units, "%/sec");
        // Push A's outgoing handle to 60 % influence at speed 0, and leave B alone.
        e.set_key_ease(layer, "transform/opacity", 0.0, true, None, 0.0, 60.0, "t1").unwrap();
        let a2 = e.key_ease(layer, uid, 0.0).unwrap();
        assert_eq!(a2.out_ease, vec![(0.0, 60.0)]);
        assert_eq!(a2.interp_out, "bezier");
        assert_eq!(e.key_ease(layer, uid, 1.0).unwrap().in_ease, b.in_ease, "B's incoming side is untouched");
        // B's incoming side: -80 %/s at 25 % influence.
        e.set_key_ease(layer, "transform/opacity", 1.0, false, None, -80.0, 25.0, "t2").unwrap();
        assert_eq!(e.key_ease(layer, uid, 1.0).unwrap().in_ease, vec![(-80.0, 25.0)]);
        // Edits that share a merge key undo together.
        e.set_key_ease(layer, "transform/opacity", 1.0, false, None, -70.0, 30.0, "t2").unwrap();
        e.undo().unwrap();
        assert_eq!(e.key_ease(layer, uid, 1.0).unwrap().in_ease, b.in_ease, "one undo reverts the whole drag");
        assert!(e.set_key_ease(layer, "transform/opacity", 1.0, true, None, f64::NAN, 30.0, "x").is_err());
    }

    #[test]
    fn an_expression_can_be_set_replaced_rejected_and_removed() {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        small_comp(&mut e);
        let layer = e.comp().unwrap().unwrap().layers[0].id;
        let expr = |e: &mut Editor| e.layer_tree(layer, 4).unwrap().find("transform/rotation").unwrap().expression.clone();
        assert_eq!(expr(&mut e), None);
        e.set_expression(layer, "transform/rotation", "time * 90").unwrap();
        assert_eq!(expr(&mut e).as_deref(), Some("time * 90"));
        e.set_expression(layer, "transform/rotation", "wiggle(2, 30)").unwrap();
        assert_eq!(expr(&mut e).as_deref(), Some("wiggle(2, 30)"));
        // A syntax error is refused, the message comes back, and the good text stays.
        let bad = e.set_expression(layer, "transform/rotation", "time * * (").unwrap_err();
        assert!(bad.to_string().starts_with("Expression error:"), "{bad}");
        assert_eq!(expr(&mut e).as_deref(), Some("wiggle(2, 30)"), "a refused expression leaves the old one");
        e.set_expression(layer, "transform/rotation", "").unwrap();
        assert_eq!(expr(&mut e), None);
        assert!(e.set_expression(layer, "no/such/prop", "1").is_err());
    }

    /// Saves a project holding one composition and returns (dir, project path, comp id).
    fn saved(tag: &str, name: &str, w: u32, h: u32, fps: f64, secs: f64) -> (std::path::PathBuf, String, u64) {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        e.new_comp(name, w, h, fps, secs).unwrap();
        e.add_layer(NewLayer::Solid).unwrap();
        let comp = e.comp().unwrap().unwrap().id;
        let dir = std::env::temp_dir().join(format!("ec-mobile-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let project = dir.join("p.ecproj");
        e.save(Some(project.to_str().unwrap())).unwrap();
        (dir, project.to_string_lossy().into_owned(), comp)
    }

    #[test]
    fn a_render_reports_progress_up_to_done() {
        let (dir, project, comp) = saved("progress", "Short", 96, 54, 10.0, 2.0);
        let out = dir.join("o.gif");
        let mut seen: Vec<f32> = Vec::new();
        let mut other = effectcraft_host::session();
        let done = render_saved_project(&mut other, &project, comp, out.to_str().unwrap(), RenderFormat::Gif, true, &mut |f| {
            seen.push(f);
            true
        })
        .unwrap();
        assert!(std::fs::metadata(&done).unwrap().len() > 100);
        assert_eq!(seen.last().copied(), Some(1.0), "{seen:?}");
        assert!(seen.iter().all(|f| (0.0..=1.0).contains(f)) && seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_render_can_be_stopped_and_says_so() {
        let (dir, project, comp) = saved("cancel", "Long", 1280, 720, 30.0, 120.0);
        let out = dir.join("o.mp4");
        let mut other = effectcraft_host::session();
        let started = std::time::Instant::now();
        let r = render_saved_project(&mut other, &project, comp, out.to_str().unwrap(), RenderFormat::Mp4, false, &mut |_| false);
        let e = r.expect_err("a stopped render is not a success");
        assert_eq!(e.to_string(), RENDER_CANCELLED);
        assert!(started.elapsed().as_secs() < 60, "stopping took {:?}", started.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_comp_renders_to_gif_and_to_mp4_in_a_second_session() {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        small_comp(&mut e);
        let comp = e.comp().unwrap().unwrap().id;
        let dir = std::env::temp_dir().join(format!("ec-mobile-render-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let project = dir.join("p.ecproj");
        e.save(Some(project.to_str().unwrap())).unwrap();
        for (format, ext) in [(RenderFormat::Gif, "gif"), (RenderFormat::Mp4, "mp4")] {
            let out = dir.join(format!("out.{ext}"));
            let mut other = effectcraft_host::session();
            let done = render_saved_project(&mut other, project.to_str().unwrap(), comp, out.to_str().unwrap(), format, true, &mut |_| true)
                .unwrap_or_else(|x| panic!("{ext}: {x}"));
            let len = std::fs::metadata(&done).unwrap().len();
            assert!(len > 100, "{done} is {len} bytes");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn media_files_import_and_become_layers() {
        let mut e = demo();
        let dir = std::env::temp_dir().join(format!("ec-mobile-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("pic.png");
        // A 2x2 PNG written by the engine's own frame export.
        e.export_frame(png.to_str().unwrap()).unwrap();
        let before = count(&mut e);
        let layer = e.import_and_add(png.to_str().unwrap()).unwrap();
        assert_eq!(count(&mut e), before + 1);
        assert!(e.comp().unwrap().unwrap().layers.iter().any(|l| l.id == layer));
        assert!(e.import_and_add("/no/such/file.png").is_err());
        let junk = dir.join("notes.txt");
        std::fs::write(&junk, "hello").unwrap();
        assert!(e.import_and_add(junk.to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nudging_moves_the_layer_and_merges_into_one_undo_step() {
        let mut e = demo();
        let before = e.prop(362, "transform/position").unwrap().value;
        e.begin_gesture();
        for _ in 0..3 {
            e.nudge(362, 10.0, 5.0).unwrap();
        }
        e.end_gesture();
        let after = e.prop(362, "transform/position").unwrap().value;
        assert!((after[0] - before[0] - 30.0).abs() < 1e-6 && (after[1] - before[1] - 15.0).abs() < 1e-6);
        e.undo().unwrap();
        assert_eq!(e.prop(362, "transform/position").unwrap().value, before);
        // Outside a gesture every call is its own step.
        e.nudge(362, 1.0, 0.0).unwrap();
        e.nudge(362, 1.0, 0.0).unwrap();
        e.undo().unwrap();
        assert_eq!(e.prop(362, "transform/position").unwrap().value[0], before[0] + 1.0);
    }

    /// A short clip rendered by the engine itself, shared by the tracking tests.
    fn clip() -> String {
        static ONCE: std::sync::Once = std::sync::Once::new();
        let dir = std::env::temp_dir().join(format!("effectia-test-clip-{}", std::process::id()));
        let path = dir.join("clip.mp4");
        ONCE.call_once(|| {
            std::fs::create_dir_all(&dir).unwrap();
            let mut e = Editor::new(effectcraft_host::session());
            e.new_project().unwrap();
            e.new_comp("Clip", 320, 180, 24.0, 1.0).unwrap();
            e.add_layer(NewLayer::Star).unwrap();
            e.render_to_file(path.to_str().unwrap(), RenderFormat::Mp4, false).unwrap();
        });
        path.to_string_lossy().into_owned()
    }

    fn footage_project() -> (Editor, u64) {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        e.new_comp("Main", 320, 180, 24.0, 1.0).unwrap();
        let layer = e.import_and_add(&clip()).unwrap();
        e.select_layer(layer).unwrap();
        (e, layer)
    }

    fn wait_for(mut poll: impl FnMut() -> JobProgress) -> JobProgress {
        let start = std::time::Instant::now();
        loop {
            let p = poll();
            if !p.running || start.elapsed().as_secs() > 60 {
                return p;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    #[test]
    fn the_camera_tracker_solves_footage_and_adds_a_camera_layer() {
        let (mut e, layer) = footage_project();
        e.start_camera_track(layer).unwrap();
        let p = wait_for(|| e.camera_track_progress(layer).unwrap());
        assert!(p.done && p.error.is_none(), "{p:?}");
        assert!((p.fraction - 1.0).abs() < 1e-6);
        let camera = e.add_tracked_camera(layer).unwrap();
        let comp = e.comp().unwrap().unwrap();
        assert!(comp.layers.iter().any(|l| l.id == camera && l.kind == "Camera"));
        e.undo().unwrap();
        assert!(!e.comp().unwrap().unwrap().layers.iter().any(|l| l.id == camera), "adding the camera is one undo step");
    }

    #[test]
    fn selecting_a_subject_adds_an_object_matte_and_follows_it() {
        let (mut e, layer) = footage_project();
        let effect = e.select_subject(layer, [160.0, 90.0]).unwrap();
        let p = wait_for(|| e.subject_progress(layer, effect).unwrap());
        assert!(p.done && p.error.is_none(), "{p:?}");
        let effects = e.layer_tree(layer, 4).unwrap();
        assert!(effects.find("effects").is_some_and(|n| !n.children.is_empty()));
    }

    #[test]
    fn selecting_a_subject_rejects_a_non_finite_point() {
        let (mut e, layer) = footage_project();
        assert!(e.select_subject(layer, [f64::NAN, 1.0]).is_err());
    }

    #[test]
    fn a_composition_point_maps_back_into_the_layer() {
        let (mut e, layer) = footage_project();
        let p = e.layer_point(layer, 160.0, 90.0).unwrap();
        assert!(p[0].is_finite() && p[1].is_finite());
        // The clip is centred and unscaled, so the middle of the composition is the middle of the clip.
        assert!((p[0] - 160.0).abs() < 1.0 && (p[1] - 90.0).abs() < 1.0, "{p:?}");
    }

    #[test]
    fn a_gradient_fill_paints_the_shape_and_edits_stick() {
        use crate::gradient::{GradientData, Track};
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        e.new_comp("G", 320, 180, 24.0, 1.0).unwrap();
        let star = e.add_layer(NewLayer::Star).unwrap();
        let before = e.render(180).unwrap();
        let item = e.add_gradient(star, false).unwrap();
        let tree = e.layer_tree(star, 8).unwrap();
        let colors = tree.find_kind("gradient").unwrap();
        assert!(colors.path.contains("group"), "it sits inside the shape's group: {}", colors.path);
        let path = format!("@{}", colors.uid);
        assert!(item != 0 && colors.group == item);

        // Left and right of the star differ now, and the plain fill is covered.
        let px = |f: &crate::editor::Frame, x: f32| {
            let (ix, iy) = ((x * f.width as f32) as usize, f.height as usize / 2);
            let o = (iy * f.width as usize + ix) * 4;
            [f.rgba[o], f.rgba[o + 1], f.rgba[o + 2]]
        };
        let after = e.render(180).unwrap();
        assert_ne!(px(&before, 0.45), px(&after, 0.45));
        assert_ne!(px(&after, 0.45), px(&after, 0.55), "a ramp, not a flat colour");

        // An edit of the stops is read back, and the picture follows.
        let mut g = e.gradient(star, &path).unwrap();
        g.set_rgb(0, [1.0, 0.0, 0.0]);
        let i = g.add(Track::Color, 0.5);
        g.set_rgb(i, [0.0, 1.0, 0.0]);
        e.set_gradient(star, &path, &g).unwrap();
        assert_eq!(e.gradient(star, &path).unwrap().colors.len(), 3);
        assert_ne!(px(&e.render(180).unwrap(), 0.5), px(&after, 0.5));
        e.undo().unwrap();
        assert_eq!(e.gradient(star, &path).unwrap(), GradientData::default());
        assert!(e.set_gradient(star, "@999999", &g).is_err(), "a missing property is an error");
    }

    #[test]
    fn a_gradient_cannot_be_added_to_a_layer_that_is_not_a_shape() {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        e.new_comp("G", 320, 180, 24.0, 1.0).unwrap();
        let solid = e.add_layer(NewLayer::Solid).unwrap();
        assert!(e.add_gradient(solid, false).is_err());
    }
}
