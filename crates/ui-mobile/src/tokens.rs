//! Design tokens: colour, type, spacing, radii, control sizes and motion.
//!
//! Every visual constant of the mobile interface lives here so screens never hard-code values.
//! The tests enforce the accessibility rules the palette is designed around (WCAG 2.2 AA text and
//! non-text contrast, 44 pt touch targets).

/// An sRGB colour with alpha, 8 bits per channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba(r, g, b, 255)
    }

    /// WCAG relative luminance of the colour's RGB (alpha ignored).
    pub fn relative_luminance(self) -> f64 {
        fn lin(c: u8) -> f64 {
            let c = f64::from(c) / 255.0;
            if c <= 0.039_28 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        0.2126 * lin(self.0) + 0.7152 * lin(self.1) + 0.0722 * lin(self.2)
    }

    /// WCAG contrast ratio between two colours, in `1.0..=21.0`.
    pub fn contrast_ratio(self, other: Rgba) -> f64 {
        let (a, b) = (self.relative_luminance(), other.relative_luminance());
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }
}

/// The semantic colours of one theme. Values follow the desktop app's dark theme
/// (`effectcraft-ui-egui` `Tokens`) so both frontends look like one product.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// The pasteboard behind the composition viewer and the app background.
    pub canvas: Rgba,
    /// Panel tab strips and the app header.
    pub header: Rgba,
    /// Panel bodies.
    pub surface: Rgba,
    /// Alternate rows, fields.
    pub surface_raised: Rgba,
    /// Sheets, menus and popovers.
    pub surface_overlay: Rgba,
    pub field: Rgba,
    pub row_alt: Rgba,
    pub row_selected: Rgba,
    pub timeline_bg: Rgba,
    pub ruler_tick: Rgba,
    pub border: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_disabled: Rgba,
    pub icon: Rgba,
    /// Selection, primary actions, active tab underline.
    pub accent: Rgba,
    /// Text and icons drawn on top of `accent`.
    pub on_accent: Rgba,
    /// Editable numeric values and the timecode.
    pub value: Rgba,
    pub playhead: Rgba,
    pub keyframe: Rgba,
    pub keyframe_selected: Rgba,
    pub focus_ring: Rgba,
    pub danger: Rgba,
    pub warning: Rgba,
    pub success: Rgba,
}

/// The dark editing workspace.
pub const DARK: Palette = Palette {
    canvas: Rgba::rgb(0x12, 0x12, 0x12),
    header: Rgba::rgb(0x1f, 0x1f, 0x1f),
    surface: Rgba::rgb(0x23, 0x23, 0x23),
    surface_raised: Rgba::rgb(0x2a, 0x2a, 0x2a),
    surface_overlay: Rgba::rgb(0x30, 0x30, 0x30),
    field: Rgba::rgb(0x17, 0x17, 0x17),
    row_alt: Rgba::rgb(0x27, 0x27, 0x27),
    row_selected: Rgba::rgb(0x3a, 0x3a, 0x3a),
    timeline_bg: Rgba::rgb(0x1b, 0x1b, 0x1b),
    ruler_tick: Rgba::rgb(0x70, 0x70, 0x70),
    border: Rgba::rgb(0x34, 0x34, 0x34),
    text_primary: Rgba::rgb(0xe3, 0xe3, 0xe3),
    text_secondary: Rgba::rgb(0x9a, 0x9a, 0x9a),
    text_disabled: Rgba::rgb(0x6b, 0x6b, 0x6b),
    icon: Rgba::rgb(0xb4, 0xb4, 0xb4),
    accent: Rgba::rgb(0x2d, 0x8c, 0xeb),
    on_accent: Rgba::rgb(0x06, 0x0c, 0x14),
    value: Rgba::rgb(0x3d, 0x8f, 0xf5),
    playhead: Rgba::rgb(0x2d, 0x8c, 0xeb),
    keyframe: Rgba::rgb(0xa8, 0xa8, 0xa8),
    keyframe_selected: Rgba::rgb(0x3d, 0x8f, 0xf5),
    focus_ring: Rgba::rgb(0x4a, 0xa0, 0xf5),
    danger: Rgba::rgb(0xff, 0x6b, 0x5e),
    warning: Rgba::rgb(0xf0, 0xa8, 0x3c),
    success: Rgba::rgb(0x4f, 0xd1, 0x8b),
};

