//! Effectia: the Android entry point for the touch workspace.

mod access;

use effectia_ui_mobile::editor::Editor;
use effectia_ui_mobile::ui::{MobileApp, apply_theme};

struct App {
    inner: MobileApp,
    sized: bool,
    #[cfg(target_os = "android")]
    host: Option<winit::platform::android::activity::AndroidApp>,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        storage: Option<std::path::PathBuf>,
        #[cfg(target_os = "android")] host: Option<winit::platform::android::activity::AndroidApp>,
    ) -> App {
        apply_theme(&cc.egui_ctx);
        let mut inner = MobileApp::new(Editor::new(effectcraft_host::session()));
        if let Some(dir) = storage {
            for sub in ["projects", "exports", "media"] {
                if let Err(e) = std::fs::create_dir_all(dir.join(sub)) {
                    log::warn!("could not create {sub}: {e}");
                }
            }
            inner = inner.with_storage(dir);
        }
        #[cfg(target_os = "android")]
        if let Some(app) = &host {
            // Photos, videos and files: permission checks and the system pages, through JNI.
            inner = inner.with_platform(access::AndroidAccess::new(app.vm_as_ptr(), app.activity_as_ptr()));
        }
        // Start on the project gallery: saved projects, the demo, and New project.
        inner.show_gallery();
        App {
            inner,
            sized: false,
            #[cfg(target_os = "android")]
            host,
        }
    }
}

#[cfg(target_os = "android")]
impl App {
    /// The part of the window the system draws over (status bar, camera cutout, gesture bar),
    /// taken from the activity's content rectangle, in points. Without it the header slides
    /// under the status bar on phones that draw edge to edge.
    fn update_safe_area(&mut self, ctx: &egui::Context) {
        let Some(host) = &self.host else { return };
        let Some(window) = host.native_window() else { return };
        let ppp = ctx.pixels_per_point().max(0.5);
        let (w, h) = (window.width() as f32, window.height() as f32);
        let r = host.content_rect();
        let (left, top) = (r.left.max(0) as f32, r.top.max(0) as f32);
        let (right, bottom) = ((w - r.right as f32).max(0.0), (h - r.bottom as f32).max(0.0));
        // A content rectangle that has not been reported yet is empty; ignore it.
        if r.right <= r.left || r.bottom <= r.top {
            return;
        }
        let cap = |v: f32| (v / ppp).clamp(0.0, 96.0);
        self.inner.safe = effectia_ui_mobile::layout::Insets { top: cap(top), left: cap(left), bottom: cap(bottom), right: cap(right) };
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "android")]
        self.update_safe_area(ui.ctx());
        if !self.sized {
            // Docked panels only fit on wide screens; phones start with the properties closed.
            self.sized = true;
            if ui.available_width() >= effectia_ui_mobile::layout::DOCK_INSPECTOR_MIN_WIDTH {
                // Open the panels but keep the sizes the person dragged them to last time.
                self.inner.layout.layers_open = true;
                self.inner.layout.inspector_open = true;
            }
        }
        self.inner.show(ui);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("effectia"));
    let storage = app.external_data_path().map(|p| p.to_path_buf());
    let host = Some(app.clone());
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    if let Err(e) = eframe::run_native("Effectia", options, Box::new(move |cc| Ok(Box::new(App::new(cc, storage, host))))) {
        log::error!("eframe failed: {e}");
    }
}
