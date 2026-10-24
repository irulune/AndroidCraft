//! Drives the touch workspace headlessly (no GPU) and checks what the user would see and do.

use effectia_ui_mobile::editor::Editor;
use effectia_ui_mobile::ui::{MobileApp, apply_theme, timecode};
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

fn harness(w: f32, h: f32, tablet: bool) -> Harness<'static, MobileApp> {
    let mut editor = Editor::new(effectcraft_host::session());
    assert!(editor.open_demo().is_ok());
    harness_with(editor, w, h, tablet)
}

fn harness_with(editor: Editor, w: f32, h: f32, tablet: bool) -> Harness<'static, MobileApp> {
    let mut app = MobileApp::new(editor);
    if tablet {
        app.layout = effectia_ui_mobile::layout::LayoutState::tablet();
    }
    let mut h = Harness::builder().with_size(vec2(w, h)).build_ui_state(|ui, app: &mut MobileApp| app.show(ui), app);
    apply_theme(&h.ctx);
    h.run();
    h
}

#[test]
fn timecode_is_fixed_width_and_total() {
    assert_eq!(timecode(0.0, 30.0), "0:00:00:00");
    assert_eq!(timecode(61.5, 30.0), "0:01:01:15");
    assert_eq!(timecode(3723.0, 30.0), "1:02:03:00");
    assert_eq!(timecode(f64::NAN, f64::NAN), "0:00:00:00");
    assert_eq!(timecode(-5.0, 24.0), "0:00:00:00");
}

#[test]
fn selecting_a_layer_row_fills_the_properties_panel() {
    let mut h = harness(1376.0, 1032.0, true);
    assert!(h.query_by_label("Keyframe Opacity").is_none(), "nothing is selected yet");
    let index = h.state_mut().editor.comp().ok().flatten().and_then(|c| c.layers.iter().find(|l| l.name == "EFFECTIA").map(|l| l.index));
    let index = index.unwrap_or(0);
    h.get_by_label(&format!("Layer {index}, EFFECTIA")).click();
    h.run();
    assert!(h.query_by_label("Keyframe Opacity").is_some());
    let comp = h.state_mut().editor.comp().ok().flatten();
    assert!(comp.is_some_and(|c| c.layers.iter().any(|l| l.selected && l.name == "EFFECTIA")));
}

#[test]
fn the_eye_hides_a_layer_without_selecting_it_and_undo_brings_it_back() {
    let mut h = harness(1376.0, 1032.0, true);
    let visible = |h: &mut Harness<'static, MobileApp>| -> Vec<bool> {
        h.state_mut().editor.comp().ok().flatten().map(|c| c.layers.iter().map(|l| l.visible).collect()).unwrap_or_default()
    };
    let initial = visible(&mut h);
    h.get_by_label("Show EFFECTIA").click();
    h.run();
    let comp = h.state_mut().editor.comp().ok().flatten();
    let row = comp.as_ref().and_then(|c| c.layers.iter().find(|l| l.name == "EFFECTIA"));
    assert!(row.is_some_and(|l| !l.visible && !l.selected), "{row:?}");
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(visible(&mut h), initial);
}

#[test]
fn the_stopwatch_adds_a_keyframe_at_the_current_time() {
    let mut h = harness(1376.0, 1032.0, true);
    h.state_mut().editor.select_layer(362).ok();
    h.run();
    h.get_by_label("Keyframe Opacity").click();
    h.run();
    let p = h.state_mut().editor.prop(362, "transform/opacity").ok();
    assert!(p.is_some_and(|p| p.animated && p.key_count == 1));
}

#[test]
fn undo_is_disabled_until_there_is_something_to_undo_then_works() {
    let mut h = harness(1376.0, 1032.0, true);
    assert!(h.get_by_label("Undo").accesskit_node().is_disabled());
    h.state_mut().editor.select_layer(362).ok();
    assert!(h.state_mut().editor.set_scalar(362, "transform/opacity", 10.0).is_ok());
    h.run();
    assert!(!h.get_by_label("Undo").accesskit_node().is_disabled());
    h.get_by_label("Undo").click();
    h.run();
    let v = h.state_mut().editor.prop(362, "transform/opacity").map(|p| p.value);
    assert_eq!(v, Ok(vec![100.0]));
}

#[test]
fn phone_opens_the_properties_sheet_from_the_tab_bar_and_closes_it() {
    let mut h = harness(390.0, 844.0, false);
    h.state_mut().editor.select_layer(362).ok();
    h.run();
    assert!(h.query_by_label("Keyframe Opacity").is_none());
    h.get_by_label("Properties").click();
    h.run();
    assert!(h.query_by_label("Keyframe Opacity").is_some());
    h.get_by_label("Close properties").click();
    h.run();
    assert!(h.query_by_label("Keyframe Opacity").is_none());
}

#[test]
fn stepping_a_frame_moves_the_timecode() {
    let mut h = harness(1376.0, 1032.0, true);
    let now = |h: &mut Harness<'static, MobileApp>| h.state_mut().editor.comp().ok().flatten().map_or(f64::NAN, |c| c.time);
    let before = now(&mut h);
    h.get_by_label("Next frame").click();
    h.run();
    let step = now(&mut h) - before;
    assert!(step > 0.0 && step < 0.05, "one frame at 29.97 fps, got {step}");
}

#[test]
fn the_viewer_uploads_a_real_frame() {
    let mut h = harness(1376.0, 1032.0, true);
    h.run();
    assert!(h.state_mut().status.is_none(), "{:?}", h.state_mut().status);
}

fn layer_count(h: &mut Harness<'static, MobileApp>) -> usize {
    h.state_mut().editor.comp().ok().flatten().map_or(0, |c| c.layers.len())
}

#[test]
fn the_add_sheet_creates_a_layer_and_undo_removes_it() {
    let mut h = harness(1376.0, 1032.0, true);
    let before = layer_count(&mut h);
    assert!(h.state_mut().open_sheet("add"));
    h.run_steps(4);
    assert!(h.state().sheet_open());
    h.get_by_label("Ellipse").click();
    h.run_steps(4);
    assert_eq!(layer_count(&mut h), before + 1);
    assert!(!h.state().sheet_open(), "the sheet closes after adding");
    h.get_by_label("Undo").click();
    h.run_steps(4);
    assert_eq!(layer_count(&mut h), before);
}

#[test]
fn timeline_chips_duplicate_split_and_delete_the_selected_layer() {
    let mut h = harness(1376.0, 1032.0, true);
    h.state_mut().editor.select_layer(362).ok();
    h.run_steps(4);
    let before = layer_count(&mut h);
    h.get_by_label("Duplicate").click();
    h.run_steps(4);
    assert_eq!(layer_count(&mut h), before + 1);
    h.get_by_label("Delete").click();
    h.run_steps(4);
    assert_eq!(layer_count(&mut h), before);
}

#[test]
fn keyframe_selection_round_trips_through_the_editor() {
    let mut h = harness(1376.0, 1032.0, true);
    h.state_mut().editor.select_layer(362).ok();
    h.run_steps(4);
    h.get_by_label("Keyframe Opacity").click();
    h.run_steps(4);
    assert!(h.state_mut().editor.select_key(362, "transform/opacity", 0.0).is_ok());
    h.state_mut().editor.deselect_keys().ok();
}

#[test]
fn the_effects_tab_applies_an_effect_from_the_browser() {
    let mut h = harness(1376.0, 1032.0, true);
    h.state_mut().editor.select_layer(362).ok();
    h.state_mut().show_panel(true);
    h.run_steps(4);
    assert!(h.state_mut().open_sheet("effects"));
    h.run_steps(4);
    h.get_by_label("Search effects").click();
    h.get_by_label("Search effects").type_text("Gaussian");
    h.run_steps(3);
    h.get_by_label("Gaussian Blur").click();
    h.run_steps(4);
    let tree = h.state_mut().editor.layer_tree(362, 8).ok();
    assert!(tree.is_some_and(|t| t.find("effects").is_some_and(|e| !e.children.is_empty())));
}