/// One step of the type scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// Size in points (density-independent pixels).
    pub size: f32,
    /// CSS-style weight (400 regular, 500 medium, 600 semibold).
    pub weight: u16,
    /// Line height as a multiple of `size`.
    pub line_height: f32,
    /// Letter spacing in em.
    pub tracking: f32,
    /// Tabular figures: timecodes and numeric fields must not jitter while scrubbing.
    pub tabular: bool,
}

/// The type scale, smallest to largest. Editing chrome stays compact; there are no display sizes.
pub const CAPTION: TextStyle = TextStyle { size: 11.0, weight: 500, line_height: 1.3, tracking: 0.01, tabular: false };
pub const FOOTNOTE: TextStyle = TextStyle { size: 12.0, weight: 400, line_height: 1.35, tracking: 0.0, tabular: false };
pub const BODY: TextStyle = TextStyle { size: 15.0, weight: 400, line_height: 1.35, tracking: 0.0, tabular: false };
pub const LABEL: TextStyle = TextStyle { size: 15.0, weight: 500, line_height: 1.3, tracking: 0.0, tabular: false };
pub const TITLE: TextStyle = TextStyle { size: 17.0, weight: 600, line_height: 1.25, tracking: -0.01, tabular: false };
pub const HEADING: TextStyle = TextStyle { size: 22.0, weight: 600, line_height: 1.2, tracking: -0.015, tabular: false };
/// Timecode and numeric property fields.
pub const NUMERIC: TextStyle = TextStyle { size: 13.0, weight: 500, line_height: 1.3, tracking: 0.0, tabular: true };

/// Spacing on a 4 pt grid.
pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_6: f32 = 24.0;

/// Corner radii: small for chips and fields, medium for controls, large for sheets only.
pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 8.0;
pub const RADIUS_LG: f32 = 14.0;

/// Smallest interactive target on touch (Apple HIG 44 pt; WCAG 2.2 AA minimum is 24 px).
pub const MIN_TOUCH_TARGET: f32 = 44.0;
/// Denser target used only when a precise pointer or stylus is the active input.
pub const MIN_POINTER_TARGET: f32 = 32.0;
/// Icon glyph sizes (one icon family, drawn on a 24 pt grid).
pub const ICON_SM: f32 = 18.0;
pub const ICON_MD: f32 = 22.0;
/// Hairline border width.
pub const BORDER_WIDTH: f32 = 1.0;
/// Width of the playhead line and the grab area around it.
pub const PLAYHEAD_WIDTH: f32 = 2.0;
pub const PLAYHEAD_HIT: f32 = MIN_TOUCH_TARGET;
/// Keyframe diamond size and the touch hit area around it.
pub const KEYFRAME_SIZE: f32 = 12.0;
pub const KEYFRAME_HIT: f32 = MIN_TOUCH_TARGET;

/// Animation durations in milliseconds.
pub const MOTION_FAST_MS: u32 = 120;
pub const MOTION_STANDARD_MS: u32 = 200;
pub const MOTION_SHEET_MS: u32 = 320;

/// The colour of a layer label by its name (`Red`, `Blue`, ...), as in the desktop app.
pub fn label_color(name: &str) -> Rgba {
    let [r, g, b] = effectcraft_color::Label::from_name(name).unwrap_or(effectcraft_color::Label::None).rgb();
    Rgba::rgb(r, g, b)
}

