//! A typed facade over the engine [`Session`] for the touch interface.
//!
//! Every action goes through the same command registry the desktop app, CLI and MCP server use
//! (`layer.select`, `prop.set`, `edit.undo`, `file.save`, ...), so undo history, dirty tracking and
//! the project file stay identical across frontends. Nothing here keeps a second copy of the
//! document: reads query the session, writes are commands.
//!
//! Errors are values: a bad id, a missing comp or a malformed reply becomes an [`EditorError`],
//! never a panic (the repository's never-crash rule).

use std::fmt;

use effectcraft_engine::project::ItemId;
use effectcraft_engine::time::Tick;
use effectcraft_engine::{Session, shortcuts::ShortcutTable};
use serde_json::{Value, json};

/// Something that went wrong while talking to the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorError(pub String);

impl fmt::Display for EditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EditorError {}

pub type Result<T> = std::result::Result<T, EditorError>;

fn err(msg: impl Into<String>) -> EditorError {
    EditorError(msg.into())
}

/// One row of the layer list.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerRow {
    pub id: u64,
    /// 1-based position in the layer stack (1 = top).
    pub index: usize,
    pub name: String,
    /// Engine source type: `Text`, `Solid`, `Shape`, `Composition`, `Null`, ...
    pub kind: String,
    /// The video (eye) switch.
    pub visible: bool,
    pub locked: bool,
    pub solo: bool,
    pub selected: bool,
    /// In, out and start times in comp seconds.
    pub in_point: f64,
    pub out_point: f64,
    pub start: f64,
    /// Label colour name (`Red`, `Blue`, ...), see [`crate::tokens::label_color`].
    pub label: String,
    /// The parent layer, if any.
    pub parent: Option<u64>,
    /// Blend mode name (`Normal`, `Screen`, ...).
    pub blend: String,
    /// The 3D layer switch.
    pub three_d: bool,
}

/// The active composition as the interface shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct CompView {
    pub id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub frame_rate: f64,
    /// Current time in seconds.
    pub time: f64,
    pub layers: Vec<LayerRow>,
}

/// The value of one property at the current time.
#[derive(Clone, Debug, PartialEq)]
pub struct PropState {
    pub name: String,
    /// One number for scalars, several for vectors and colours.
    pub value: Vec<f64>,
    pub animated: bool,
    pub key_count: usize,
}