#[test]
fn the_comps_sheet_lists_compositions_and_opens_the_new_comp_form() {
    let mut h = harness(1376.0, 1032.0, true);
    assert!(h.state_mut().open_sheet("comps"));
    h.run_steps(4);
    assert!(h.query_by_label("New composition\u{2026}").is_some());
    h.get_by_label("New composition\u{2026}").click();
    h.run_steps(4);
    assert!(h.query_by_label("Create composition").is_some());
}

#[test]
fn sheets_open_and_close_and_phone_layout_has_them_too() {
    let mut h = harness(412.0, 892.0, false);
    assert!(!h.state_mut().open_sheet("nonsense"));
    for name in ["add", "file", "comps", "export", "newcomp"] {
        assert!(h.state_mut().open_sheet(name), "{name}");
        h.run_steps(4);
        assert!(h.state().sheet_open());
        h.get_by_label("Close").click();
        h.run_steps(4);
        assert!(!h.state().sheet_open(), "{name} closes");
    }
}

#[test]
fn the_header_clears_the_system_bars() {
    let mut h = harness(412.0, 892.0, false);
    let plain = h.get_by_label("Undo").rect().top();
    h.state_mut().safe = effectia_ui_mobile::layout::Insets { top: 44.0, left: 0.0, bottom: 24.0, right: 0.0 };
    h.run_steps(3);
    let moved = h.get_by_label("Undo").rect().top();
    assert!(moved >= plain + 40.0, "Undo moved from {plain} to {moved}");
    let bottom = h.get_by_label("Timeline").rect().bottom();
    assert!(bottom <= 892.0 - 24.0 + 0.5, "tool bar bottom {bottom} stays above the gesture bar");
}

#[test]
fn a_render_shows_progress_and_the_export_sheet_stops_it() {
    use effectia_ui_mobile::ops::NewLayer;
    let storage = std::env::temp_dir().join(format!("effectia-ui-render-{}", std::process::id()));
    let mut e = Editor::new(effectcraft_host::session());
    assert!(e.new_project().is_ok());
    assert!(e.new_comp("Long", 1280, 720, 30.0, 120.0).is_ok());
    assert!(e.add_layer(NewLayer::Solid).is_ok());
    let mut h = harness_with(e, 412.0, 892.0, false);
    h.state_mut().storage = storage.clone();
    assert!(h.state_mut().open_sheet("export"));
    h.run_steps(4);
    h.get_by_label("Render MP4, full size").click();
    h.run_steps(4);
    assert!(h.state().rendering().is_some(), "the render started");
    // While it runs the Export sheet offers only a way to stop it.
    assert!(h.state_mut().open_sheet("export"));
    h.run_steps(4);
    assert!(h.query_by_label("Render MP4, full size").is_none());
    h.get_by_label("Stop rendering").click();
    for _ in 0..400 {
        if h.state().rendering().is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        h.run_steps(2);
    }
    assert!(h.state().rendering().is_none(), "stopping ends the render");
    let left: Vec<_> = std::fs::read_dir(storage.join("exports")).map(|d| d.flatten().map(|f| f.path()).collect()).unwrap_or_default();
    assert!(left.is_empty(), "a stopped render leaves no file: {left:?}");
    let _ = std::fs::remove_dir_all(&storage);
}

#[test]
fn a_mask_is_added_from_the_layer_sheet_and_edited_from_its_header() {
    use effectia_ui_mobile::ops::MaskState;
    let mut h = harness(1376.0, 1032.0, true);
    h.state_mut().editor.select_layer(362).ok();
    h.state_mut().show_panel(false);
    h.run_steps(3);
    assert!(h.state_mut().open_sheet("layer"));
    h.run_steps(4);
    h.get_by_label("Add ellipse mask").click();
    h.run_steps(5);
    let tree = h.state_mut().editor.layer_tree(362, 6).ok().unwrap();
    let mask = tree.masks().first().map(|m| (m.uid, m.name.clone()));
    let Some((uid, name)) = mask else { panic!("the layer has no mask") };
    // The header says what the mask does and opens its menu.
    // Its menu leads to editing the points on the picture.
    h.get_by_label(&format!("Options for {name}")).click();
    h.run_steps(4);
    h.get_by_label("Edit points").click();
    h.run_steps(4);
    assert!(h.state().path_editing());
    h.get_by_label("Done editing points").click();
    h.run_steps(3);
    assert!(!h.state().path_editing());
    h.get_by_label(&format!("Options for {name}")).click();
    h.run_steps(4);
    h.get_by_label("Inverted").click();
    h.run_steps(4);
    let state = |h: &mut Harness<'static, MobileApp>| {
        h.state_mut().editor.layer_tree(362, 6).ok().and_then(|t| t.masks().iter().find(|m| m.uid == uid).and_then(|m| m.mask.clone()))
    };
    assert_eq!(state(&mut h), Some(MaskState { mode: "Add".into(), inverted: true }));
    h.get_by_label(&format!("Options for {name}")).click();
    h.run_steps(4);
    h.get_by_label("Mode").click();
    h.run_steps(4);
    h.get_by_label("Intersect").click();
    h.run_steps(4);
    assert_eq!(state(&mut h), Some(MaskState { mode: "Intersect".into(), inverted: true }));
    h.get_by_label(&format!("Options for {name}")).click();
    h.run_steps(4);
    h.get_by_label("Remove mask").click();
    h.run_steps(4);
    assert!(state(&mut h).is_none(), "the mask is gone");
}

/// A layer whose opacity fades 100 -> 20 over one second, selected, on a phone.
fn fading_harness() -> Harness<'static, MobileApp> {
    use effectia_ui_mobile::ops::NewLayer;
    let mut e = Editor::new(effectcraft_host::session());
    assert!(e.new_project().is_ok());
    assert!(e.new_comp("Fade", 320, 180, 24.0, 4.0).is_ok());
    let layer = e.add_layer(NewLayer::Solid).unwrap_or(0);
    assert!(e.set_time(0.0).is_ok());
    assert!(e.toggle_key(layer, "transform/opacity").is_ok());
    assert!(e.set_time(1.0).is_ok());
    assert!(e.toggle_key(layer, "transform/opacity").is_ok());
    assert!(e.set_scalar(layer, "transform/opacity", 20.0).is_ok());
    assert!(e.select_layer(layer).is_ok());
    harness_with(e, 412.0, 892.0, false)
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 0.02 && (a.1 - b.1).abs() < 0.02
}

#[test]
fn the_curve_editor_drags_handles_and_applies_presets() {
    let mut h = fading_harness();
    h.run_steps(4);
    assert!(h.query_by_label("Curve").is_none(), "the Curve chip needs a selected keyframe");
    h.get_by_label("Keyframe Opacity at 0.00 s").click();
    h.run_steps(4);
    h.get_by_label("Curve").click();
    h.run_steps(4);
    assert!(h.query_by_label("Outgoing handle").is_some() && h.query_by_label("Incoming handle").is_some());
    // The editor starts from the keys' own ease: a fresh pair of keys is linear-ish, so on the diagonal.
    let start = h.state_mut().ease_handles();
    assert!(start.is_some(), "handles are shown for two keys with different values");
    // Presets set both handles.
    h.get_by_label("Curve: Easy Ease").click();
    h.run_steps(4);
    let (out, into) = h.state_mut().ease_handles().unwrap();
    assert!(near(out, (1.0 / 3.0, 0.0)) && near(into, (2.0 / 3.0, 1.0)), "{out:?} {into:?}");
    h.get_by_label("Curve: Linear").click();
    h.run_steps(4);
    let (out, into) = h.state_mut().ease_handles().unwrap();
    assert!(near(out, (1.0 / 3.0, 1.0 / 3.0)) && near(into, (2.0 / 3.0, 2.0 / 3.0)), "{out:?} {into:?}");
    // A dragged handle goes where it was put, overshoot included, and only that handle moves.
    assert!(h.state_mut().drag_ease_handle(true, 0.6, 1.3));
    h.run_steps(3);
    let (out2, into2) = h.state_mut().ease_handles().unwrap();
    assert!(near(out2, (0.6, 1.3)), "{out2:?}");
    assert!(near(into2, into), "the other handle stayed: {into2:?}");
    // The engine agrees: the first key now leaves at 60 % influence.
    let Some(layer) = h.state_mut().editor.comp().ok().flatten().and_then(|c| c.layers.first().map(|l| l.id)) else { panic!("no layer") };
    let uid = h.state_mut().editor.layer_tree(layer, 4).ok().and_then(|t| t.find("transform/opacity").map(|n| n.uid)).unwrap_or(0);
    let k = h.state_mut().editor.key_ease(layer, uid, 0.0).ok().unwrap();
    assert!((k.out_ease[0].1 - 60.0).abs() < 0.5, "{:?}", k.out_ease);
    h.get_by_label("Close").click();
    h.run_steps(3);
    assert!(!h.state().sheet_open());
}

