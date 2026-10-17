//! AndroidCraft: the Android entry point for the EffectCraft touch workspace.

use effectcraft_ui_mobile::editor::Editor;
use effectcraft_ui_mobile::ui::{apply_theme, MobileApp};

struct App {
    inner: MobileApp,
    sized: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, storage: Option<std::path::PathBuf>) -> App {
        apply_theme(&cc.egui_ctx);
        let mut editor = Editor::new(effectcraft_host::session());
        if let Err(e) = editor.open_demo() {
            log::error!("could not open the demo project: {e}");
        }
        let mut inner = MobileApp::new(editor);
        if let Some(dir) = storage {
            for sub in ["projects", "exports", "media"] {
                if let Err(e) = std::fs::create_dir_all(dir.join(sub)) {
                    log::warn!("could not create {sub}: {e}");
                }
            }
            inner = inner.with_storage(dir);
        }
        App { inner, sized: false }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.sized {
            // Docked panels only fit on wide screens; phones start with the properties closed.
            self.sized = true;
            if ui.available_width() >= effectcraft_ui_mobile::layout::DOCK_INSPECTOR_MIN_WIDTH {
                self.inner.layout = effectcraft_ui_mobile::layout::LayoutState::tablet();
            }
        }
        self.inner.show(ui);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("androidcraft"));
    let storage = app.external_data_path().map(|p| p.to_path_buf());
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    if let Err(e) = eframe::run_native("AndroidCraft", options, Box::new(move |cc| Ok(Box::new(App::new(cc, storage))))) {
        log::error!("eframe failed: {e}");
    }
}
