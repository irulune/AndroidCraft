//! The egui rendering of the touch workspace.
//!
//! The look follows the desktop app (same palette, Inter, panel tab strips, blue editable values,
//! label-coloured timeline bars) so a project feels like one product on every device. Panels are
//! placed by [`crate::layout::resolve`]; the widgets hold no document state, every change is an
//! [`Editor`] command, so undo, dirty tracking and the saved file match the desktop app.

use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

mod props;
mod sheets;
mod timeline;

use egui::{
    Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Painter, Pos2, Rect as ERect, Response, Sense, Shape, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::editor::{CompView, Editor, Frame, LayerRow};
use crate::keys::{self, Modifiers};
use crate::layout::{self, LayoutState, Rect, Viewport, WorkspaceKind};
use crate::ops::{EffectInfo, PropNode};
use crate::tokens::{self, DARK, Rgba, label_color};

static INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
static INTER_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
static INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
static JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

pub(crate) const TAB_H: f32 = 32.0;
const ROW_H: f32 = tokens::MIN_TOUCH_TARGET;
const RULER_H: f32 = 32.0;

fn col(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.0, c.1, c.2, c.3)
}

fn erect(r: Rect, origin: Pos2) -> ERect {
    ERect::from_min_size(pos2(origin.x + r.x, origin.y + r.y), vec2(r.w, r.h))
}

/// `h:mm:ss:ff` timecode, the same format as the desktop app. Total: bad input reads as zero.
pub fn timecode(seconds: f64, fps: f64) -> String {
    let s = if seconds.is_finite() { seconds.max(0.0) } else { 0.0 };
    let fps = if fps.is_finite() && fps >= 1.0 { fps } else { 30.0 };
    let whole = s.floor() as u64;
    let frame = (((s - whole as f64) * fps).floor() as u64).min(fps.ceil() as u64 - 1);
    format!("{}:{:02}:{:02}:{:02}", whole / 3600, (whole / 60) % 60, whole % 60, frame)
}

fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

/// Installs Inter (UI) and JetBrains Mono (timecode, numbers), as the desktop app does.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [("inter", INTER_REGULAR), ("inter-medium", INTER_MEDIUM), ("inter-semibold", INTER_SEMIBOLD), ("jbmono", JETBRAINS_MONO)] {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let with_fallback = |first: &str| {
        let mut v = vec![first.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, with_fallback("inter"));
    fonts.families.insert(medium(), with_fallback("inter-medium"));
    fonts.families.insert(semibold(), with_fallback("inter-semibold"));
    let mut mono = vec!["jbmono".to_string()];
    mono.extend(fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Monospace, mono);
    ctx.set_fonts(fonts);
}

/// Applies the dark theme and fonts to an egui context.
pub fn apply_theme(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut v = egui::Visuals::dark();
    v.panel_fill = col(DARK.surface);
    v.window_fill = col(DARK.surface_overlay);
    v.extreme_bg_color = col(DARK.field);
    v.override_text_color = Some(col(DARK.text_primary));
    v.selection.bg_fill = col(DARK.accent);
    v.selection.stroke = Stroke::new(1.0, col(DARK.on_accent));
    v.widgets.noninteractive.bg_stroke = Stroke::new(tokens::BORDER_WIDTH, col(DARK.border));
    v.widgets.inactive.weak_bg_fill = col(DARK.field);
    v.widgets.inactive.bg_fill = col(DARK.field);
    v.widgets.hovered.weak_bg_fill = col(DARK.field);
    v.widgets.active.weak_bg_fill = col(DARK.field);
    ctx.set_visuals(v);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.spacing.interact_size = vec2(tokens::MIN_TOUCH_TARGET, tokens::MIN_TOUCH_TARGET);
        s.spacing.item_spacing = vec2(tokens::SPACE_2, tokens::SPACE_2);
        s.text_styles.insert(egui::TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles.insert(egui::TextStyle::Button, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles.insert(egui::TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));
    });
}

// ----------------------------------------------------------------------------------------- icons

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
    Eye,
    EyeOff,
    Stopwatch,
    Play,
    Pause,
    SkipStart,
    SkipEnd,
    StepBack,
    StepForward,
    Undo,
    Redo,
    Close,
    Timeline,
    Sliders,
    Plus,
    Menu,
    Trash,
    Copy,
    Scissors,
    More,
    Fx,
    ChevronRight,
    ChevronDown,
    ChevronUp,
    Diamond,
    Check,
    ZoomIn,
    ZoomOut,
    TrimIn,
    TrimOut,
    KeyPrev,
    KeyNext,
    Film,
    Export,
    Lock,
    Fit,
    TextT,
    SolidSq,
    RectOutline,
    Ellipse,
    StarShape,
    NullBox,
    Sun,
    Cube,
    Folder,
    Floppy,
}

fn tri(p: &Painter, c: Pos2, r: f32, right: bool, color: Color32) {
    let s = if right { 1.0 } else { -1.0 };
    p.add(Shape::convex_polygon(vec![pos2(c.x - s * r * 0.7, c.y - r), pos2(c.x - s * r * 0.7, c.y + r), pos2(c.x + s * r * 0.9, c.y)], color, Stroke::NONE));
}

