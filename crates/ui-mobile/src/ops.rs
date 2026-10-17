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
    pub expression: bool,
    pub children: Vec<PropNode>,
}

impl PropNode {
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
        PropNode {
            path: text(v, "path"),
            name: text(v, "name"),
            kind: text(v, "type"),
            value: v.get("value").cloned().unwrap_or(Value::Null),
            keys,
            enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            effect: v.get("effect").and_then(Value::as_str).map(str::to_string),
            expression: v.get("expression").is_some(),
            children: v.get("children").and_then(Value::as_array).map(|a| a.iter().map(PropNode::parse).collect()).unwrap_or_default(),
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

/// Opens a saved copy of a project in `session` and renders one composition: what a background
/// render runs, so the editor stays usable meanwhile.
pub fn render_saved_project(session: &mut Session, project: &str, comp: u64, out: &str, format: RenderFormat, half: bool) -> Result<String> {
    session.execute("file.open", json!({ "path": project })).map_err(|e| err(format!("file.open: {e}")))?;
    session.execute("comp.open", json!({ "comp": comp })).map_err(|e| err(format!("comp.open: {e}")))?;
    render_queue(session, out, format, half)
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
            let done =
                render_saved_project(&mut other, project.to_str().unwrap(), comp, out.to_str().unwrap(), format, true).unwrap_or_else(|x| panic!("{ext}: {x}"));
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
}
