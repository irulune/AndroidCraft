//! Drag handles between the panels: the timeline's top strip moves its height, and the docked
//! inspector's inner edge moves its width. The sizes are remembered between runs.

use super::*;
use crate::layout::{PanelSizes, Workspace};

const FILE: &str = "layout.json";

/// A grip: three dots, drawn on a handle so it reads as something to hold.
fn grip(p: &egui::Painter, centre: Pos2, vertical: bool, ink: Color32) {
    for k in [-1.0_f32, 0.0, 1.0] {
        let d = if vertical { vec2(0.0, k * 6.0) } else { vec2(k * 6.0, 0.0) };
        p.circle_filled(centre + d, 1.8, ink);
    }
}

impl MobileApp {
    /// Reads the panel sizes saved by an earlier run from the storage folder.
    pub(super) fn load_panel_sizes(&mut self) {
        let Ok(text) = std::fs::read_to_string(self.storage.join(FILE)) else { return };
        if let Ok(sizes) = serde_json::from_str::<PanelSizes>(&text) {
            self.layout.set_sizes(sizes);
        }
    }

    fn save_panel_sizes(&self) {
        let Ok(text) = serde_json::to_string(&self.layout.sizes()) else { return };
        // Best effort: not being able to remember a layout is no reason to bother anyone.
        if std::fs::create_dir_all(&self.storage).is_ok() {
            let _ = std::fs::write(self.storage.join(FILE), text);
        }
    }

    /// Draws and handles every resize handle of the workspace. Called after the panels so the
    /// handles sit on top of them.
    pub(super) fn splitters(&mut self, ui: &mut Ui, ws: &Workspace, origin: Pos2) {
        let mut dragging = false;
        let mut released = false;
        let ink = col(DARK.text_secondary);
        let live = col(DARK.accent);

        if ws.timeline_resizable
            && let Some(t) = ws.timeline
        {
            let strip = erect(Rect { h: TAB_H, ..t }, origin);
            let r = ui.interact(strip, Id::new("split-timeline"), Sense::drag());
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, "Resize timeline"));
            let on = r.dragged() || r.hovered();
            if on {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
            let tint = if r.dragged() { live } else { ink };
            grip(ui.painter(), pos2(strip.right() - 28.0, strip.center().y), false, tint);
            if r.dragged()
                && let Some(p) = ui.ctx().pointer_interact_pos()
            {
                // The strip's middle follows the finger.
                let top = p.y - origin.y - TAB_H / 2.0;
                let bottom = t.y + t.h;
                self.layout.resize_timeline(ws, bottom - top);
            }
            dragging |= r.dragged();
            released |= r.drag_stopped();
        }

        if let (Some(i), Some(band)) = (ws.inspector, ws.inspector_splitter) {
            let band = erect(band, origin);
            let r = ui.interact(band, Id::new("split-inspector"), Sense::drag());
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, "Resize properties panel"));
            if r.dragged() || r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            let edge = origin.x + i.x;
            let tint = if r.dragged() { live } else { ink };
            ui.painter().line_segment([pos2(edge, band.top()), pos2(edge, band.bottom())], Stroke::new(if r.dragged() { 2.0 } else { 1.0 }, tint));
            grip(ui.painter(), pos2(edge, band.center().y), true, tint);
            if r.dragged()
                && let Some(p) = ui.ctx().pointer_interact_pos()
            {
                self.layout.resize_inspector(origin.x + i.x + i.w - p.x);
            }
            dragging |= r.dragged();
            released |= r.drag_stopped();
        }

        self.splitting = dragging;
        if released {
            self.save_panel_sizes();
        }
    }
}