fn arc(c: Pos2, r: f32, from: f32, to: f32) -> Vec<Pos2> {
    (0..=20)
        .map(|i| {
            let a = from + (to - from) * i as f32 / 20.0;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// Draws a 20-unit icon centred on `c`, scaled by `size / 20`.
fn paint_icon(p: &Painter, icon: Icon, c: Pos2, size: f32, color: Color32) {
    let u = size / 20.0;
    let st = Stroke::new(1.6 * u.max(0.8), color);
    match icon {
        Icon::Eye | Icon::EyeOff => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let x = -1.0 + 2.0 * i as f32 / 16.0;
                    pos2(c.x + x * 9.0 * u, c.y - (1.0 - x * x) * 5.5 * u)
                })
                .collect();
            let low: Vec<Pos2> = pts.iter().map(|q| pos2(q.x, 2.0 * c.y - q.y)).collect();
            if icon == Icon::Eye {
                p.add(Shape::line(pts, st));
                p.add(Shape::line(low, st));
                p.circle_filled(c, 2.6 * u, color);
            } else {
                let dim = Stroke::new(st.width, color.gamma_multiply(0.45));
                p.add(Shape::line(pts, dim));
                p.add(Shape::line(low, dim));
            }
        }
        Icon::Stopwatch => {
            let ctr = pos2(c.x, c.y + 1.5 * u);
            p.circle_stroke(ctr, 6.5 * u, st);
            p.line_segment([pos2(c.x - 2.0 * u, c.y - 8.5 * u), pos2(c.x + 2.0 * u, c.y - 8.5 * u)], st);
            p.line_segment([ctr, pos2(ctr.x + 3.2 * u, ctr.y - 3.4 * u)], st);
        }
        Icon::Play => tri(p, c, 7.0 * u, true, color),
        Icon::Pause => {
            for dx in [-3.5, 3.5] {
                p.rect_filled(ERect::from_center_size(pos2(c.x + dx * u, c.y), vec2(3.4 * u, 12.0 * u)), 1.0, color);
            }
        }
        Icon::SkipStart => {
            p.rect_filled(ERect::from_center_size(pos2(c.x - 6.5 * u, c.y), vec2(2.4 * u, 12.0 * u)), 1.0, color);
            tri(p, pos2(c.x + 1.5 * u, c.y), 6.0 * u, false, color);
        }
        Icon::SkipEnd => {
            p.rect_filled(ERect::from_center_size(pos2(c.x + 6.5 * u, c.y), vec2(2.4 * u, 12.0 * u)), 1.0, color);
            tri(p, pos2(c.x - 1.5 * u, c.y), 6.0 * u, true, color);
        }
        Icon::StepBack => tri(p, c, 6.5 * u, false, color),
        Icon::StepForward => {
            tri(p, pos2(c.x, c.y), 6.5 * u, true, color);
            p.rect_filled(ERect::from_center_size(pos2(c.x + 7.5 * u, c.y), vec2(2.2 * u, 12.0 * u)), 1.0, color);
        }
        Icon::Undo | Icon::Redo => {
            let flip = if icon == Icon::Redo { -1.0 } else { 1.0 };
            let ctr = pos2(c.x + 0.5 * u * flip, c.y + 1.5 * u);
            let pts: Vec<Pos2> = arc(ctr, 6.5 * u, -PI * 0.15, -PI * 0.15 - PI * 1.15).into_iter().map(|q| pos2(c.x + (q.x - c.x) * flip, q.y)).collect();
            let end = pts.last().copied().unwrap_or(c);
            p.add(Shape::line(pts, st));
            let dir = flip;
            p.add(Shape::convex_polygon(
                vec![pos2(end.x - dir * 4.5 * u, end.y - 0.5 * u), pos2(end.x + dir * 3.0 * u, end.y - 3.2 * u), pos2(end.x + dir * 3.0 * u, end.y + 3.8 * u)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Close => {
            p.line_segment([pos2(c.x - 5.0 * u, c.y - 5.0 * u), pos2(c.x + 5.0 * u, c.y + 5.0 * u)], st);
            p.line_segment([pos2(c.x - 5.0 * u, c.y + 5.0 * u), pos2(c.x + 5.0 * u, c.y - 5.0 * u)], st);
        }
        Icon::Timeline => {
            for (i, (x0, x1)) in [(-8.0, 3.0), (-3.0, 8.0), (-6.0, 6.0)].into_iter().enumerate() {
                let y = c.y + (i as f32 - 1.0) * 5.5 * u;
                p.rect_filled(ERect::from_min_max(pos2(c.x + x0 * u, y - 1.8 * u), pos2(c.x + x1 * u, y + 1.8 * u)), 1.5, color);
            }
        }
        Icon::Sliders => {
            for (i, k) in [-3.0_f32, 3.0, -1.0].into_iter().enumerate() {
                let y = c.y + (i as f32 - 1.0) * 5.5 * u;
                p.line_segment([pos2(c.x - 8.0 * u, y), pos2(c.x + 8.0 * u, y)], st);
                p.circle_filled(pos2(c.x + k * u * 1.6, y), 2.6 * u, color);
            }
        }
        Icon::Plus => {
            p.line_segment([pos2(c.x - 6.5 * u, c.y), pos2(c.x + 6.5 * u, c.y)], st);
            p.line_segment([pos2(c.x, c.y - 6.5 * u), pos2(c.x, c.y + 6.5 * u)], st);
        }
        Icon::Menu => {
            for dy in [-5.5_f32, 0.0, 5.5] {
                p.line_segment([pos2(c.x - 7.5 * u, c.y + dy * u), pos2(c.x + 7.5 * u, c.y + dy * u)], st);
            }
        }
        Icon::Trash => {
            p.line_segment([pos2(c.x - 7.0 * u, c.y - 5.0 * u), pos2(c.x + 7.0 * u, c.y - 5.0 * u)], st);
            p.line_segment([pos2(c.x - 2.5 * u, c.y - 7.5 * u), pos2(c.x + 2.5 * u, c.y - 7.5 * u)], st);
            p.rect_stroke(ERect::from_min_max(pos2(c.x - 5.0 * u, c.y - 4.0 * u), pos2(c.x + 5.0 * u, c.y + 8.0 * u)), 1.5, st, StrokeKind::Middle);
            p.line_segment([pos2(c.x, c.y - 1.0 * u), pos2(c.x, c.y + 5.0 * u)], st);
        }
        Icon::Copy => {
            p.rect_stroke(ERect::from_min_max(pos2(c.x - 3.0 * u, c.y - 3.0 * u), pos2(c.x + 7.5 * u, c.y + 8.0 * u)), 2.0, st, StrokeKind::Middle);
            p.rect_stroke(ERect::from_min_max(pos2(c.x - 7.5 * u, c.y - 8.0 * u), pos2(c.x + 3.0 * u, c.y + 3.0 * u)), 2.0, st, StrokeKind::Middle);
        }
        Icon::Scissors => {
            p.circle_stroke(pos2(c.x - 4.0 * u, c.y + 5.0 * u), 2.6 * u, st);
            p.circle_stroke(pos2(c.x + 4.0 * u, c.y + 5.0 * u), 2.6 * u, st);
            p.line_segment([pos2(c.x - 3.0 * u, c.y + 2.8 * u), pos2(c.x + 5.5 * u, c.y - 8.0 * u)], st);
            p.line_segment([pos2(c.x + 3.0 * u, c.y + 2.8 * u), pos2(c.x - 5.5 * u, c.y - 8.0 * u)], st);
        }
        Icon::More => {
            for dx in [-5.5_f32, 0.0, 5.5] {
                p.circle_filled(pos2(c.x + dx * u, c.y), 1.9 * u, color);
            }
        }
        Icon::Fx => {
            p.text(c, Align2::CENTER_CENTER, "fx", FontId::new(13.0 * u, semibold()), color);
        }
        Icon::ChevronRight => {
            p.add(Shape::line(vec![pos2(c.x - 2.5 * u, c.y - 5.0 * u), pos2(c.x + 2.5 * u, c.y), pos2(c.x - 2.5 * u, c.y + 5.0 * u)], st));
        }
        Icon::ChevronDown => {
            p.add(Shape::line(vec![pos2(c.x - 5.0 * u, c.y - 2.5 * u), pos2(c.x, c.y + 2.5 * u), pos2(c.x + 5.0 * u, c.y - 2.5 * u)], st));
        }
        Icon::ChevronUp => {
            p.add(Shape::line(vec![pos2(c.x - 5.0 * u, c.y + 2.5 * u), pos2(c.x, c.y - 2.5 * u), pos2(c.x + 5.0 * u, c.y + 2.5 * u)], st));
        }
        Icon::Diamond => {
            let r = 6.0 * u;
            p.add(Shape::convex_polygon(vec![pos2(c.x, c.y - r), pos2(c.x + r, c.y), pos2(c.x, c.y + r), pos2(c.x - r, c.y)], color, Stroke::NONE));
        }
        Icon::Check => {
            p.add(Shape::line(
                vec![pos2(c.x - 6.0 * u, c.y), pos2(c.x - 2.0 * u, c.y + 4.5 * u), pos2(c.x + 6.5 * u, c.y - 5.0 * u)],
                Stroke::new(2.0 * u.max(0.8), color),
            ));
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            p.circle_stroke(pos2(c.x - 1.5 * u, c.y - 1.5 * u), 5.5 * u, st);
            p.line_segment([pos2(c.x + 2.5 * u, c.y + 2.5 * u), pos2(c.x + 8.0 * u, c.y + 8.0 * u)], st);
            p.line_segment([pos2(c.x - 4.0 * u, c.y - 1.5 * u), pos2(c.x + 1.0 * u, c.y - 1.5 * u)], st);
            if icon == Icon::ZoomIn {
                p.line_segment([pos2(c.x - 1.5 * u, c.y - 4.0 * u), pos2(c.x - 1.5 * u, c.y + 1.0 * u)], st);
            }
        }
        Icon::TrimIn => {
            p.add(Shape::line(
                vec![
                    pos2(c.x + 1.0 * u, c.y - 7.0 * u),
                    pos2(c.x - 5.0 * u, c.y - 7.0 * u),
                    pos2(c.x - 5.0 * u, c.y + 7.0 * u),
                    pos2(c.x + 1.0 * u, c.y + 7.0 * u),
                ],
                st,
            ));
            tri(p, pos2(c.x + 3.5 * u, c.y), 3.8 * u, true, color);
        }
        Icon::TrimOut => {
            p.add(Shape::line(
                vec![
                    pos2(c.x - 1.0 * u, c.y - 7.0 * u),
                    pos2(c.x + 5.0 * u, c.y - 7.0 * u),
                    pos2(c.x + 5.0 * u, c.y + 7.0 * u),
                    pos2(c.x - 1.0 * u, c.y + 7.0 * u),
                ],
                st,
            ));
            tri(p, pos2(c.x - 3.5 * u, c.y), 3.8 * u, false, color);
        }
        Icon::KeyPrev | Icon::KeyNext => {
            let s = if icon == Icon::KeyNext { 1.0 } else { -1.0 };
            let d = pos2(c.x + s * 1.5 * u, c.y);
            let r = 4.6 * u;
            p.add(Shape::convex_polygon(vec![pos2(d.x, d.y - r), pos2(d.x + r, d.y), pos2(d.x, d.y + r), pos2(d.x - r, d.y)], color, Stroke::NONE));
            tri(p, pos2(c.x - s * 6.5 * u, c.y), 3.0 * u, s > 0.0, color);
        }
        Icon::Film => {
            p.rect_stroke(ERect::from_center_size(c, vec2(15.0 * u, 12.0 * u)), 2.0, st, StrokeKind::Middle);
            for dx in [-3.0_f32, 3.0] {
                p.line_segment([pos2(c.x + dx * u, c.y - 6.0 * u), pos2(c.x + dx * u, c.y + 6.0 * u)], Stroke::new(st.width * 0.7, color));
            }
        }
        Icon::Export => {
            p.add(Shape::line(
                vec![
                    pos2(c.x - 6.0 * u, c.y + 1.0 * u),
                    pos2(c.x - 6.0 * u, c.y + 7.0 * u),
                    pos2(c.x + 6.0 * u, c.y + 7.0 * u),
                    pos2(c.x + 6.0 * u, c.y + 1.0 * u),
                ],
                st,
            ));
            p.line_segment([pos2(c.x, c.y + 3.0 * u), pos2(c.x, c.y - 8.0 * u)], st);
            p.add(Shape::line(vec![pos2(c.x - 3.5 * u, c.y - 4.5 * u), pos2(c.x, c.y - 8.0 * u), pos2(c.x + 3.5 * u, c.y - 4.5 * u)], st));
        }
        Icon::Lock => {
            p.rect_filled(ERect::from_min_max(pos2(c.x - 5.5 * u, c.y - 1.0 * u), pos2(c.x + 5.5 * u, c.y + 7.0 * u)), 1.5, color);
            p.add(Shape::line(arc(pos2(c.x, c.y - 2.0 * u), 3.8 * u, PI, 2.0 * PI), st));
            p.line_segment([pos2(c.x - 3.8 * u, c.y - 2.0 * u), pos2(c.x - 3.8 * u, c.y)], st);
            p.line_segment([pos2(c.x + 3.8 * u, c.y - 2.0 * u), pos2(c.x + 3.8 * u, c.y)], st);
        }
        Icon::TextT => {
            p.line_segment([pos2(c.x - 6.0 * u, c.y - 6.0 * u), pos2(c.x + 6.0 * u, c.y - 6.0 * u)], st);
            p.line_segment([pos2(c.x, c.y - 6.0 * u), pos2(c.x, c.y + 7.0 * u)], st);
            p.line_segment([pos2(c.x - 2.5 * u, c.y + 7.0 * u), pos2(c.x + 2.5 * u, c.y + 7.0 * u)], st);
        }
        Icon::SolidSq => {
            p.rect_filled(ERect::from_center_size(c, vec2(14.0 * u, 14.0 * u)), 2.0, color);
        }
        Icon::RectOutline => {
            p.rect_stroke(ERect::from_center_size(c, vec2(15.0 * u, 11.0 * u)), 2.5, st, StrokeKind::Middle);
        }
        Icon::Ellipse => {
            p.circle_stroke(c, 7.0 * u, st);
        }
        Icon::StarShape => {
            let pts: Vec<Pos2> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 8.0 * u } else { 3.6 * u };
                    let a = -PI / 2.0 + i as f32 * PI / 5.0;
                    pos2(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            p.add(Shape::convex_polygon(pts, Color32::TRANSPARENT, st));
        }
        Icon::NullBox => {
            p.rect_stroke(
                ERect::from_center_size(c, vec2(14.0 * u, 14.0 * u)),
                1.0,
                Stroke::new(st.width * 0.8, color.gamma_multiply(0.6)),
                StrokeKind::Middle,
            );
            p.line_segment([pos2(c.x - 4.0 * u, c.y), pos2(c.x + 4.0 * u, c.y)], st);
            p.line_segment([pos2(c.x, c.y - 4.0 * u), pos2(c.x, c.y + 4.0 * u)], st);
        }
        Icon::Sun => {
            p.circle_stroke(c, 3.6 * u, st);
            for i in 0..8 {
                let a = i as f32 * PI / 4.0;
                p.line_segment([pos2(c.x + 6.0 * u * a.cos(), c.y + 6.0 * u * a.sin()), pos2(c.x + 8.5 * u * a.cos(), c.y + 8.5 * u * a.sin())], st);
            }
        }
        Icon::Cube => {
            let r = 7.5 * u;
            let hex: Vec<Pos2> = (0..6)
                .map(|i| {
                    let a = -PI / 2.0 + i as f32 * PI / 3.0;
                    pos2(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            p.add(Shape::convex_polygon(hex.clone(), Color32::TRANSPARENT, st));
            p.line_segment([c, hex[1]], Stroke::new(st.width * 0.8, color));
            p.line_segment([c, hex[3]], Stroke::new(st.width * 0.8, color));
            p.line_segment([c, hex[5]], Stroke::new(st.width * 0.8, color));
        }
        Icon::Folder => {
            p.add(Shape::line(
                vec![
                    pos2(c.x - 8.0 * u, c.y + 6.0 * u),
                    pos2(c.x - 8.0 * u, c.y - 6.0 * u),
                    pos2(c.x - 2.0 * u, c.y - 6.0 * u),
                    pos2(c.x, c.y - 3.5 * u),
                    pos2(c.x + 8.0 * u, c.y - 3.5 * u),
                    pos2(c.x + 8.0 * u, c.y + 6.0 * u),
                    pos2(c.x - 8.0 * u, c.y + 6.0 * u),
                ],
                st,
            ));
        }
        Icon::Floppy => {
            p.rect_stroke(ERect::from_center_size(c, vec2(15.0 * u, 15.0 * u)), 2.0, st, StrokeKind::Middle);
            p.rect_stroke(
                ERect::from_min_max(pos2(c.x - 4.0 * u, c.y - 7.5 * u), pos2(c.x + 4.0 * u, c.y - 2.5 * u)),
                0.0,
                Stroke::new(st.width * 0.8, color),
                StrokeKind::Middle,
            );
            p.rect_filled(ERect::from_min_max(pos2(c.x - 4.0 * u, c.y + 2.0 * u), pos2(c.x + 4.0 * u, c.y + 7.5 * u)), 1.0, color);
        }
        Icon::Fit => {
            for (sx, sy) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let corner = pos2(c.x + sx * 7.0 * u, c.y + sy * 6.0 * u);
                p.add(Shape::line(vec![pos2(corner.x - sx * 4.0 * u, corner.y), corner, pos2(corner.x, corner.y - sy * 4.0 * u)], st));
            }
        }
    }
}

// -------------------------------------------------------------------------------------- the app

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelTab {
    Properties,
    Effects,
}

/// A selected keyframe: the property it is on and its time in layer seconds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelKey {
    layer: u64,
    path: String,
    time: f64,
}

/// A cached copy of the selected layer's property tree.
struct TreeCache {
    layer: u64,
    stamp: (u64, i64),
    tree: Rc<PropNode>,
}

/// A render running on its own session so the editor stays usable.
struct RenderJob {
    rx: Receiver<Result<String, String>>,
    label: String,
}

/// The whole touch workspace.
pub struct MobileApp {
    pub editor: Editor,
    pub layout: LayoutState,
    pub playing: bool,
    texture: Option<TextureHandle>,
    shown: Option<(u64, i64, u32)>,
    pub status: Option<String>,
    themed: bool,
    /// A short confirmation shown at the bottom, with the time it expires.
    notice: Option<(String, f64)>,
    /// Where projects, exports and media live (`projects/`, `exports/`, `media/`).
    pub storage: PathBuf,
    sheet: Option<sheets::Sheet>,
    panel_tab: PanelTab,
    expanded: HashSet<String>,
    catalog: Option<Rc<Vec<EffectInfo>>>,
    tree: Option<TreeCache>,
    keys: (u64, HashMap<u64, Rc<PropNode>>),
    view: timeline::View,
    drag: Option<timeline::Drag>,
    sel_key: Option<SelKey>,
    viewer_drag: bool,
    job: Option<RenderJob>,
}

/// An icon-only button: 44 pt touch target, hover / pressed fill, accessibility label.
fn icon_button(ui: &mut Ui, id: Id, rect: ERect, icon: Icon, tint: Color32, label: &str, enabled: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let r = ui.interact(rect, id, sense);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
    if enabled && r.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect.shrink(4.0), tokens::RADIUS_MD, col(DARK.row_selected));
    }
    let tint = if enabled { tint } else { col(DARK.text_disabled) };
    paint_icon(ui.painter(), icon, rect.center(), tokens::ICON_MD, tint);
    r
}

impl MobileApp {
    pub fn new(editor: Editor) -> MobileApp {
        MobileApp {
            editor,
            layout: LayoutState { timeline_fraction: 0.5, ..LayoutState::default() },
            playing: false,
            texture: None,
            shown: None,
            status: None,
            themed: false,
            notice: None,
            storage: std::env::temp_dir().join("effectcraft"),
            sheet: None,
            panel_tab: PanelTab::Properties,
            expanded: HashSet::from(["transform".to_string()]),
            catalog: None,
            tree: None,
            keys: (u64::MAX, HashMap::new()),
            view: timeline::View::default(),
            drag: None,
            sel_key: None,
            viewer_drag: false,
            job: None,
        }
    }

    /// Opens a sheet by name (`add`, `layer`, `file`, `comps`, `effects`, `blend`, `parent`, `label`,
    /// `export`, `open`, `media`, `newcomp`, `settings`); false for an unknown name. This is how the
    /// control channel and the screenshot tool reach the sheets without tapping.
    pub fn open_sheet(&mut self, name: &str) -> bool {
        use sheets::{CompForm, Sheet};
        let sheet = match name {
            "add" => Sheet::Add,
            "layer" => Sheet::Layer,
            "file" => Sheet::File,
            "comps" => Sheet::Comps,
            "effects" => Sheet::Effects { query: String::new() },
            "blend" => Sheet::Blend,
            "parent" => Sheet::Parent,
            "label" => Sheet::Label,
            "export" => Sheet::Export,
            "open" => Sheet::OpenProject,
            "media" => Sheet::ImportMedia,
            "newcomp" => Sheet::NewComp(CompForm::default_new()),
            _ => return false,
        };
        self.sheet = Some(sheet);
        true
    }

    /// Shows the Properties or Effects tab of the properties panel.
    pub fn show_panel(&mut self, effects: bool) {
        self.panel_tab = if effects { PanelTab::Effects } else { PanelTab::Properties };
        self.layout.inspector_open = true;
    }

    /// Expands a property group (or an effect) in the properties panel by its path.
    pub fn expand(&mut self, path: &str) {
        self.expanded.insert(path.to_string());
    }

    /// Closes any open sheet.
    pub fn close_sheet(&mut self) {
        self.sheet = None;
    }

    /// Whether a sheet is open.
    pub fn sheet_open(&self) -> bool {
        self.sheet.is_some()
    }

    /// Chooses where projects (`projects/`), exports (`exports/`) and importable media (`media/`) live.
    pub fn with_storage(mut self, dir: impl Into<PathBuf>) -> MobileApp {
        self.storage = dir.into();
        self
    }

    fn report<T>(&mut self, r: crate::editor::Result<T>) -> Option<T> {
        match r {
            Ok(v) => {
                self.status = None;
                Some(v)
            }
            Err(e) => {
                self.status = Some(e.to_string());
                None
            }
        }
    }

    /// Shows a confirmation at the bottom of the screen for a few seconds.
    fn notify(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        let until = ctx.input(|i| i.time) + 4.0;
        self.notice = Some((msg.into(), until));
        self.status = None;
    }

    /// The selected layer, if exactly one is selected.
    fn selected<'a>(&self, comp: &'a CompView) -> Option<&'a LayerRow> {
        comp.layers.iter().find(|l| l.selected)
    }

    /// The selected layer's property tree, cached per edit and time.
    fn tree_of(&mut self, layer: u64, time: f64) -> Option<Rc<PropNode>> {
        let stamp = (self.editor.session().revision, (time * 1000.0).round() as i64);
        if let Some(c) = &self.tree
            && c.layer == layer
            && c.stamp == stamp
        {
            return Some(c.tree.clone());
        }
        match self.editor.layer_tree(layer, 8) {
            Ok(t) => {
                let tree = Rc::new(t);
                self.tree = Some(TreeCache { layer, stamp, tree: tree.clone() });
                Some(tree)
            }
            Err(_) => None,
        }
    }

    /// A layer's property tree for keyframe display: refreshed per edit, not per frame of playback.
    fn keys_of(&mut self, layer: u64) -> Option<Rc<PropNode>> {
        let rev = self.editor.session().revision;
        if self.keys.0 != rev {
            if self.editor.in_gesture() && self.keys.0 != u64::MAX {
                // Mid-drag: keep what is shown until the finger lifts.
            } else {
                self.keys = (rev, HashMap::new());
            }
        }
        if let Some(t) = self.keys.1.get(&layer) {
            return Some(t.clone());
        }
        let t = Rc::new(self.editor.layer_tree(layer, 8).ok()?);
        self.keys.1.insert(layer, t.clone());
        Some(t)
    }

    fn catalog(&mut self) -> Rc<Vec<EffectInfo>> {
        if let Some(c) = &self.catalog {
            return c.clone();
        }
        let c = Rc::new(self.editor.effect_catalog().unwrap_or_default());
        self.catalog = Some(c.clone());
        c
    }

    /// Draws one frame into `ui`, which should fill the window.
    pub fn show(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        if !self.themed {
            // Fonts installed now only exist from the next frame on; drawing text before then
            // would ask for a family that is not there yet.
            apply_theme(ui.ctx());
            self.themed = true;
            ui.painter().rect_filled(full, 0.0, col(DARK.canvas));
            ui.ctx().request_repaint();
            return;
        }
        if self.editor.in_gesture() && !ui.ctx().input(|i| i.pointer.any_down()) {
            self.editor.end_gesture();
        }
        self.poll_render(ui.ctx());
        self.handle_keys(ui.ctx());
        self.advance_playback(ui.ctx());
        let comp = match self.editor.comp() {
            Ok(c) => c,
            Err(e) => {
                self.status = Some(e.to_string());
                None
            }
        };
        // The layers live inside the timeline (as in the desktop app), so only the properties
        // panel is a dock or a sheet.
        self.layout.layers_open = false;
        let ws = layout::resolve(Viewport::new(full.width(), full.height()), &self.layout);
        let o = full.min;
        ui.painter().rect_filled(full, 0.0, col(DARK.canvas));

        self.header(ui, erect(ws.top_bar, o), comp.as_ref());
        if let Some(c) = comp {
            let viewer = erect(ws.viewer, o);
            self.viewer_panel(ui, viewer, &c);
            self.transport(ui, erect(ws.transport, o), &c);
            if let Some(t) = ws.timeline {
                self.timeline_panel(ui, erect(t, o), &c, ws.kind != WorkspaceKind::Phone);
            }
            if let Some(i) = ws.inspector {
                self.properties(ui, erect(i, o), &c, false);
            } else if let Some(i) = ws.inspector_sheet {
                self.properties(ui, erect(i, o), &c, true);
            }
            if let Some(t) = ws.tool_bar {
                self.tool_bar(ui, erect(t, o), t.w > t.h);
            }
            self.sheet_ui(ui, full, Some(&c), ws.kind == WorkspaceKind::Phone);
        } else {
            let r = erect(ws.viewer, o);
            ui.painter().text(
                r.center() - vec2(0.0, 30.0),
                Align2::CENTER_CENTER,
                "No composition",
                FontId::new(tokens::TITLE.size, semibold()),
                col(DARK.text_secondary),
            );
            if self.sheet.is_none() {
                self.sheet = Some(sheets::Sheet::Comps);
            }
            self.sheet_ui(ui, full, None, ws.kind == WorkspaceKind::Phone);
        }
        self.toast(ui, full);
    }

    fn scoped(&mut self, ui: &mut Ui, rect: ERect, f: impl FnOnce(&mut Self, &mut Ui)) {
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            f(self, ui)
        });
    }

    fn toast(&mut self, ui: &mut Ui, full: ERect) {
        let now = ui.ctx().input(|i| i.time);
        let Some((msg, until)) = self.notice.clone() else { return };
        if now >= until {
            self.notice = None;
            return;
        }
        let galley = ui.painter().layout(msg, FontId::new(13.0, FontFamily::Proportional), col(DARK.text_primary), (full.width() - 48.0).max(80.0));
        let size = galley.size() + vec2(28.0, 20.0);
        let rect = ERect::from_center_size(pos2(full.center().x, full.bottom() - 84.0 - size.y / 2.0), size);
        ui.painter().rect_filled(rect.expand(1.0), 10.0, col(DARK.border));
        ui.painter().rect_filled(rect, 10.0, col(DARK.surface_overlay));
        ui.painter().galley(rect.min + vec2(14.0, 10.0), galley, col(DARK.text_primary));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }

    /// A panel tab strip: the active tab's title with an accent underline, and a dim subtitle.
    fn strip(&self, ui: &Ui, rect: ERect, title: &str, sub: &str) {
        let p = ui.painter();
        p.rect_filled(rect, 0.0, col(DARK.header));
        let galley = p.layout_no_wrap(title.to_string(), FontId::new(13.0, medium()), col(DARK.text_primary));
        let x = rect.left() + tokens::SPACE_3;
        let y = rect.center().y;
        p.galley(pos2(x, y - galley.size().y / 2.0), galley.clone(), col(DARK.text_primary));
        p.rect_filled(ERect::from_min_size(pos2(x, rect.bottom() - 2.0), vec2(galley.size().x, 2.0)), 0.0, col(DARK.accent));
        if !sub.is_empty() {
            p.text(
                pos2(x + galley.size().x + tokens::SPACE_2, y),
                Align2::LEFT_CENTER,
                sub,
                FontId::new(13.0, FontFamily::Proportional),
                col(DARK.text_secondary),
            );
        }
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, col(DARK.border)));
    }

    /// A strip with several tappable tabs; returns the index tapped.
    fn tab_strip(&self, ui: &mut Ui, rect: ERect, tabs: &[&str], active: usize) -> Option<usize> {
        ui.painter().rect_filled(rect, 0.0, col(DARK.header));
        ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, col(DARK.border)));
        let mut x = rect.left() + tokens::SPACE_3;
        let mut hit = None;
        for (i, t) in tabs.iter().enumerate() {
            let on = i == active;
            let font = FontId::new(13.0, if on { medium() } else { FontFamily::Proportional });
            let color = if on { col(DARK.text_primary) } else { col(DARK.text_secondary) };
            let galley = ui.painter().layout_no_wrap((*t).to_string(), font, color);
            let w = galley.size().x;
            let cell = ERect::from_min_size(pos2(x - 8.0, rect.top()), vec2(w + 16.0, rect.height()));
            let r = ui.interact(cell, Id::new(("panel-tab", *t)), Sense::click());
            r.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, *t));
            ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, color);
            if on {
                ui.painter().rect_filled(ERect::from_min_size(pos2(x, rect.bottom() - 2.0), vec2(w, 2.0)), 0.0, col(DARK.accent));
            }
            if r.clicked() {
                hit = Some(i);
            }
            x += w + 22.0;
        }
        hit
    }

    fn header(&mut self, ui: &mut Ui, rect: ERect, comp: Option<&CompView>) {
        let (can_undo, can_redo) = (self.editor.can_undo().unwrap_or(false), self.editor.can_redo().unwrap_or(false));
        ui.painter().rect_filled(rect, 0.0, col(DARK.header));
        ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, col(DARK.border)));
        // Logo tile, as in the desktop app's header.
        let logo = ERect::from_center_size(pos2(rect.left() + tokens::SPACE_3 + 14.0, rect.center().y), vec2(28.0, 28.0));
        ui.painter().rect_filled(logo, 7.0, col(DARK.accent));
        let c = logo.center();
        ui.painter().add(Shape::convex_polygon(
            vec![pos2(c.x, c.y - 7.0), pos2(c.x + 6.0, c.y), pos2(c.x, c.y + 7.0), pos2(c.x - 6.0, c.y)],
            Color32::WHITE,
            Stroke::NONE,
        ));

        let b = tokens::MIN_TOUCH_TARGET;
        let redo = ERect::from_min_size(pos2(rect.right() - b - 2.0, rect.center().y - b / 2.0), vec2(b, b));
        let undo = redo.translate(vec2(-b, 0.0));
        let menu = undo.translate(vec2(-b, 0.0));

        // The composition name opens the composition switcher.
        let name_box = ERect::from_min_max(pos2(logo.right() + 4.0, rect.top()), pos2(menu.left() - 4.0, rect.bottom()));
        let r = ui.interact(name_box, Id::new("comp-switcher"), Sense::click());
        let name = comp.map_or("EffectCraft".to_string(), |c| c.name.clone());
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Composition {name}")));
        if r.is_pointer_button_down_on() {
            ui.painter().rect_filled(name_box.shrink2(vec2(0.0, 6.0)), tokens::RADIUS_MD, col(DARK.row_selected));
        }
        let (text, color, font) = match (&self.status, &self.job) {
            (Some(msg), _) => (msg.clone(), col(DARK.danger), FontId::new(12.0, FontFamily::Proportional)),
            (None, Some(j)) => (format!("{}\u{2026}", j.label), col(DARK.warning), FontId::new(13.0, medium())),
            (None, None) => (name, col(DARK.text_primary), FontId::new(15.0, semibold())),
        };
        let chev = comp.is_some() && self.status.is_none() && self.job.is_none();
        let max_w = name_box.width() - 10.0 - if chev { 22.0 } else { 0.0 };
        let galley = ui.painter().layout(text, font, color, max_w.max(40.0));
        let tx = name_box.left() + 10.0;
        ui.painter().galley(pos2(tx, rect.center().y - galley.size().y / 2.0), galley.clone(), color);
        if chev {
            paint_icon(ui.painter(), Icon::ChevronDown, pos2(tx + galley.size().x + 14.0, rect.center().y), 16.0, col(DARK.text_secondary));
        }
        if r.clicked() {
            self.sheet = Some(sheets::Sheet::Comps);
        }

        let icon = col(DARK.icon);
        if icon_button(ui, Id::new("file-menu"), menu, Icon::Menu, icon, "File menu", true).clicked() {
            self.sheet = Some(sheets::Sheet::File);
        }
        if icon_button(ui, Id::new("undo"), undo, Icon::Undo, icon, "Undo", can_undo).clicked() {
            let r = self.editor.undo();
            self.report(r);
        }
        if icon_button(ui, Id::new("redo"), redo, Icon::Redo, icon, "Redo", can_redo).clicked() {
            let r = self.editor.redo();
            self.report(r);
        }
    }

    fn viewer_panel(&mut self, ui: &mut Ui, rect: ERect, comp: &CompView) {
        let (strip, rest) = (ERect::from_min_size(rect.min, vec2(rect.width(), TAB_H)), ERect::from_min_max(pos2(rect.left(), rect.top() + TAB_H), rect.max));
        self.strip(ui, strip, "Composition", &comp.name);
        let foot_h = 30.0;
        let board = ERect::from_min_max(rest.min, pos2(rest.right(), (rest.bottom() - foot_h).max(rest.top())));
        let foot = ERect::from_min_max(pos2(rest.left(), board.bottom()), rest.max);
        ui.painter().rect_filled(board, 0.0, col(DARK.canvas));
        if let Some(img) = self.paint_frame(ui, board.shrink(tokens::SPACE_2), comp) {
            self.viewer_touch(ui, img, comp);
        }
        ui.painter().rect_filled(foot, 0.0, col(DARK.surface));
        ui.painter().text(
            pos2(foot.left() + tokens::SPACE_3, foot.center().y),
            Align2::LEFT_CENTER,
            timecode(comp.time, comp.frame_rate),
            FontId::new(14.0, FontFamily::Monospace),
            col(DARK.value),
        );
        let hint = if self.selected(comp).is_some() { "Drag to move the layer" } else { "Pick a layer to move it" };
        ui.painter().text(foot.center(), Align2::CENTER_CENTER, hint, FontId::new(11.0, FontFamily::Proportional), col(DARK.text_disabled));
        ui.painter().text(
            pos2(foot.right() - tokens::SPACE_3, foot.center().y),
            Align2::RIGHT_CENTER,
            format!("{} \u{00d7} {}", comp.width, comp.height),
            FontId::new(12.0, FontFamily::Proportional),
            col(DARK.text_secondary),
        );
    }

    /// Selection marker and drag-to-move for the selected layer, on the picture at `img`.
    fn viewer_touch(&mut self, ui: &mut Ui, img: ERect, comp: &CompView) {
        let resp = ui.interact(img, Id::new("viewer"), Sense::drag());
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Composition viewer"));
        let Some(layer) = self.selected(comp).map(|l| (l.id, l.locked, l.name.clone())) else { return };
        let (id, locked, _) = layer;
        if let Ok(p) = self.editor.prop(id, "transform/position")
            && let (Some(x), Some(y)) = (p.value.first(), p.value.get(1))
        {
            let at =
                pos2(img.left() + (*x as f32 / comp.width.max(1) as f32) * img.width(), img.top() + (*y as f32 / comp.height.max(1) as f32) * img.height());
            if img.expand(12.0).contains(at) {
                let clip = ui.painter().with_clip_rect(img);
                let ring = Stroke::new(1.5, col(DARK.accent));
                clip.circle_stroke(at, 10.0, ring);
                clip.line_segment([pos2(at.x - 16.0, at.y), pos2(at.x + 16.0, at.y)], ring);
                clip.line_segment([pos2(at.x, at.y - 16.0), pos2(at.x, at.y + 16.0)], ring);
            }
        }
        if locked {
            return;
        }
        if resp.drag_started() {
            self.editor.begin_gesture();
            self.viewer_drag = true;
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            let scale = comp.width.max(1) as f32 / img.width().max(1.0);
            let r = self.editor.nudge(id, f64::from(d.x * scale), f64::from(d.y * scale));
            self.report(r);
        }
        if resp.drag_stopped() {
            self.editor.end_gesture();
            self.viewer_drag = false;
        }
    }

    /// Paints the composition frame and returns the rectangle it occupies.
    fn paint_frame(&mut self, ui: &mut Ui, area: ERect, comp: &CompView) -> Option<ERect> {
        let key = (comp.id, (comp.time * 1000.0).round() as i64);
        let rev = self.editor.session().revision as i64;
        // Dragging and playing render at half size so they keep up; a still frame is full size.
        let draft = self.viewer_drag || self.playing;
        let full_side = (area.width().max(area.height()) * ui.ctx().pixels_per_point()).clamp(64.0, 1920.0) as u32;
        let side = if draft { (full_side / 2).max(64) } else { full_side };
        let stamp = (key.0 ^ (key.1 as u64).rotate_left(17), rev, side);
        if self.shown != Some(stamp) {
            match self.editor.render(side) {
                Ok(f) => {
                    self.upload(ui.ctx(), &f);
                    self.shown = Some(stamp);
                }
                Err(e) => self.status = Some(e.to_string()),
            }
        }
        let tex = self.texture.as_ref()?;
        let aspect = comp.width.max(1) as f32 / comp.height.max(1) as f32;
        let mut size = vec2(area.width(), area.width() / aspect);
        if size.y > area.height() {
            size = vec2(area.height() * aspect, area.height());
        }
        let r = ERect::from_center_size(area.center(), size);
        ui.painter().image(tex.id(), r, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        Some(r)
    }

    fn upload(&mut self, ctx: &egui::Context, f: &Frame) {
        let img = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba);
        match &mut self.texture {
            Some(t) => t.set(img, TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture("viewer", img, TextureOptions::LINEAR)),
        }
    }

    fn transport(&mut self, ui: &mut Ui, rect: ERect, comp: &CompView) {
        ui.painter().rect_filled(rect, 0.0, col(DARK.surface));
        ui.painter().line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, col(DARK.border)));
        let b = tokens::MIN_TOUCH_TARGET;
        let total = 5.0 * b + 4.0 * tokens::SPACE_1;
        let mut x = rect.center().x - total / 2.0;
        let y = rect.center().y - b / 2.0;
        let mut next = || {
            let r = ERect::from_min_size(pos2(x, y), vec2(b, b));
            x += b + tokens::SPACE_1;
            r
        };
        let icon = col(DARK.icon);
        if icon_button(ui, Id::new("t-start"), next(), Icon::SkipStart, icon, "Go to start", true).clicked() {
            self.playing = false;
            let r = self.editor.set_time(0.0);
            self.report(r);
        }
        if icon_button(ui, Id::new("t-back"), next(), Icon::StepBack, icon, "Previous frame", true).clicked() {
            self.playing = false;
            let r = self.editor.step_frames(-1);
            self.report(r);
        }
        let play = next();
        let r = ui.interact(play, Id::new("t-play"), Sense::click());
        let label = if self.playing { "Pause" } else { "Play" };
        r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
        ui.painter().circle_filled(play.center(), 19.0, col(if r.is_pointer_button_down_on() { DARK.focus_ring } else { DARK.accent }));
        paint_icon(ui.painter(), if self.playing { Icon::Pause } else { Icon::Play }, play.center(), tokens::ICON_MD, col(DARK.on_accent));
        if r.clicked() {
            self.playing = !self.playing;
        }
        if icon_button(ui, Id::new("t-fwd"), next(), Icon::StepForward, icon, "Next frame", true).clicked() {
            self.playing = false;
            let r = self.editor.step_frames(1);
            self.report(r);
        }
        if icon_button(ui, Id::new("t-end"), next(), Icon::SkipEnd, icon, "Go to end", true).clicked() {
            self.playing = false;
            let r = self.editor.set_time(comp.duration);
            self.report(r);
        }
    }

    fn advance_playback(&mut self, ctx: &egui::Context) {
        if !self.playing {
            return;
        }
        let Ok(Some(c)) = self.editor.comp() else {
            self.playing = false;
            return;
        };
        let dt = f64::from(ctx.input(|i| i.unstable_dt)).min(0.1);
        let mut t = c.time + dt;
        if t >= c.duration {
            t = 0.0;
        }
        let r = self.editor.set_time(t);
        self.report(r);
        ctx.request_repaint();
    }

    /// The bottom tab bar (phone) or left rail (phone landscape): timeline, properties, effects.
    fn tool_bar(&mut self, ui: &mut Ui, rect: ERect, horizontal: bool) {
        ui.painter().rect_filled(rect, 0.0, col(DARK.header));
        let edge = if horizontal { [rect.left_top(), rect.right_top()] } else { [rect.right_top(), rect.right_bottom()] };
        ui.painter().line_segment(edge, Stroke::new(1.0, col(DARK.border)));
        let props_on = self.layout.inspector_open && self.panel_tab == PanelTab::Properties;
        let fx_on = self.layout.inspector_open && self.panel_tab == PanelTab::Effects;
        let items = [(Icon::Timeline, "Timeline", self.layout.timeline_open), (Icon::Sliders, "Properties", props_on), (Icon::Fx, "Effects", fx_on)];
        let n = items.len() as f32;
        for (i, (icon, label, on)) in items.into_iter().enumerate() {
            let cell = if horizontal {
                ERect::from_min_size(pos2(rect.left() + rect.width() / n * i as f32, rect.top()), vec2(rect.width() / n, rect.height()))
            } else {
                ERect::from_min_size(pos2(rect.left(), rect.top() + 4.0 + 60.0 * i as f32), vec2(rect.width(), 56.0))
            };
            let r = ui.interact(cell, Id::new(("tool", label)), Sense::click());
            r.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, label));
            let tint = if on { col(DARK.accent) } else { col(DARK.text_secondary) };
            paint_icon(ui.painter(), icon, pos2(cell.center().x, cell.center().y - 8.0), tokens::ICON_MD, tint);
            ui.painter().text(pos2(cell.center().x, cell.center().y + 12.0), Align2::CENTER_CENTER, label, FontId::new(10.5, medium()), tint);
            if r.clicked() {
                match label {
                    "Timeline" => self.layout.timeline_open = !self.layout.timeline_open,
                    "Properties" => {
                        let same = self.panel_tab == PanelTab::Properties;
                        self.panel_tab = PanelTab::Properties;
                        self.layout.inspector_open = !(self.layout.inspector_open && same);
                    }
                    _ => {
                        let same = self.panel_tab == PanelTab::Effects;
                        self.panel_tab = PanelTab::Effects;
                        self.layout.inspector_open = !(self.layout.inspector_open && same);
                    }
                }
            }
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.sheet.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let events: Vec<(Modifiers, String)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, repeat: false, .. } => Some((
                        Modifiers {
                            ctrl: modifiers.ctrl,
                            cmd: modifiers.mac_cmd || (modifiers.command && !modifiers.ctrl),
                            alt: modifiers.alt,
                            shift: modifiers.shift,
                        },
                        key.name().to_string(),
                    )),
                    _ => None,
                })
                .collect()
        });
        for (mods, key) in events {
            let Some(chord) = keys::chord_text(mods, &key) else { continue };
            let hit = keys::resolve(self.editor.shortcuts(), &chord, &["Composition", "Timeline"]).map(|b| (b.command.clone(), b.params.clone()));
            if let Some((cmd, params)) = hit {
                let r = self.editor.session_mut().execute(&cmd, params).map(drop).map_err(|e| crate::editor::EditorError(format!("{cmd}: {e}")));
                self.report(r);
            }
        }
    }
}