#[test]
fn the_curve_editor_needs_two_keyframes() {
    use effectia_ui_mobile::ops::NewLayer;
    let mut e = Editor::new(effectcraft_host::session());
    assert!(e.new_project().is_ok());
    assert!(e.new_comp("One", 320, 180, 24.0, 2.0).is_ok());
    let layer = e.add_layer(NewLayer::Solid).unwrap_or(0);
    assert!(e.toggle_key(layer, "transform/opacity").is_ok());
    assert!(e.select_layer(layer).is_ok());
    let mut h = harness_with(e, 412.0, 892.0, false);
    h.run_steps(4);
    h.get_by_label("Keyframe Opacity at 0.00 s").click();
    h.run_steps(4);
    h.get_by_label("Curve").click();
    h.run_steps(4);
    assert!(h.query_by_label("Outgoing handle").is_none(), "one key has no segment to shape");
    assert!(!h.state().sheet_open());
}

#[test]
fn an_expression_is_added_from_a_snippet_refused_when_broken_and_removed() {
    let mut h = harness(1376.0, 1032.0, true);
    h.state_mut().editor.select_layer(362).ok();
    h.state_mut().show_panel(false);
    h.run_steps(4);
    let expr = |h: &mut Harness<'static, MobileApp>| {
        h.state_mut().editor.layer_tree(362, 4).ok().and_then(|t| t.find("transform/rotation").and_then(|n| n.expression.clone()))
    };
    assert_eq!(expr(&mut h), None);
    h.get_by_label("Rotation options").click();
    h.run_steps(4);
    h.get_by_label("Add expression\u{2026}").click();
    h.run_steps(4);
    h.get_by_label("Insert time * 90").click();
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(4);
    assert_eq!(expr(&mut h).as_deref(), Some("time * 90"));
    // The row is marked, and the menu now edits and removes it.
    h.get_by_label("Rotation options").click();
    h.run_steps(4);
    assert!(h.query_by_label("Edit expression\u{2026}").is_some());
    h.get_by_label("Edit expression\u{2026}").click();
    h.run_steps(4);
    h.get_by_label("Value").type_text("((");
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(4);
    assert!(h.state().sheet_open(), "a broken expression keeps the editor open");
    assert_eq!(expr(&mut h).as_deref(), Some("time * 90"), "and changes nothing");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    h.get_by_label("Rotation options").click();
    h.run_steps(4);
    h.get_by_label("Remove expression").click();
    h.run_steps(4);
    assert_eq!(expr(&mut h), None);
}

/// A full-frame solid with a star in front, nothing selected, on a phone: the ids are (solid, star).
fn stacked_harness() -> (Harness<'static, MobileApp>, u64, u64) {
    use effectia_ui_mobile::ops::NewLayer;
    let mut e = Editor::new(effectcraft_host::session());
    assert!(e.new_project().is_ok());
    assert!(e.new_comp("Stack", 320, 180, 24.0, 2.0).is_ok());
    let solid = e.add_layer(NewLayer::Solid).unwrap_or(0);
    let star = e.add_layer(NewLayer::Star).unwrap_or(0);
    assert!(e.clear_selection().is_ok());
    (harness_with(e, 412.0, 892.0, false), solid, star)
}

fn selected_ids(h: &mut Harness<'static, MobileApp>) -> Vec<u64> {
    h.state_mut().editor.comp().ok().flatten().map(|c| c.layers.iter().filter(|l| l.selected).map(|l| l.id).collect()).unwrap_or_default()
}

#[test]
fn tapping_the_picture_selects_the_layer_under_the_finger_and_cycles_through_the_stack() {
    use effectia_ui_mobile::ui::ViewMode;
    let (mut h, solid, star) = stacked_harness();
    assert_eq!(h.state().view_mode(), ViewMode::Select, "tap-to-select is the default");
    h.run_steps(3);
    assert!(selected_ids(&mut h).is_empty());
    // The centre of the picture holds both layers: the first tap takes the front one.
    h.get_by_label("Composition viewer").click();
    h.run_steps(3);
    assert_eq!(selected_ids(&mut h), vec![star]);
    // Tapping the same spot again takes the one beneath it, then wraps round.
    h.get_by_label("Composition viewer").click();
    h.run_steps(3);
    assert_eq!(selected_ids(&mut h), vec![solid]);
    h.get_by_label("Composition viewer").click();
    h.run_steps(3);
    assert_eq!(selected_ids(&mut h), vec![star]);
    // A point only the solid covers selects it directly, and empty space clears the selection.
    h.state_mut().tap_picture(4.0, 4.0);
    assert_eq!(selected_ids(&mut h), vec![solid]);
    h.state_mut().tap_picture(-40.0, -40.0);
    assert!(selected_ids(&mut h).is_empty());
}

#[test]
fn move_mode_leaves_the_selection_alone_and_the_switch_changes_mode() {
    use effectia_ui_mobile::ui::ViewMode;
    let (mut h, solid, _) = stacked_harness();
    h.run_steps(3);
    assert!(h.state_mut().editor.select_layer(solid).is_ok());
    h.get_by_label("Move mode").click();
    h.run_steps(3);
    assert_eq!(h.state().view_mode(), ViewMode::Move);
    h.get_by_label("Composition viewer").click();
    h.run_steps(3);
    assert_eq!(selected_ids(&mut h), vec![solid], "a tap does nothing in Move mode");
    h.get_by_label("Select mode").click();
    h.run_steps(3);
    assert_eq!(h.state().view_mode(), ViewMode::Select);
}

#[test]
fn dragging_a_layer_under_the_finger_selects_and_moves_it() {
    let (mut h, _, star) = stacked_harness();
    h.run_steps(3);
    let before = h.state_mut().editor.prop(star, "transform/position").ok().map(|p| p.value.clone());
    // The drag starts on the star (the centre), so the star is picked and moved.
    let c = h.get_by_label("Composition viewer").rect().center();
    h.hover_at(c);
    h.run_steps(2);
    h.drag_at(c);
    h.run_steps(2);
    for k in 1..=4 {
        h.hover_at(c + egui::vec2(25.0 * k as f32, 12.0 * k as f32));
        h.run_steps(2);
    }
    h.drop_at(c + egui::vec2(100.0, 48.0));
    h.run_steps(3);
    assert_eq!(selected_ids(&mut h), vec![star]);
    let after = h.state_mut().editor.prop(star, "transform/position").ok().map(|p| p.value.clone());
    assert_ne!(before, after, "the star moved");
}

/// A tap at a screen point (the harness only clicks widgets, and an edge is not one).
fn tap_at(h: &mut Harness<'static, MobileApp>, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
}

/// The points of the first mask on `layer`, read back from the engine.
fn mask_points(h: &mut Harness<'static, MobileApp>, layer: u64) -> effectia_ui_mobile::ops::PathData {
    let tree = h.state_mut().editor.layer_tree(layer, 6).ok().unwrap();
    let prop = tree.masks().first().and_then(|m| m.path_prop().map(|p| p.uid)).unwrap_or(0);
    h.state_mut().editor.path_data(layer, &format!("@{prop}")).unwrap_or_default()
}