/// A rendered frame, opaque 8-bit RGBA, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The editor: a session plus the few bits of view state the touch UI needs.
pub struct Editor {
    session: Session,
    /// While a drag is in progress: the id that makes its edits one undo step.
    gesture: Option<String>,
    gestures: u64,
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

pub(crate) fn finite(v: f64) -> Result<f64> {
    if v.is_finite() { Ok(v) } else { Err(err("value must be a finite number")) }
}

impl Editor {
    pub fn new(session: Session) -> Editor {
        Editor { session, gesture: None, gestures: 0 }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Starts a drag: until [`Editor::end_gesture`], value edits and moves merge into one undo step.
    pub fn begin_gesture(&mut self) {
        self.gestures += 1;
        self.gesture = Some(format!("mobile-gesture-{}", self.gestures));
    }

    pub fn end_gesture(&mut self) {
        self.gesture = None;
    }

    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    pub(crate) fn run(&mut self, command: &str, mut params: Value) -> Result<Value> {
        if let (Some(key), Some(obj)) = (&self.gesture, params.as_object_mut())
            && matches!(command, "prop.set" | "layer.nudge" | "layer.timing")
            && !obj.contains_key("merge")
        {
            obj.insert("merge".into(), Value::String(key.clone()));
        }
        self.session.execute(command, params).map_err(|e| err(format!("{command}: {e}")))
    }

    /// Starts a new empty project.
    pub fn new_project(&mut self) -> Result<()> {
        self.run("file.newProject", json!({})).map(drop)
    }

    /// Opens a `.ecproj` file.
    pub fn open(&mut self, path: &str) -> Result<()> {
        self.run("file.open", json!({ "path": path })).map(drop)
    }

    /// Opens the built-in demo project (useful for first launch and for tests).
    pub fn open_demo(&mut self) -> Result<()> {
        self.run("file.openDemoProject", json!({})).map(drop)
    }

    /// Saves to `path`, or to the project's own path when `None`. Returns the path written.
    pub fn save(&mut self, path: Option<&str>) -> Result<String> {
        let params = match path {
            Some(p) => json!({ "path": p }),
            None => json!({}),
        };
        let r = self.run("file.save", params)?;
        Ok(text(&r, "path"))
    }

    /// The project's file path, if it has been saved.
    pub fn project_path(&mut self) -> Result<Option<String>> {
        Ok(self.run("project.summary", json!({}))?.get("path").and_then(Value::as_str).map(str::to_string))
    }

    pub fn is_dirty(&mut self) -> Result<bool> {
        Ok(flag(&self.run("project.summary", json!({}))?, "dirty"))
    }

    /// The active composition, or `None` when the project has none.
    pub fn comp(&mut self) -> Result<Option<CompView>> {
        let summary = self.run("project.summary", json!({}))?;
        let Some(active) = summary.get("activeComp").and_then(Value::as_u64) else {
            return Ok(None);
        };
        let selected: Vec<u64> =
            summary.get("selectedLayers").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
        let time = num(&summary, "time");
        let info = self.run("comp.info", json!({}))?;
        let name = summary
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.iter().find(|i| i.get("id").and_then(Value::as_u64) == Some(active)))
            .map(|i| text(i, "name"))
            .unwrap_or_default();
        let layers = info
            .get("layers")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|l| {
                        let id = l.get("id").and_then(Value::as_u64)?;
                        let sw = l.get("switches").cloned().unwrap_or(Value::Null);
                        Some(LayerRow {
                            id,
                            index: l.get("index").and_then(Value::as_u64).unwrap_or(0) as usize,
                            name: text(l, "name"),
                            kind: text(l, "type"),
                            visible: sw.get("video").and_then(Value::as_bool).unwrap_or(true),
                            locked: flag(&sw, "locked"),
                            solo: flag(&sw, "solo"),
                            selected: selected.contains(&id),
                            in_point: num(l, "in"),
                            out_point: num(l, "out"),
                            start: num(l, "start"),
                            label: text(l, "label"),
                            parent: l.get("parent").and_then(Value::as_u64),
                            blend: text(l, "blendMode"),
                            three_d: flag(&sw, "three_d"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(CompView {
            id: active,
            name,
            width: num(&info, "width") as u32,
            height: num(&info, "height") as u32,
            duration: num(&info, "duration"),
            frame_rate: num(&info, "frameRate"),
            time,
            layers,
        }))
    }

    /// Selects exactly one layer.
    pub fn select_layer(&mut self, id: u64) -> Result<()> {
        // The engine accepts unknown ids silently, which would leave the UI showing a selection
        // that does not exist, so check against the active composition first.
        let comp = self.comp()?.ok_or_else(|| err("no composition"))?;
        if !comp.layers.iter().any(|l| l.id == id) {
            return Err(err(format!("layer {id} is not in this composition")));
        }
        self.run("layer.select", json!({ "layers": [id] })).map(drop)
    }

    /// Clears the layer selection.
    pub fn clear_selection(&mut self) -> Result<()> {
        self.run("layer.select", json!({ "layers": [] })).map(drop)
    }

    /// Reads a property (`transform/opacity`, `transform/position`, ...) at the current time.
    pub fn prop(&mut self, layer: u64, path: &str) -> Result<PropState> {
        let r = self.run("prop.get", json!({ "layer": layer, "path": path }))?;
        let value = match r.get("value") {
            Some(Value::Number(n)) => vec![n.as_f64().unwrap_or(0.0)],
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_f64).collect(),
            _ => return Err(err(format!("{path} has no numeric value"))),
        };
        Ok(PropState {
            name: text(&r, "name"),
            value,
            animated: flag(&r, "animated"),
            key_count: r.get("keys").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        })
    }

    /// Sets a scalar property such as opacity or rotation.
    pub fn set_scalar(&mut self, layer: u64, path: &str, value: f64) -> Result<()> {
        let value = finite(value)?;
        self.run("prop.set", json!({ "layer": layer, "path": path, "value": value })).map(drop)
    }

    /// Sets a vector property such as position or scale.
    pub fn set_vec(&mut self, layer: u64, path: &str, value: &[f64]) -> Result<()> {
        for v in value {
            finite(*v)?;
        }
        self.run("prop.set", json!({ "layer": layer, "path": path, "value": value })).map(drop)
    }

    /// Moves a layer to `(x, y)` in comp pixels, keeping its depth.
    /// Flips a layer switch (`video`, `lock`, `solo`, ...).
    pub fn set_switch(&mut self, layer: u64, name: &str, on: bool) -> Result<()> {
        self.run("layer.setSwitch", json!({ "layers": [layer], "switch": name, "value": on })).map(drop)
    }

    pub fn set_position(&mut self, layer: u64, x: f64, y: f64) -> Result<()> {
        let z = self.prop(layer, "transform/position")?.value.get(2).copied().unwrap_or(0.0);
        self.set_vec(layer, "transform/position", &[x, y, z])
    }

    /// Adds or removes a keyframe on the property at the current time.
    pub fn toggle_key(&mut self, layer: u64, path: &str) -> Result<()> {
        self.run("prop.toggleKey", json!({ "layer": layer, "path": path })).map(drop)
    }

    /// Moves the current time, in comp seconds.
    pub fn set_time(&mut self, seconds: f64) -> Result<()> {
        let seconds = finite(seconds)?;
        self.run("time.set", json!({ "time": seconds })).map(drop)
    }

    /// Steps by whole frames (negative = back).
    pub fn step_frames(&mut self, frames: i64) -> Result<()> {
        self.run("time.step", json!({ "frames": frames })).map(drop)
    }

    pub fn undo(&mut self) -> Result<()> {
        self.run("edit.undo", json!({})).map(drop)
    }

    pub fn redo(&mut self) -> Result<()> {
        self.run("edit.redo", json!({})).map(drop)
    }

    pub fn can_undo(&mut self) -> Result<bool> {
        let s = self.run("project.summary", json!({}))?;
        Ok(s.get("undo").and_then(Value::as_array).is_some_and(|u| !u.is_empty()))
    }

    pub fn can_redo(&mut self) -> Result<bool> {
        let r = self.run("command.list", json!({ "filter": "edit.redo" }))?;
        Ok(r.as_array().is_some_and(|rows| rows.iter().any(|c| text(c, "id") == "edit.redo" && flag(c, "enabled"))))
    }

    /// Renders the active comp at the current time, longest side at most `max_side` pixels.
    pub fn render(&mut self, max_side: u32) -> Result<Frame> {
        let comp = self.comp()?.ok_or_else(|| err("no composition to render"))?;
        let (width, height, rgba) =
            self.session.render_rgba8(ItemId(comp.id), Tick::from_seconds_f64(comp.time), max_side).map_err(|e| err(format!("render: {e}")))?;
        Ok(Frame { width, height, rgba })
    }

    /// The active keyboard shortcut table (shared with the desktop frontend).
    pub fn shortcuts(&self) -> &ShortcutTable {
        self.session.shortcuts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> Editor {
        let mut e = Editor::new(effectcraft_host::session());
        e.open_demo().unwrap();
        e
    }

    fn temp_path(tag: &str) -> String {
        let dir = std::env::temp_dir();
        format!("{}/effectcraft-mobile-{}-{tag}.ecproj", dir.display(), std::process::id())
    }

    #[test]
    fn the_demo_project_shows_a_real_composition_with_layers() {
        let mut e = demo();
        let comp = e.comp().unwrap().unwrap();
        assert_eq!((comp.width, comp.height), (1920, 1080));
        assert_eq!(comp.name, "EffectCraft Intro");
        assert_eq!(comp.layers.len(), 8);
        assert!(comp.layers.iter().any(|l| l.name == "EFFECTCRAFT" && l.kind == "Text"));
        assert!(comp.layers.iter().all(|l| !l.selected));
        assert!(comp.frame_rate > 29.0 && comp.frame_rate < 30.0);
    }

    #[test]
    fn selecting_a_layer_is_reflected_and_exclusive() {
        let mut e = demo();
        e.select_layer(362).unwrap();
        let comp = e.comp().unwrap().unwrap();
        let picked: Vec<u64> = comp.layers.iter().filter(|l| l.selected).map(|l| l.id).collect();
        assert_eq!(picked, vec![362]);
        e.select_layer(438).unwrap();
        let picked: Vec<u64> = e.comp().unwrap().unwrap().layers.iter().filter(|l| l.selected).map(|l| l.id).collect();
        assert_eq!(picked, vec![438]);
        e.clear_selection().unwrap();
        assert!(e.comp().unwrap().unwrap().layers.iter().all(|l| !l.selected));
    }

    #[test]
    fn editing_a_property_changes_the_rendered_frame_and_undo_restores_it() {
        let mut e = demo();
        e.set_time(2.0).unwrap();
        let before = e.render(160).unwrap();
        assert_eq!((before.width, before.height), (160, 90));
        assert_eq!(before.rgba.len(), 160 * 90 * 4);

        e.select_layer(362).unwrap();
        assert_eq!(e.prop(362, "transform/opacity").unwrap().value, vec![100.0]);
        e.set_scalar(362, "transform/opacity", 0.0).unwrap();
        assert_eq!(e.prop(362, "transform/opacity").unwrap().value, vec![0.0]);
        let faded = e.render(160).unwrap();
        assert_ne!(before, faded, "hiding the title must change the picture");

        assert!(e.can_undo().unwrap());
        e.undo().unwrap();
        assert_eq!(e.prop(362, "transform/opacity").unwrap().value, vec![100.0]);
        assert_eq!(e.render(160).unwrap(), before, "undo restores the exact frame");

        assert!(e.can_redo().unwrap());
        e.redo().unwrap();
        assert_eq!(e.prop(362, "transform/opacity").unwrap().value, vec![0.0]);
    }

    #[test]
    fn moving_a_layer_keeps_its_depth() {
        let mut e = demo();
        let z = e.prop(362, "transform/position").unwrap().value[2];
        e.set_position(362, 100.0, 200.0).unwrap();
        assert_eq!(e.prop(362, "transform/position").unwrap().value, vec![100.0, 200.0, z]);
    }

    #[test]
    fn keyframes_toggle_at_the_current_time() {
        let mut e = demo();
        e.set_time(1.0).unwrap();
        assert!(!e.prop(362, "transform/opacity").unwrap().animated);
        e.toggle_key(362, "transform/opacity").unwrap();
        let on = e.prop(362, "transform/opacity").unwrap();
        assert!(on.animated && on.key_count == 1, "{on:?}");
        e.toggle_key(362, "transform/opacity").unwrap();
        assert_eq!(e.prop(362, "transform/opacity").unwrap().key_count, 0);
    }

    #[test]
    fn animated_values_interpolate_deterministically() {
        let mut e = demo();
        e.set_time(0.0).unwrap();
        e.set_scalar(362, "transform/opacity", 0.0).unwrap();
        e.toggle_key(362, "transform/opacity").ok();
        e.set_time(2.0).unwrap();
        e.set_scalar(362, "transform/opacity", 100.0).unwrap();
        e.set_time(1.001).unwrap();
        let mid1 = e.prop(362, "transform/opacity").unwrap().value[0];
        e.set_time(1.001).unwrap();
        let mid2 = e.prop(362, "transform/opacity").unwrap().value[0];
        assert_eq!(mid1, mid2, "same time, same value");
        assert!(mid1 > 0.0 && mid1 < 100.0, "halfway between keys is strictly between: {mid1}");
    }

    #[test]
    fn time_stepping_moves_by_whole_frames() {
        let mut e = demo();
        e.set_time(0.0).unwrap();
        e.step_frames(30).unwrap();
        let t = e.comp().unwrap().unwrap().time;
        assert!((t - 1.001).abs() < 0.01, "30 frames at 29.97 fps is about 1.001 s, got {t}");
        e.step_frames(-30).unwrap();
        assert!(e.comp().unwrap().unwrap().time.abs() < 1e-9);
    }

    #[test]
    fn saving_and_reopening_keeps_the_edit() {
        let mut e = demo();
        assert!(!e.is_dirty().unwrap());
        e.set_scalar(362, "transform/opacity", 37.0).unwrap();
        assert!(e.is_dirty().unwrap());
        let path = temp_path("roundtrip");
        assert_eq!(e.save(Some(&path)).unwrap(), path);
        assert!(!e.is_dirty().unwrap());

        let mut fresh = Editor::new(effectcraft_host::session());
        fresh.open(&path).unwrap();
        assert_eq!(fresh.prop(362, "transform/opacity").unwrap().value, vec![37.0]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_new_project_has_no_composition_until_one_is_made() {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        assert!(e.comp().unwrap().is_none());
        assert!(e.render(160).is_err());
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        let mut e = demo();
        assert!(e.select_layer(999_999).is_err());
        assert!(e.prop(999_999, "transform/opacity").is_err());
        assert!(e.prop(362, "transform/nonsense").is_err());
        assert!(e.set_scalar(362, "transform/opacity", f64::NAN).is_err());
        assert!(e.set_vec(362, "transform/position", &[1.0, f64::INFINITY, 0.0]).is_err());
        assert!(e.set_time(f64::NAN).is_err());
        assert!(e.open("/definitely/not/here.ecproj").is_err());
        // The document is untouched by all of the above.
        assert_eq!(e.prop(362, "transform/opacity").unwrap().value, vec![100.0]);
    }

    #[test]
    fn external_keyboard_shortcuts_come_from_the_shared_table() {
        let e = demo();
        let hit = crate::keys::resolve(e.shortcuts(), "Cmd+S", &[]);
        assert!(hit.is_some_and(|b| b.command == "file.save"), "{hit:?}");
    }
}