/// Duration to use for a transition, honouring the system reduce-motion setting.
pub fn motion_ms(base: u32, reduce_motion: bool) -> u32 {
    if reduce_motion { 0 } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACES: [Rgba; 4] = [DARK.canvas, DARK.surface, DARK.surface_raised, DARK.surface_overlay];

    #[test]
    fn contrast_ratio_matches_wcag_reference_values() {
        let black = Rgba::rgb(0, 0, 0);
        let white = Rgba::rgb(255, 255, 255);
        assert!((black.contrast_ratio(white) - 21.0).abs() < 1e-9);
        assert!((white.contrast_ratio(white) - 1.0).abs() < 1e-9);
        assert_eq!(black.contrast_ratio(DARK.accent), DARK.accent.contrast_ratio(black));
    }

    #[test]
    fn text_meets_aa_on_every_surface() {
        for s in SURFACES {
            assert!(DARK.text_primary.contrast_ratio(s) >= 7.0, "primary text on {s:?}");
            assert!(DARK.text_secondary.contrast_ratio(s) >= 4.5, "secondary text on {s:?}");
        }
    }

    #[test]
    fn accent_text_and_status_colours_are_legible() {
        assert!(DARK.on_accent.contrast_ratio(DARK.accent) >= 4.5);
        for s in [DARK.surface, DARK.surface_raised, DARK.surface_overlay] {
            for c in [DARK.danger, DARK.warning, DARK.success] {
                assert!(c.contrast_ratio(s) >= 4.5, "{c:?} on {s:?}");
            }
        }
    }

    #[test]
    fn non_text_marks_meet_3_to_1() {
        for s in SURFACES {
            assert!(DARK.accent.contrast_ratio(s) >= 3.0, "accent on {s:?}");
            assert!(DARK.focus_ring.contrast_ratio(s) >= 3.0, "focus ring on {s:?}");
        }
        for s in [DARK.canvas, DARK.surface, DARK.surface_raised] {
            assert!(DARK.playhead.contrast_ratio(s) >= 3.0, "playhead on {s:?}");
            assert!(DARK.keyframe.contrast_ratio(s) >= 3.0, "keyframe on {s:?}");
            assert!(DARK.keyframe_selected.contrast_ratio(s) >= 3.0, "selected keyframe on {s:?}");
        }
    }

    #[test]
    fn surfaces_step_up_in_brightness() {
        let l: Vec<f64> = SURFACES.iter().map(|c| c.relative_luminance()).collect();
        assert!(l.windows(2).all(|w| w[0] < w[1]), "{l:?}");
    }

    #[test]
    fn type_scale_is_ordered_and_readable() {
        let scale = [CAPTION, FOOTNOTE, BODY, TITLE, HEADING];
        assert!(scale.windows(2).all(|w| w[0].size < w[1].size));
        assert!(scale.iter().all(|s| s.size >= 11.0 && s.line_height >= 1.2));
        const { assert!(NUMERIC.tabular, "numeric fields need tabular figures") };
    }

    #[test]
    fn touch_targets_follow_the_hig() {
        const { assert!(MIN_TOUCH_TARGET >= 44.0) };
        const { assert!(PLAYHEAD_HIT >= MIN_TOUCH_TARGET && KEYFRAME_HIT >= MIN_TOUCH_TARGET) };
        const { assert!(MIN_POINTER_TARGET >= 24.0, "WCAG 2.2 target size minimum") };
    }

    #[test]
    fn spacing_is_on_the_grid_and_ascending() {
        let s = [SPACE_1, SPACE_2, SPACE_3, SPACE_4, SPACE_6];
        assert!(s.iter().all(|v| v % 4.0 == 0.0));
        assert!(s.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn reduce_motion_removes_durations() {
        assert_eq!(motion_ms(MOTION_SHEET_MS, true), 0);
        assert_eq!(motion_ms(MOTION_SHEET_MS, false), MOTION_SHEET_MS);
    }
}