#[test]
fn a_masks_points_are_dragged_added_deleted_smoothed_and_closed_on_the_picture() {
    use effectia_ui_mobile::ops::MaskShape;
    let (mut h, solid, _) = stacked_harness();
    h.run_steps(3);
    assert!(h.state_mut().editor.select_layer(solid).is_ok());
    let mask = h.state_mut().editor.add_mask(solid, MaskShape::Rectangle).unwrap_or(0);
    let prop = h.state_mut().editor.layer_tree(solid, 6).ok().and_then(|t| t.masks().first().and_then(|m| m.path_prop().map(|p| p.uid))).unwrap_or(0);
    assert!(mask != 0 && prop != 0);
    assert!(h.state_mut().edit_points(solid, mask, prop));
    h.run_steps(4);
    assert!(h.state().path_editing());
    for i in 1..=4 {
        assert!(h.query_by_label(&format!("Point {i}")).is_some(), "point {i} is shown");
    }
    assert!(h.get_by_label("Delete point").accesskit_node().is_disabled(), "nothing is selected yet");
    let before = mask_points(&mut h, solid);
    assert_eq!(before.vertices.len(), 4);

    // Dragging a point moves it, as one undo step.
    let c = h.get_by_label("Point 3").rect().center();
    h.hover_at(c);
    h.run_steps(2);
    h.drag_at(c);
    h.run_steps(2);
    for k in 1..=4 {
        h.hover_at(c - egui::vec2(12.0 * k as f32, 8.0 * k as f32));
        h.run_steps(2);
    }
    h.drop_at(c - egui::vec2(48.0, 32.0));
    h.run_steps(3);
    let moved = mask_points(&mut h, solid);
    assert_eq!(moved.vertices.len(), 4);
    assert!(
        moved.vertices[2][0] < before.vertices[2][0] - 10.0 && moved.vertices[2][1] < before.vertices[2][1] - 5.0,
        "{:?} -> {:?}",
        before.vertices[2],
        moved.vertices[2]
    );
    assert_eq!(moved.vertices[0], before.vertices[0], "the other points stayed");
    assert!(!h.get_by_label("Delete point").accesskit_node().is_disabled(), "the dragged point is selected");

    // Tapping the middle of an edge adds a point there.
    let a = h.get_by_label("Point 1").rect().center();
    let b = h.get_by_label("Point 2").rect().center();
    tap_at(&mut h, a.lerp(b, 0.5));
    h.run_steps(3);
    let added = mask_points(&mut h, solid);
    assert_eq!(added.vertices.len(), 5, "an edge tap adds a point");
    assert!(added.vertices[1][0] > added.vertices[0][0] && added.vertices[1][0] < added.vertices[2][0], "between its neighbours: {:?}", added.vertices);

    // A point added on an edge keeps the edge's shape with handles along it, so it is smooth: make it a corner, then smooth again.
    assert!(mask_points(&mut h, solid).is_smooth(1));
    h.get_by_label("Make corner").click();
    h.run_steps(3);
    assert!(!mask_points(&mut h, solid).is_smooth(1));
    h.get_by_label("Make smooth").click();
    h.run_steps(3);
    assert!(mask_points(&mut h, solid).is_smooth(1));

    // Delete the new point, then open the path.
    h.get_by_label("Delete point").click();
    h.run_steps(3);
    assert_eq!(mask_points(&mut h, solid).vertices.len(), 4);
    assert!(mask_points(&mut h, solid).closed);
    h.get_by_label("Open path").click();
    h.run_steps(3);
    assert!(!mask_points(&mut h, solid).closed);
    h.get_by_label("Close path").click();
    h.run_steps(3);
    assert!(mask_points(&mut h, solid).closed);

    h.get_by_label("Done editing points").click();
    h.run_steps(3);
    assert!(!h.state().path_editing());
    assert!(h.query_by_label("Point 1").is_none(), "the handles go away");
}

#[test]
fn a_shape_layers_path_is_edited_through_its_group_transform() {
    use serde_json::json;
    let (mut h, _, star) = stacked_harness();
    h.run_steps(3);
    assert!(h.state_mut().editor.select_layer(star).is_ok());
    let ed = &mut h.state_mut().editor;
    let item = ed
        .command("layer.addShapeItem", json!({ "layer": star, "kind": "path" }))
        .ok()
        .and_then(|r| r.get("uid").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);
    assert!(item != 0);
    for p in [[-60.0, -40.0], [60.0, -40.0], [0.0, 50.0]] {
        assert!(ed.command("mask.addVertex", json!({ "layer": star, "mask": item, "point": p })).is_ok());
    }
    let tree = ed.layer_tree(star, 8).ok().unwrap();
    fn find(n: &effectia_ui_mobile::ops::PropNode, group: u64) -> Option<u64> {
        if n.kind == "path" && n.group == group {
            return Some(n.uid);
        }
        n.children.iter().find_map(|c| find(c, group))
    }
    let prop = find(&tree, item).unwrap_or(0);
    assert!(prop != 0, "the path item's path property is in the tree");
    assert!(h.state_mut().edit_points(star, item, prop));
    h.run_steps(4);
    assert!(h.query_by_label("Point 3").is_some());
    let before = h.state_mut().editor.path_data(star, &format!("@{prop}")).unwrap_or_default();
    let c = h.get_by_label("Point 3").rect().center();
    h.hover_at(c);
    h.run_steps(2);
    h.drag_at(c);
    h.run_steps(2);
    for k in 1..=3 {
        h.hover_at(c + egui::vec2(0.0, 10.0 * k as f32));
        h.run_steps(2);
    }
    h.drop_at(c + egui::vec2(0.0, 30.0));
    h.run_steps(3);
    let after = h.state_mut().editor.path_data(star, &format!("@{prop}")).unwrap_or_default();
    assert!(after.vertices[2][1] > before.vertices[2][1] + 10.0, "{:?} -> {:?}", before.vertices[2], after.vertices[2]);
    assert_eq!(after.vertices[0], before.vertices[0]);
}

#[test]
fn a_gradient_is_added_to_a_shape_layer_and_edited_stop_by_stop() {
    let (mut h, _, star) = stacked_harness();
    h.run_steps(3);
    assert!(h.state_mut().editor.select_layer(star).is_ok());
    assert!(h.state_mut().open_sheet("layer"));
    h.run_steps(4);
    h.get_by_label("Add gradient fill").click();
    h.run_steps(5);
    let tree = h.state_mut().editor.layer_tree(star, 8).ok().unwrap();
    let node = tree.find_kind("gradient").cloned();
    let Some(node) = node else { panic!("the gradient fill is in the tree") };
    let path = format!("@{}", node.uid);
    let g0 = h.state_mut().editor.gradient(star, &path).unwrap();
    assert_eq!(g0.colors.len(), 2);

    // The Colors row shows the gradient and opens its editor.
    let row = h.get_by_label("Colors edit gradient").rect();
    assert!(row.max.y < 892.0, "the new gradient is scrolled into view: {row:?}");
    h.get_by_label("Colors edit gradient").click();
    h.run_steps(4);
    assert!(h.query_by_label("Add colour stop").is_some() && h.query_by_label("Colour stop 1").is_some());
    h.get_by_label("Add colour stop").click();
    h.run_steps(4);
    let g1 = h.state_mut().editor.gradient(star, &path).unwrap();
    assert_eq!(g1.colors.len(), 3, "a tap on the bar adds a stop");
    assert!(h.query_by_label("Colour stop 2").is_some());
    assert!(h.query_by_label("Delete stop").is_some(), "the new stop is selected");

    // Opacity stops work the same way, and a stop can be removed again.
    h.get_by_label("Add opacity stop").click();
    h.run_steps(4);
    assert_eq!(h.state_mut().editor.gradient(star, &path).unwrap().opacities.len(), 3);
    h.get_by_label("Stop opacity").click();
    h.run_steps(2);
    h.get_by_label("Delete stop").click();
    h.run_steps(4);
    let g2 = h.state_mut().editor.gradient(star, &path).unwrap();
    assert_eq!((g2.colors.len(), g2.opacities.len()), (3, 2));

    // Dragging a colour stop along the bar moves it, and the engine keeps the result.
    let c = h.get_by_label("Colour stop 2").rect().center();
    h.hover_at(c);
    h.run_steps(2);
    h.drag_at(c);
    h.run_steps(2);
    for k in 1..=4 {
        h.hover_at(c + egui::vec2(20.0 * k as f32, 0.0));
        h.run_steps(2);
    }
    h.drop_at(c + egui::vec2(80.0, 0.0));
    h.run_steps(3);
    let g3 = h.state_mut().editor.gradient(star, &path).unwrap();
    assert_ne!(g3.colors, g2.colors, "the stop moved");
    assert!(g3.colors.windows(2).all(|w| w[0].0 <= w[1].0), "still in order");
    // The whole drag is one undo step.
    assert!(h.state_mut().editor.undo().is_ok());
    assert_eq!(h.state_mut().editor.gradient(star, &path).unwrap(), g2);
}

