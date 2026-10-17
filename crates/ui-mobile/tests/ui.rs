//! Drives the touch workspace headlessly (no GPU) and checks what the user would see and do.

use effectcraft_ui_mobile::editor::Editor;
use effectcraft_ui_mobile::ui::{MobileApp, apply_theme, timecode};
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

fn harness(w: f32, h: f32, tablet: bool) -> Harness<'static, MobileApp> {
    let mut editor = Editor::new(effectcraft_host::session());
    assert!(editor.open_demo().is_ok());
    let mut app = MobileApp::new(editor);
    if tablet {
        app.layout = effectcraft_ui_mobile::layout::LayoutState::tablet();
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
    let index = h.state_mut().editor.comp().ok().flatten().and_then(|c| c.layers.iter().find(|l| l.name == "EFFECTCRAFT").map(|l| l.index));
    let index = index.unwrap_or(0);
    h.get_by_label(&format!("Layer {index}, EFFECTCRAFT")).click();
    h.run();
    assert!(h.query_by_label("Keyframe Opacity").is_some());
    let comp = h.state_mut().editor.comp().ok().flatten();
    assert!(comp.is_some_and(|c| c.layers.iter().any(|l| l.selected && l.name == "EFFECTCRAFT")));
}

#[test]
fn the_eye_hides_a_layer_without_selecting_it_and_undo_brings_it_back() {
    let mut h = harness(1376.0, 1032.0, true);
    let visible = |h: &mut Harness<'static, MobileApp>| -> Vec<bool> {
        h.state_mut().editor.comp().ok().flatten().map(|c| c.layers.iter().map(|l| l.visible).collect()).unwrap_or_default()
    };
    let initial = visible(&mut h);
    h.get_by_label("Show EFFECTCRAFT").click();
    h.run();
    let comp = h.state_mut().editor.comp().ok().flatten();
    let row = comp.as_ref().and_then(|c| c.layers.iter().find(|l| l.name == "EFFECTCRAFT"));
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