#[test]
fn a_layer_reference_property_is_pointed_at_a_layer_from_a_list() {
    let (mut h, solid, star) = stacked_harness();
    h.run_steps(3);
    assert!(h.state_mut().editor.select_layer(star).is_ok());
    let fx = h.state_mut().editor.apply_effect(star, "ec.channel.setchannels").unwrap_or_default();
    assert!(!fx.is_empty());
    let prop = format!("{fx}/sourceLayer1");
    let value = |h: &mut Harness<'static, MobileApp>| h.state_mut().editor.layer_tree(star, 8).ok().and_then(|t| t.find(&prop).map(|n| n.value.clone()));
    assert_eq!(value(&mut h), Some(serde_json::Value::Null), "no layer yet");
    h.state_mut().show_panel(true);
    h.state_mut().expand(&fx);
    h.run_steps(4);
    h.get_by_label("Source Layer 1").click();
    h.run_steps(4);
    // The list offers None and every layer; pick the solid.
    let solid_row = h
        .state_mut()
        .editor
        .comp()
        .ok()
        .flatten()
        .and_then(|c| c.layers.iter().find(|l| l.id == solid).map(|l| format!("{}  {}", l.index, l.name)))
        .unwrap_or_default();
    h.get_by_label_contains(&solid_row).click();
    h.run_steps(4);
    assert_eq!(value(&mut h), Some(serde_json::json!(solid)), "the property points at the solid");
    // And back to none.
    h.get_by_label("Source Layer 1").click();
    h.run_steps(4);
    h.get_by_label("None").click();
    h.run_steps(4);
    assert_eq!(value(&mut h), Some(serde_json::Value::Null));
}

#[test]
fn a_property_is_linked_to_another_layers_property_through_two_lists() {
    let mut h = harness(1376.0, 1032.0, true);
    h.state_mut().editor.select_layer(362).ok();
    h.state_mut().show_panel(false);
    h.run_steps(4);
    h.get_by_label("Rotation options").click();
    h.run_steps(4);
    h.get_by_label("Link to another property\u{2026}").click();
    h.run_steps(4);
    // First the layer to follow...
    let tagline = h
        .state_mut()
        .editor
        .comp()
        .ok()
        .flatten()
        .and_then(|c| c.layers.iter().find(|l| l.name == "Tagline").map(|l| format!("{}  {}", l.index, l.name)))
        .unwrap_or_default();
    h.get_by_label_contains(&tagline).click();
    h.run_steps(4);
    // ...then its property.
    h.get_by_label("Transform \u{25b8} Opacity").click();
    h.run_steps(4);
    let expr = h.state_mut().editor.layer_tree(362, 4).ok().and_then(|t| t.find("transform/rotation").and_then(|n| n.expression.clone())).unwrap_or_default();
    assert!(expr.contains("thisComp.layer(\"Tagline\")") && expr.to_lowercase().contains("opacity"), "{expr}");
    assert!(h.state_mut().status.is_none(), "{:?}", h.state_mut().status);
    assert!(!h.state_mut().sheet_open(), "the sheet closes");
}

#[test]
fn point_editing_ends_when_the_layer_is_deselected() {
    use effectia_ui_mobile::ops::MaskShape;
    let (mut h, solid, _) = stacked_harness();
    h.run_steps(3);
    assert!(h.state_mut().editor.select_layer(solid).is_ok());
    let mask = h.state_mut().editor.add_mask(solid, MaskShape::Ellipse).unwrap_or(0);
    let prop = h.state_mut().editor.layer_tree(solid, 6).ok().and_then(|t| t.masks().first().and_then(|m| m.path_prop().map(|p| p.uid))).unwrap_or(0);
    assert!(h.state_mut().edit_points(solid, mask, prop));
    h.run_steps(3);
    assert!(h.state().path_editing());
    assert!(h.state_mut().editor.clear_selection().is_ok());
    h.run_steps(3);
    assert!(!h.state().path_editing(), "nothing left to edit");
}

/// A footage layer selected in a fresh project, from a clip the engine renders itself.
fn footage_harness() -> (Harness<'static, MobileApp>, u64) {
    use effectia_ui_mobile::ops::{NewLayer, RenderFormat};
    let dir = std::env::temp_dir().join(format!("effectia-ui-clip-{}", std::process::id()));
    assert!(std::fs::create_dir_all(&dir).is_ok());
    let clip = dir.join("clip.mp4").to_string_lossy().into_owned();
    let mut e = Editor::new(effectcraft_host::session());
    assert!(e.new_project().is_ok());
    assert!(e.new_comp("Clip", 320, 180, 24.0, 1.0).is_ok());
    assert!(e.add_layer(NewLayer::Star).is_ok());
    assert!(e.render_to_file(&clip, RenderFormat::Mp4, false).is_ok());
    assert!(e.new_project().is_ok());
    assert!(e.new_comp("Main", 320, 180, 24.0, 1.0).is_ok());
    let layer = e.import_and_add(&clip).unwrap_or(0);
    assert!(e.select_layer(layer).is_ok());
    (harness_with(e, 412.0, 892.0, false), layer)
}

fn wait_idle(h: &mut Harness<'static, MobileApp>) {
    for _ in 0..400 {
        if h.state().tracking_idle() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        h.run_steps(2);
    }
}

#[test]
fn the_track_sheet_needs_footage() {
    let mut h = harness(412.0, 892.0, false);
    h.state_mut().editor.select_layer(362).ok();
    h.run_steps(3);
    h.get_by_label("Track").click();
    h.run_steps(3);
    assert!(h.state().sheet_open());
    assert!(h.get_by_label("Track camera (3D)").accesskit_node().is_disabled());
    assert!(h.get_by_label("Select subject").accesskit_node().is_disabled());
}

#[test]
fn tapping_the_subject_isolates_it_and_tracking_the_camera_adds_a_camera() {
    let (mut h, layer) = footage_harness();
    h.run_steps(3);

    // Subject: the Track chip, "Select subject", then a tap on the picture.
    h.get_by_label("Track").click();
    h.run_steps(3);
    h.get_by_label("Select subject").click();
    h.run_steps(3);
    assert!(h.query_by_label("Cancel subject selection").is_some(), "the footer asks for a tap");
    h.get_by_label("Composition viewer").click();
    h.run_steps(3);
    wait_idle(&mut h);
    assert!(h.state().tracking_idle(), "the subject job finished");
    let tree = h.state_mut().editor.layer_tree(layer, 4).ok();
    assert!(tree.is_some_and(|t| t.find("effects").is_some_and(|e| !e.children.is_empty())), "an Object Matte is on the layer");

    // Camera: tracking adds a 3D camera layer when it finishes.
    let before = layer_count(&mut h);
    h.get_by_label("Track").click();
    h.run_steps(3);
    h.get_by_label("Track camera (3D)").click();
    h.run_steps(3);
    wait_idle(&mut h);
    assert!(h.state().tracking_idle(), "the camera job finished");
    assert_eq!(layer_count(&mut h), before + 1, "a camera layer was added");
}

// ------------------------------------------------------------------------------ the media picker

mod picker {
    use super::*;
    use effectia_ui_mobile::platform::{Access, Needs, Platform, Root};
    use std::cell::Cell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    #[derive(Clone)]
    struct Fake {
        media: Rc<Cell<bool>>,
        all: Rc<Cell<bool>>,
        asked: Rc<Cell<u32>>,
        asked_all: Rc<Cell<u32>>,
        settings: Rc<Cell<u32>>,
        roots: Vec<Root>,
    }

    impl Platform for Fake {
        fn media_access(&self) -> Access {
            if self.media.get() { Access::Granted } else { Access::NotGranted }
        }
        fn request_media_access(&self) {
            self.asked.set(self.asked.get() + 1);
        }
        fn all_files_access(&self) -> Access {
            if self.all.get() { Access::Granted } else { Access::NotGranted }
        }
        fn request_all_files_access(&self) {
            self.asked_all.set(self.asked_all.get() + 1);
        }
        fn open_app_settings(&self) {
            self.settings.set(self.settings.get() + 1);
        }
        fn roots(&self) -> Vec<Root> {
            self.roots.clone()
        }
    }

    fn photo(path: &Path, rgb: [u8; 3]) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        image::RgbImage::from_pixel(64, 48, image::Rgb(rgb)).save(path).unwrap();
    }

    /// A mono 8 kHz tone, `secs` long.
    fn tone(path: &Path, secs: f32) {
        let n = (8000.0 * secs) as u32;
        let mut b = Vec::new();
        for part in [&b"RIFF"[..], &(36 + n * 2).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes(), &1u16.to_le_bytes(), &1u16.to_le_bytes()] {
            b.extend_from_slice(part);
        }
        for part in [&8000u32.to_le_bytes()[..], &16000u32.to_le_bytes(), &2u16.to_le_bytes(), &16u16.to_le_bytes(), b"data", &(n * 2).to_le_bytes()] {
            b.extend_from_slice(part);
        }
        for i in 0..n {
            b.extend_from_slice(&(((i as f32 * 0.2).sin() * 8000.0) as i16).to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    struct Rig {
        h: Harness<'static, MobileApp>,
        dev: Fake,
        phone: PathBuf,
    }

    /// A phone whose shared storage holds two camera photos and one in a sub-folder.
    fn rig(tag: &str, granted: bool) -> Rig {
        let base = std::env::temp_dir().join(format!("effectia-picker-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let phone = base.join("phone");
        photo(&phone.join("DCIM/Camera/beach.png"), [200, 160, 40]);
        photo(&phone.join("DCIM/Camera/city.png"), [40, 60, 200]);
        photo(&phone.join("DCIM/Camera/trip/mountain.png"), [30, 160, 90]);
        std::fs::write(phone.join("DCIM/Camera/notes.txt"), "not media").unwrap();
        tone(&phone.join("DCIM/Camera/rain.wav"), 2.0);
        let dev = Fake {
            media: Rc::new(Cell::new(granted)),
            all: Rc::new(Cell::new(false)),
            asked: Rc::default(),
            asked_all: Rc::default(),
            settings: Rc::default(),
            roots: vec![Root::new("Camera", phone.join("DCIM/Camera"), Needs::Media), Root::new("All files", &phone, Needs::AllFiles)],
        };
        let mut editor = Editor::new(effectcraft_host::session());
        assert!(editor.open_demo().is_ok());
        let mut app = MobileApp::new(editor).with_storage(base.join("app")).with_platform(dev.clone());
        app.layout = effectia_ui_mobile::layout::LayoutState::tablet();
        let mut h = Harness::builder().with_size(vec2(900.0, 1000.0)).build_ui_state(|ui, app: &mut MobileApp| app.show(ui), app);
        apply_theme(&h.ctx);
        h.run_steps(2);
        assert!(h.state_mut().open_sheet("media"));
        h.run_steps(2);
        Rig { h, dev, phone }
    }

    /// Steps the UI (workers run on real threads) until `done` holds.
    fn until(h: &mut Harness<'static, MobileApp>, what: &str, done: impl Fn(&Harness<'static, MobileApp>) -> bool) {
        for _ in 0..300 {
            h.run_steps(2);
            if done(h) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        panic!("timed out waiting for: {what}");
    }

    fn layers(h: &mut Harness<'static, MobileApp>) -> usize {
        h.state_mut().editor.comp().ok().flatten().map_or(0, |c| c.layers.len())
    }

    #[test]
    fn without_permission_the_picker_explains_and_asks_then_fills_in_when_it_is_granted() {
        let mut r = rig("ask", false);
        assert!(r.h.query_by_label("Allow access").is_some(), "the explanation and the button show");
        assert!(r.h.query_by_label("Open app settings").is_none(), "settings is only offered after asking once");
        assert!(r.h.query_by_label_contains("beach.png").is_none(), "nothing is listed without access");

        r.h.get_by_label("Allow access").click();
        r.h.run_steps(2);
        assert_eq!(r.dev.asked.get(), 1, "the platform was asked");
        assert!(r.h.query_by_label("Open app settings").is_some(), "after a refusal the way back is offered");
        r.h.get_by_label("Open app settings").click();
        r.h.run_steps(2);
        assert_eq!(r.dev.settings.get(), 1);

        // The user allows it in the system dialog: the picker notices by itself.
        r.dev.media.set(true);
        until(&mut r.h, "photos to appear after the permission arrives", |h| h.query_by_label_contains("beach.png").is_some());
        assert!(r.h.query_by_label("Allow access").is_none(), "the card goes away");
        assert!(r.h.query_by_label_contains("city.png").is_some());
        assert!(r.h.query_by_label_contains("mountain.png").is_some(), "recent looks inside sub-folders");
        assert!(r.h.query_by_label_contains("notes.txt").is_none(), "only what the engine can import");
    }

    #[test]
    fn ticking_photos_and_adding_them_makes_layers() {
        let mut r = rig("add", true);
        until(&mut r.h, "the recent photos", |h| h.query_by_label_contains("beach.png").is_some());
        let before = layers(&mut r.h);
        assert!(r.h.get_by_label("Add selected media").accesskit_node().is_disabled(), "nothing ticked yet");

        r.h.get_by_label_contains("beach.png").click();
        r.h.run_steps(2);
        r.h.get_by_label_contains("city.png").click();
        r.h.run_steps(2);
        assert!(r.h.query_by_label("2 selected").is_some());
        r.h.get_by_label("Add selected media").click();
        r.h.run_steps(4);
        assert_eq!(layers(&mut r.h), before + 2, "both photos became layers");
        assert!(!r.h.state_mut().sheet_open(), "the picker closes");
        assert!(r.h.state_mut().status.is_none());
    }

    #[test]
    fn audio_is_listed_filtered_and_added_as_an_audio_layer() {
        let mut r = rig("audio", true);
        until(&mut r.h, "the recent files", |h| h.query_by_label_contains("rain.wav").is_some());
        assert!(r.h.query_by_label_contains("beach.png").is_some(), "photos and audio share the list");

        r.h.get_by_label("Show audio").click();
        r.h.run_steps(2);
        assert!(r.h.query_by_label_contains("rain.wav").is_some());
        assert!(r.h.query_by_label_contains("beach.png").is_none(), "only audio");
        r.h.get_by_label("Show photos").click();
        r.h.run_steps(2);
        assert!(r.h.query_by_label_contains("rain.wav").is_none());
        assert!(r.h.query_by_label_contains("beach.png").is_some());
        r.h.get_by_label("Show videos").click();
        r.h.run_steps(2);
        assert!(r.h.query_by_label_contains("beach.png").is_none() && r.h.query_by_label_contains("rain.wav").is_none(), "no videos on this phone");
        r.h.get_by_label("Show audio").click();
        r.h.run_steps(2);

        // Its length is read in the background without breaking the grid.
        for _ in 0..40 {
            r.h.run_steps(2);
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let before = layers(&mut r.h);
        r.h.get_by_label_contains("rain.wav").click();
        r.h.run_steps(2);
        r.h.get_by_label("Add selected media").click();
        r.h.run_steps(4);
        assert_eq!(layers(&mut r.h), before + 1, "the sound became a layer");
        assert!(r.h.state_mut().status.is_none(), "{:?}", r.h.state_mut().status);
        let id = r.h.state_mut().editor.comp().ok().flatten().and_then(|c| c.layers.first().map(|l| l.id));
        let tree = id.and_then(|id| r.h.state_mut().editor.layer_tree(id, 3).ok());
        assert!(tree.as_ref().is_some_and(|t| t.find("audio/levels").is_some()), "an audio layer has Audio Levels");
    }

    #[test]
    fn folders_show_audio_too_and_the_filter_applies_there() {
        let mut r = rig("audio-folder", true);
        r.h.get_by_label("Folders").click();
        r.h.run_steps(2);
        r.h.get_by_label("Camera").click();
        until(&mut r.h, "the listing", |h| h.query_by_label_contains("rain.wav").is_some());
        r.h.get_by_label("Show audio").click();
        r.h.run_steps(2);
        assert!(r.h.query_by_label_contains("beach.png").is_none() && r.h.query_by_label_contains("rain.wav").is_some());
        assert!(r.h.query_by_label("trip").is_some(), "folders stay so you can keep browsing");
    }

    #[test]
    fn tapping_an_item_again_unticks_it() {
        let mut r = rig("untick", true);
        until(&mut r.h, "the recent photos", |h| h.query_by_label_contains("beach.png").is_some());
        r.h.get_by_label_contains("beach.png").click();
        r.h.run_steps(2);
        assert!(r.h.query_by_label("1 selected").is_some());
        r.h.get_by_label_contains("beach.png").click();
        r.h.run_steps(2);
        assert!(r.h.query_by_label("Tap items to select them").is_some());
    }

    #[test]
    fn folders_can_be_browsed_down_and_back_up() {
        let mut r = rig("folders", true);
        r.h.get_by_label("Folders").click();
        r.h.run_steps(2);
        r.h.get_by_label("Camera").click();
        until(&mut r.h, "the Camera folder listing", |h| h.query_by_label("trip").is_some());
        assert!(r.h.query_by_label_contains("beach.png").is_some());
        r.h.get_by_label("trip").click();
        until(&mut r.h, "the trip folder listing", |h| h.query_by_label_contains("mountain.png").is_some());
        assert!(r.h.query_by_label_contains("beach.png").is_none(), "only this folder");
        r.h.get_by_label("Up one folder").click();
        until(&mut r.h, "back in Camera", |h| h.query_by_label_contains("beach.png").is_some());
        r.h.get_by_label("Up one folder").click();
        r.h.run_steps(2);
        assert!(r.h.query_by_label("All files").is_some(), "the root list again");
    }

    #[test]
    fn all_files_needs_its_own_permission_and_opens_settings_for_it() {
        let mut r = rig("allfiles", true);
        r.h.get_by_label("Folders").click();
        r.h.run_steps(2);
        r.h.get_by_label("All files").click();
        r.h.run_steps(2);
        assert_eq!(r.dev.asked_all.get(), 1, "tapping it asks for all-files access");
        r.dev.all.set(true);
        until(&mut r.h, "the all-files row to open", |h| h.query_by_label("All files").is_some());
        r.h.get_by_label("All files").click();
        until(&mut r.h, "the storage root to list", |h| h.query_by_label("DCIM").is_some());
    }

    #[test]
    fn a_failed_import_says_so_and_keeps_the_picker_open() {
        let mut r = rig("fail", true);
        until(&mut r.h, "the recent photos", |h| h.query_by_label_contains("beach.png").is_some());
        r.h.get_by_label_contains("beach.png").click();
        r.h.run_steps(2);
        std::fs::write(r.phone.join("DCIM/Camera/beach.png"), "no longer an image").unwrap();
        let before = layers(&mut r.h);
        r.h.get_by_label("Add selected media").click();
        r.h.run_steps(4);
        assert_eq!(layers(&mut r.h), before);
        assert!(r.h.state_mut().sheet_open(), "still open so nothing is lost");
        assert!(r.h.state_mut().status.as_deref().is_some_and(|s| s.contains("beach.png")), "{:?}", r.h.state_mut().status);
    }
}

// ---------------------------------------------------------------------------- the project gallery

mod project_gallery {
    use super::*;
    use std::path::PathBuf;

    fn rig(tag: &str) -> (Harness<'static, MobileApp>, PathBuf) {
        let base = std::env::temp_dir().join(format!("effectia-gallery-ui-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut editor = Editor::new(effectcraft_host::session());
        assert!(editor.open_demo().is_ok());
        let app = MobileApp::new(editor).with_storage(&base);
        let mut h = Harness::builder().with_size(vec2(420.0, 900.0)).build_ui_state(|ui, app: &mut MobileApp| app.show(ui), app);
        apply_theme(&h.ctx);
        h.run_steps(3);
        (h, base)
    }

    fn until(h: &mut Harness<'static, MobileApp>, what: &str, done: impl Fn(&mut Harness<'static, MobileApp>) -> bool) {
        for _ in 0..400 {
            h.run_steps(2);
            if done(h) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        panic!("timed out waiting for: {what}");
    }

    fn projects(base: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(base.join("projects"))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn leaving_for_the_gallery_saves_unsaved_work_and_draws_its_preview() {
        let (mut h, base) = rig("save");
        h.state_mut().editor.set_scalar(362, "transform/opacity", 37.0).ok();
        assert!(h.state_mut().show_gallery());
        assert_eq!(projects(&base), ["Effectia Intro.ecproj"], "named after the composition");
        until(&mut h, "the card", |h| h.query_by_label("Open Effectia Intro").is_some());
        until(&mut h, "the preview to be drawn", |h| !h.state_mut().gallery_busy());
        assert!(base.join("thumbs/Effectia Intro.ecproj.png").is_file());
        assert!(h.query_by_label("New project").is_some() && h.query_by_label("Open demo Effectia Intro").is_some());
    }

    #[test]
    fn an_untouched_project_is_not_written_to_disk() {
        let (mut h, base) = rig("clean");
        assert!(h.state_mut().show_gallery());
        h.run_steps(3);
        assert!(projects(&base).is_empty(), "{:?}", projects(&base));
        assert!(h.state_mut().in_gallery());
    }

    #[test]
    fn tapping_a_card_opens_that_project_with_its_edits() {
        let (mut h, _base) = rig("open");
        h.state_mut().editor.set_scalar(362, "transform/opacity", 37.0).ok();
        h.state_mut().show_gallery();
        until(&mut h, "the card", |h| h.query_by_label("Open Effectia Intro").is_some());
        // Open the demo first so the saved edit is not simply still in the session.
        h.get_by_label("Open demo Effectia Intro").click();
        h.run_steps(3);
        assert!(!h.state_mut().in_gallery());
        assert_eq!(h.state_mut().editor.prop(362, "transform/opacity").map(|p| p.value), Ok(vec![100.0]));
        h.state_mut().show_gallery();
        until(&mut h, "the card again", |h| h.query_by_label("Open Effectia Intro").is_some());
        h.get_by_label("Open Effectia Intro").click();
        h.run_steps(3);
        assert!(!h.state_mut().in_gallery());
        assert_eq!(h.state_mut().editor.prop(362, "transform/opacity").map(|p| p.value), Ok(vec![37.0]));
    }

    #[test]
    fn duplicate_and_delete_work_from_the_card_menu() {
        let (mut h, base) = rig("menu");
        h.state_mut().editor.set_scalar(362, "transform/opacity", 37.0).ok();
        h.state_mut().show_gallery();
        until(&mut h, "the card", |h| h.query_by_label("Actions for Effectia Intro").is_some());
        h.get_by_label("Actions for Effectia Intro").click();
        h.run_steps(2);
        h.get_by_label("Duplicate").click();
        h.run_steps(3);
        assert_eq!(projects(&base), ["Effectia Intro copy.ecproj", "Effectia Intro.ecproj"]);
        until(&mut h, "the copy's card", |h| h.query_by_label("Open Effectia Intro copy").is_some());

        h.get_by_label("Actions for Effectia Intro copy").click();
        h.run_steps(2);
        h.get_by_label_contains("Delete").click();
        h.run_steps(2);
        assert!(h.query_by_label("Delete project").is_some(), "a confirmation, not an instant delete");
        h.get_by_label("Keep it").click();
        h.run_steps(2);
        assert_eq!(projects(&base).len(), 2, "keeping it deletes nothing");

        h.get_by_label("Actions for Effectia Intro copy").click();
        h.run_steps(2);
        h.get_by_label_contains("Delete").click();
        h.run_steps(2);
        h.get_by_label("Delete project").click();
        h.run_steps(3);
        assert_eq!(projects(&base), ["Effectia Intro.ecproj"]);
        assert!(h.query_by_label("Open Effectia Intro copy").is_none());
    }

    #[test]
    fn the_back_button_returns_to_the_open_project_and_the_logo_goes_to_the_gallery() {
        let (mut h, _base) = rig("back");
        h.get_by_label("Projects").click();
        h.run_steps(3);
        assert!(h.state_mut().in_gallery());
        h.get_by_label("Back to editor").click();
        h.run_steps(3);
        assert!(!h.state_mut().in_gallery());
        assert!(h.query_by_label("Undo").is_some(), "the editor is back");
    }

    #[test]
    fn deleting_the_open_project_leaves_nothing_to_resurrect_it() {
        let (mut h, base) = rig("deleteopen");
        h.state_mut().editor.set_scalar(362, "transform/opacity", 37.0).ok();
        h.state_mut().show_gallery();
        until(&mut h, "the card", |h| h.query_by_label("Actions for Effectia Intro").is_some());
        h.get_by_label("Actions for Effectia Intro").click();
        h.run_steps(2);
        h.get_by_label_contains("Delete").click();
        h.run_steps(2);
        h.get_by_label("Delete project").click();
        h.run_steps(3);
        assert!(projects(&base).is_empty());
        assert!(h.query_by_label("Back to editor").is_none(), "there is no open project to go back to");
        h.state_mut().show_gallery();
        h.run_steps(3);
        assert!(projects(&base).is_empty(), "leaving again must not write the deleted project back");
    }
}

fn settle_frames(h: &mut Harness<'static, MobileApp>) {
    for _ in 0..400 {
        h.run_steps(2);
        if !h.state().render_busy() {
            h.run_steps(2);
            if !h.state().render_busy() {
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn frames_are_drawn_in_the_background_and_the_picture_stays_touchable() {
    let mut h = harness(412.0, 892.0, false);
    // The very first frame already has a picture area to touch; the frame itself follows.
    settle_frames(&mut h);
    let (side, draft) = h.state().frame_shown().expect("a frame arrived");
    assert!(!draft && side >= 64, "a still frame is drawn in full, got {side} draft={draft}");

    // A change of time is shown at once as a full frame.
    h.state_mut().editor.set_time(0.5).ok();
    settle_frames(&mut h);
    assert_eq!(h.state().frame_shown().map(|f| f.1), Some(false));
}

#[test]
fn scrubbing_draws_small_drafts_and_the_full_frame_follows_when_it_rests() {
    let mut h = harness(412.0, 892.0, false);
    // A generous window, so a loaded machine running other tests cannot make the steps look slow.
    h.state_mut().settle = std::time::Duration::from_millis(1500);
    settle_frames(&mut h);
    let full = h.state().frame_shown().map_or(0, |f| f.0);
    for i in 1..=6 {
        h.state_mut().editor.set_time(0.1 * f64::from(i)).ok();
        h.run_steps(2);
    }
    // Wait for whatever is in flight; the frame on screen is a draft while the time keeps moving.
    for _ in 0..200 {
        if !h.state().render_busy() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(1);
    }
    h.run_steps(2);
    let (side, draft) = h.state().frame_shown().expect("a frame");
    assert!(draft && side <= full, "scrubbing shows a draft ({side} vs {full}, draft={draft})");

    std::thread::sleep(std::time::Duration::from_millis(1700));
    settle_frames(&mut h);
    let (side, draft) = h.state().frame_shown().expect("a frame");
    assert!(!draft && side == full, "at rest the full frame is back ({side} vs {full})");
}

fn drag_by(h: &mut Harness<'static, MobileApp>, from: egui::Pos2, by: egui::Vec2) {
    h.hover_at(from);
    h.run_steps(2);
    h.drag_at(from);
    h.run_steps(2);
    for k in 1..=4 {
        h.hover_at(from + by * (k as f32 / 4.0));
        h.run_steps(2);
    }
    h.drop_at(from + by);
    h.run_steps(3);
}

fn timeline_height(h: &mut Harness<'static, MobileApp>) -> f32 {
    // The timeline's handle is its top strip; the transport row sits right above it.
    let handle = h.get_by_label("Resize timeline").rect();
    h.ctx.content_rect().bottom() - handle.top()
}

#[test]
fn dragging_the_timeline_strip_resizes_the_timeline() {
    let mut h = harness(412.0, 892.0, false);
    h.run_steps(3);
    let before = timeline_height(&mut h);
    // A phone starts with the tallest timeline there is room for, so first make it shorter.
    let start = h.get_by_label("Resize timeline").rect().center();
    drag_by(&mut h, start, egui::vec2(0.0, 140.0));
    let shorter = timeline_height(&mut h);
    assert!(shorter < before - 60.0, "dragging down shrinks it ({before} -> {shorter})");
    let start = h.get_by_label("Resize timeline").rect().center();
    drag_by(&mut h, start, egui::vec2(0.0, -60.0));
    let taller = timeline_height(&mut h);
    assert!(taller > shorter + 20.0, "dragging up grows it ({shorter} -> {taller})");
}

#[test]
fn dragging_the_properties_edge_resizes_the_docked_panel_and_it_is_remembered() {
    let dir = std::env::temp_dir().join(format!("effectia-split-{}", std::process::id()));
    let mut editor = Editor::new(effectcraft_host::session());
    assert!(editor.open_demo().is_ok());
    let mut app = MobileApp::new(editor).with_storage(&dir);
    app.layout = effectia_ui_mobile::layout::LayoutState::tablet();
    let mut h = Harness::builder().with_size(vec2(1376.0, 1032.0)).build_ui_state(|ui, app: &mut MobileApp| app.show(ui), app);
    apply_theme(&h.ctx);
    h.run_steps(4);
    let width = |h: &mut Harness<'static, MobileApp>| 1376.0 - h.get_by_label("Resize properties panel").rect().center().x;
    let before = width(&mut h);
    let start = h.get_by_label("Resize properties panel").rect().center();
    drag_by(&mut h, start, egui::vec2(-100.0, 0.0));
    let wider = width(&mut h);
    assert!(wider > before + 60.0, "dragging left widens it ({before} -> {wider})");
    // The width follows the finger: it grew by what the band moved.
    let grown = h.state().layout.inspector_width - 320.0;
    assert!((grown - (wider - before)).abs() < 2.0 && (grown - 100.0).abs() < 6.0, "grew {grown} while the band moved {}", wider - before);

    // A new run with the same storage folder starts with the dragged width.
    let again = MobileApp::new(Editor::new(effectcraft_host::session())).with_storage(&dir);
    assert!(again.layout.inspector_width > 330.0, "the width was saved, got {}", again.layout.inspector_width);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shrinking_the_window_closes_a_docked_panel_and_growing_it_brings_it_back() {
    let mut h = harness(1376.0, 1032.0, true);
    h.run_steps(3);
    assert!(h.query_by_label("Resize properties panel").is_some(), "docked on a wide window");

    // Split-screen: the window becomes narrow. The panel closes instead of covering the picture.
    h.set_size(vec2(700.0, 1032.0));
    h.run_steps(4);
    assert!(h.query_by_label("Resize properties panel").is_none());
    assert!(!h.state().layout.inspector_open, "no sheet pops up over the picture");

    // The window is a phone-sized column.
    h.set_size(vec2(390.0, 844.0));
    h.run_steps(4);
    assert!(h.get_by_label("Composition viewer").rect().width() > 300.0);

    // Full screen again: the panel returns where it was.
    h.set_size(vec2(1376.0, 1032.0));
    h.run_steps(4);
    assert!(h.query_by_label("Resize properties panel").is_some(), "docked again");
}

#[test]
fn draft_frames_are_sized_to_keep_up_with_the_engine() {
    use effectia_ui_mobile::ui::draft_side;
    // Nothing known yet: half size.
    assert_eq!(draft_side(None, 1000, 1920, 1080), 500);
    // A fast engine (5 ns a pixel) earns the full size; a slow one (400 ns) a quarter.
    assert_eq!(draft_side(Some(5.0), 1000, 1920, 1080), 1000);
    assert_eq!(draft_side(Some(1500.0), 1000, 1920, 1080), 250);
    // In between, the size follows the cost: twice the cost per pixel, fewer pixels.
    let (a, b) = (draft_side(Some(40.0), 1920, 1920, 1080), draft_side(Some(80.0), 1920, 1920, 1080));
    assert!(a > b && b >= 480, "{a} {b}");
    // Nonsense costs fall back to half size instead of dividing by zero.
    for bad in [Some(0.0), Some(-3.0), Some(f64::NAN), Some(f64::INFINITY)] {
        assert_eq!(draft_side(bad, 800, 100, 100), 400, "{bad:?}");
    }
}
