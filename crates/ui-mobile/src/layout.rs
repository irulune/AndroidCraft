//! Responsive workspace layout for phones and tablets.
//!
//! The layout is chosen from the available window size, not from a device name, so split-screen,
//! Stage Manager, foldables and desktop-class tablets all fall out of the same rules. [`resolve`]
//! is a pure function of the viewport, the safe-area insets and the user's panel state, which
//! keeps it deterministic and exhaustively testable.

use crate::tokens::MIN_TOUCH_TARGET;

const EPS: f32 = 0.01;

/// An axis-aligned rectangle in points (density-independent pixels), origin top-left.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Whether `other` lies fully inside this rectangle.
    pub fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x - EPS && other.y >= self.y - EPS && other.right() <= self.right() + EPS && other.bottom() <= self.bottom() + EPS
    }

    /// Whether the two rectangles share interior area (touching edges do not overlap).
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right() - EPS && other.x < self.right() - EPS && self.y < other.bottom() - EPS && other.y < self.bottom() - EPS
    }

    fn split_top(self, h: f32) -> (Rect, Rect) {
        let h = h.clamp(0.0, self.h);
        (Rect::new(self.x, self.y, self.w, h), Rect::new(self.x, self.y + h, self.w, self.h - h))
    }

    fn split_bottom(self, h: f32) -> (Rect, Rect) {
        let h = h.clamp(0.0, self.h);
        (Rect::new(self.x, self.bottom() - h, self.w, h), Rect::new(self.x, self.y, self.w, self.h - h))
    }

    fn split_left(self, w: f32) -> (Rect, Rect) {
        let w = w.clamp(0.0, self.w);
        (Rect::new(self.x, self.y, w, self.h), Rect::new(self.x + w, self.y, self.w - w, self.h))
    }

    fn split_right(self, w: f32) -> (Rect, Rect) {
        let w = w.clamp(0.0, self.w);
        (Rect::new(self.right() - w, self.y, w, self.h), Rect::new(self.x, self.y, self.w - w, self.h))
    }
}

/// Safe-area insets (notch, home indicator, rounded corners, system bars).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Insets {
    pub top: f32,
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
}

/// The window the workspace is laid out in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub safe: Insets,
}

impl Viewport {
    pub fn new(width: f32, height: f32) -> Viewport {
        Viewport { width, height, safe: Insets::default() }
    }
}

/// Which workspace arrangement the viewport calls for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceKind {
    /// Narrow and tall: viewer on top, collapsible timeline, bottom toolbar, sheets.
    Phone,
    /// Short and wide: tool rail on the left, viewer and timeline centred, side sheets.
    PhoneLandscape,
    /// Roomy: docked layers panel, multi-track timeline, docked inspector when there is width.
    Tablet,
}

/// Width at which the tablet arrangement starts.
pub const TABLET_MIN_WIDTH: f32 = 600.0;
/// Height at which the tablet arrangement starts.
pub const TABLET_MIN_HEIGHT: f32 = 500.0;
/// Width from which the layers panel is docked instead of a sheet.
pub const DOCK_LAYERS_MIN_WIDTH: f32 = 720.0;
/// Width from which the inspector is docked instead of a sheet.
pub const DOCK_INSPECTOR_MIN_WIDTH: f32 = 960.0;

const TRANSPORT_H: f32 = MIN_TOUCH_TARGET;
const PHONE_TOP_H: f32 = 48.0;
const LANDSCAPE_TOP_H: f32 = 44.0;
const TABLET_TOP_H: f32 = 52.0;
const BOTTOM_BAR_H: f32 = 56.0;
const RAIL_W: f32 = 56.0;
const LAYERS_W: f32 = 280.0;
const INSPECTOR_W: f32 = 320.0;

/// Classifies a viewport. Tablet layouts need both a roomy width and height.
pub fn classify(width: f32, height: f32) -> WorkspaceKind {
    if width >= TABLET_MIN_WIDTH && height >= TABLET_MIN_HEIGHT {
        WorkspaceKind::Tablet
    } else if width > height {
        WorkspaceKind::PhoneLandscape
    } else {
        WorkspaceKind::Phone
    }
}

/// What the user has opened or resized. Panels that cannot be docked at the current size appear
/// as sheets instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutState {
    pub timeline_open: bool,
    /// Share of the editing area given to the timeline, `0.0..=1.0` (clamped per layout).
    pub timeline_fraction: f32,
    pub layers_open: bool,
    pub inspector_open: bool,
}

impl Default for LayoutState {
    fn default() -> Self {
        LayoutState { timeline_open: true, timeline_fraction: 0.38, layers_open: false, inspector_open: false }
    }
}

impl LayoutState {
    /// Tablet default: both side panels open.
    pub fn tablet() -> LayoutState {
        LayoutState { layers_open: true, inspector_open: true, ..LayoutState::default() }
    }
}

/// The resolved rectangles of one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pub kind: WorkspaceKind,
    /// The usable area after safe-area insets.
    pub area: Rect,
    /// Project title, save status, undo / redo, export.
    pub top_bar: Rect,
    /// Bottom toolbar (phone) or left tool rail (landscape phone); none on tablets, where the
    /// tools live in the top bar.
    pub tool_bar: Option<Rect>,
    pub viewer: Rect,
    /// Play / pause, timecode, loop. Always directly under the viewer.
    pub transport: Rect,
    pub timeline: Option<Rect>,
    pub layers: Option<Rect>,
    pub inspector: Option<Rect>,
    /// Layers shown as a sheet that floats over the workspace (only when not docked).
    pub layers_sheet: Option<Rect>,
    /// Inspector shown as a sheet that floats over the workspace (only when not docked).
    pub inspector_sheet: Option<Rect>,
}

impl Workspace {
    /// Every docked rectangle; none of them overlap and all lie inside [`Workspace::area`].
    pub fn docked(&self) -> Vec<Rect> {
        let mut v = vec![self.top_bar, self.viewer, self.transport];
        v.extend(self.tool_bar);
        v.extend(self.timeline);
        v.extend(self.layers);
        v.extend(self.inspector);
        v
    }
}

fn clean(v: f32) -> f32 {
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

/// Stacks viewer, transport row and timeline inside `body`.
fn stack(body: Rect, open: bool, fraction: f32, timeline_min: f32, max_share: f32) -> (Rect, Rect, Option<Rect>) {
    let transport_h = TRANSPORT_H.min(body.h);
    let avail = (body.h - transport_h).max(0.0);
    let timeline_h = if open {
        let hi_raw = avail * max_share;
        let lo = timeline_min.min(hi_raw);
        let hi = hi_raw.max(lo);
        (avail * fraction).clamp(lo, hi)
    } else {
        0.0
    };
    let (viewer, rest) = body.split_top(avail - timeline_h);
    let (transport, rest) = rest.split_top(transport_h);
    let timeline = if timeline_h > 0.0 { Some(rest) } else { None };
    (viewer, transport, timeline)
}

/// A sheet anchored to the bottom (phone) or to a side (larger layouts) of `region`.
fn sheet(kind: WorkspaceKind, region: Rect, left_side: bool) -> Rect {
    match kind {
        WorkspaceKind::Phone => region.split_bottom((region.h * 0.6).min(420.0)).0,
        _ => {
            let w = (region.w * 0.5).min(if left_side { LAYERS_W } else { INSPECTOR_W + 20.0 });
            if left_side { region.split_left(w).0 } else { region.split_right(w).0 }
        }
    }
}

/// Resolves the workspace for a viewport and panel state. Total: garbage sizes (NaN, negative)
/// produce an empty layout instead of panicking.
pub fn resolve(vp: Viewport, state: &LayoutState) -> Workspace {
    let (width, height) = (clean(vp.width), clean(vp.height));
    let s = vp.safe;
    let (st, sl, sb, sr) = (clean(s.top), clean(s.left), clean(s.bottom), clean(s.right));
    let area = Rect::new(sl.min(width), st.min(height), (width - sl - sr).max(0.0), (height - st - sb).max(0.0));
    let kind = classify(width, height);
    let fraction = if state.timeline_fraction.is_finite() { state.timeline_fraction.clamp(0.0, 1.0) } else { 0.38 };

    let (top_bar, tool_bar, body, timeline_min, max_share, layers, inspector, side_region) = match kind {
        WorkspaceKind::Phone => {
            let (top, rest) = area.split_top(PHONE_TOP_H);
            let (bottom, body) = rest.split_bottom(BOTTOM_BAR_H);
            (top, Some(bottom), body, 96.0, 0.45, None, None, rest)
        }
        WorkspaceKind::PhoneLandscape => {
            let (top, rest) = area.split_top(LANDSCAPE_TOP_H);
            let (rail, body) = rest.split_left(RAIL_W);
            (top, Some(rail), body, 100.0, 0.47, None, None, rest)
        }
        WorkspaceKind::Tablet => {
            let (top, rest) = area.split_top(TABLET_TOP_H);
            let mut body = rest;
            let mut layers = None;
            let mut inspector = None;
            if state.layers_open && width >= DOCK_LAYERS_MIN_WIDTH {
                let (l, b) = body.split_left(LAYERS_W);
                layers = Some(l);
                body = b;
            }
            if state.inspector_open && width >= DOCK_INSPECTOR_MIN_WIDTH {
                let (i, b) = body.split_right(INSPECTOR_W);
                inspector = Some(i);
                body = b;
            }
            (top, None, body, 160.0, 0.5, layers, inspector, rest)
        }
    };

    // Landscape phones are short: the timeline needs room for its chips, ruler and a few rows.
    let fraction = if kind == WorkspaceKind::PhoneLandscape { fraction.max(max_share) } else { fraction };
    let (viewer, transport, timeline) = stack(body, state.timeline_open, fraction, timeline_min, max_share);

    // Panels that did not fit as docks become sheets. Only one sheet shows at a time on a phone
    // (the inspector wins, since it is what the user is editing).
    let want_inspector_sheet = state.inspector_open && inspector.is_none();
    let want_layers_sheet = state.layers_open && layers.is_none();
    let single_sheet = kind != WorkspaceKind::Tablet;
    let inspector_sheet = if want_inspector_sheet { Some(sheet(kind, side_region, false)) } else { None };
    let layers_sheet = if want_layers_sheet && !(single_sheet && want_inspector_sheet) { Some(sheet(kind, side_region, true)) } else { None };

    Workspace { kind, area, top_bar, tool_bar, viewer, transport, timeline, layers, inspector, layers_sheet, inspector_sheet }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(w: f32, h: f32, state: LayoutState) -> Workspace {
        resolve(Viewport::new(w, h), &state)
    }

    #[test]
    fn real_device_sizes_get_the_expected_arrangement() {
        use WorkspaceKind::*;
        for (w, h, want) in [
            (320.0, 568.0, Phone),
            (375.0, 667.0, Phone),
            (360.0, 800.0, Phone),
            (430.0, 932.0, Phone),
            (932.0, 430.0, PhoneLandscape),
            (844.0, 390.0, PhoneLandscape),
            (744.0, 1133.0, Tablet),
            (1133.0, 744.0, Tablet),
            (1032.0, 1376.0, Tablet),
            (1376.0, 1032.0, Tablet),
            (673.0, 841.0, Tablet),
        ] {
            assert_eq!(classify(w, h), want, "{w}x{h}");
        }
    }

    /// Checks the structural invariants of a resolved workspace.
    fn check(w: f32, h: f32, state: LayoutState, viewer_min: (f32, f32)) {
        let ws = ws(w, h, state);
        let docked = ws.docked();
        for (i, a) in docked.iter().enumerate() {
            assert!(ws.area.contains(a), "{w}x{h}: {a:?} outside {:?}", ws.area);
            for b in &docked[i + 1..] {
                assert!(!a.overlaps(b), "{w}x{h}: {a:?} overlaps {b:?}");
            }
        }
        assert!(ws.viewer.w >= viewer_min.0 && ws.viewer.h >= viewer_min.1, "{w}x{h}: viewer {:?}", ws.viewer);
        assert!(ws.transport.h >= MIN_TOUCH_TARGET - EPS, "{w}x{h}: transport {:?}", ws.transport);
        assert!(ws.top_bar.h >= MIN_TOUCH_TARGET - EPS, "{w}x{h}: top bar {:?}", ws.top_bar);
        if let Some(t) = ws.tool_bar {
            assert!(t.h >= MIN_TOUCH_TARGET - EPS && t.w >= MIN_TOUCH_TARGET - EPS, "{w}x{h}: tools {t:?}");
        }
        for s in [ws.layers_sheet, ws.inspector_sheet].into_iter().flatten() {
            assert!(ws.area.contains(&s), "{w}x{h}: sheet {s:?}");
        }
    }

    fn states() -> Vec<LayoutState> {
        let mut v = Vec::new();
        for timeline_open in [false, true] {
            for fraction in [0.0, 0.38, 1.0] {
                for layers_open in [false, true] {
                    for inspector_open in [false, true] {
                        v.push(LayoutState { timeline_open, timeline_fraction: fraction, layers_open, inspector_open });
                    }
                }
            }
        }
        v
    }

    #[test]
    fn phone_portrait_invariants_hold_across_sizes() {
        for state in states() {
            let mut w: f32 = 320.0;
            while w < 600.0 {
                let mut h = w.max(568.0);
                while h <= 1000.0 {
                    check(w, h, state, (240.0, 160.0));
                    h += 53.0;
                }
                w += 17.0;
            }
        }
    }

    #[test]
    fn phone_landscape_invariants_hold_across_sizes() {
        for state in states() {
            let mut h: f32 = 320.0;
            while h < 500.0 {
                let mut w = h.max(568.0) + 1.0;
                while w <= 1200.0 {
                    check(w, h, state, (240.0, 120.0));
                    w += 41.0;
                }
                h += 23.0;
            }
        }
    }

    #[test]
    fn tablet_invariants_hold_across_sizes() {
        for state in states() {
            let mut w: f32 = 600.0;
            while w <= 1400.0 {
                let mut h: f32 = 500.0;
                while h <= 1100.0 {
                    check(w, h, state, (240.0, 200.0));
                    h += 47.0;
                }
                w += 37.0;
            }
        }
    }

    #[test]
    fn safe_area_insets_are_respected() {
        let vp = Viewport { width: 393.0, height: 852.0, safe: Insets { top: 59.0, left: 0.0, bottom: 34.0, right: 0.0 } };
        let ws = resolve(vp, &LayoutState::default());
        assert_eq!(ws.area, Rect::new(0.0, 59.0, 393.0, 759.0));
        assert!(ws.area.contains(&ws.top_bar) && ws.top_bar.y >= 59.0);
        let bar = ws.tool_bar.expect("phone has a bottom toolbar");
        assert!(bar.bottom() <= 852.0 - 34.0 + EPS);
    }

    #[test]
    fn closing_the_timeline_gives_the_viewer_the_space() {
        let open = ws(390.0, 844.0, LayoutState::default());
        let closed = ws(390.0, 844.0, LayoutState { timeline_open: false, ..LayoutState::default() });
        assert!(open.timeline.is_some() && closed.timeline.is_none());
        assert!(closed.viewer.h > open.viewer.h);
        assert!((closed.transport.y - closed.viewer.bottom()).abs() < EPS);
        assert!((open.transport.y - open.viewer.bottom()).abs() < EPS);
    }

    #[test]
    fn timeline_fraction_is_clamped_to_keep_the_viewer_usable() {
        let big = ws(390.0, 844.0, LayoutState { timeline_fraction: 1.0, ..LayoutState::default() });
        let small = ws(390.0, 844.0, LayoutState { timeline_fraction: 0.0, ..LayoutState::default() });
        let (big, small) = (big.timeline.map(|r| r.h), small.timeline.map(|r| r.h));
        assert!(big.is_some_and(|h| h <= 844.0 * 0.45), "{big:?}");
        assert!(small.is_some_and(|h| h >= 96.0 - EPS), "{small:?}");
    }

    #[test]
    fn tablets_dock_panels_only_when_there_is_room() {
        let wide = ws(1376.0, 1032.0, LayoutState::tablet());
        assert!(wide.layers.is_some() && wide.inspector.is_some());
        assert!(wide.layers_sheet.is_none() && wide.inspector_sheet.is_none());

        let mid = ws(834.0, 1194.0, LayoutState::tablet());
        assert!(mid.layers.is_some() && mid.inspector.is_none());
        assert!(mid.inspector_sheet.is_some());

        let narrow = ws(640.0, 900.0, LayoutState::tablet());
        assert!(narrow.layers.is_none() && narrow.inspector.is_none());
        assert!(narrow.layers_sheet.is_some() && narrow.inspector_sheet.is_some());
    }

    #[test]
    fn a_phone_shows_one_sheet_at_a_time() {
        let both = ws(390.0, 844.0, LayoutState { layers_open: true, inspector_open: true, ..LayoutState::default() });
        assert!(both.inspector_sheet.is_some());
        assert!(both.layers_sheet.is_none());
        let layers_only = ws(390.0, 844.0, LayoutState { layers_open: true, ..LayoutState::default() });
        assert!(layers_only.layers_sheet.is_some());
    }

    #[test]
    fn resolve_is_deterministic() {
        let a = ws(1180.0, 820.0, LayoutState::tablet());
        let b = ws(1180.0, 820.0, LayoutState::tablet());
        assert_eq!(a, b);
    }

    #[test]
    fn garbage_input_never_panics() {
        let bad = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -50.0, 0.0, 1.0];
        for &w in &bad {
            for &h in &bad {
                for &f in &bad {
                    let vp = Viewport { width: w, height: h, safe: Insets { top: f, left: f, bottom: f, right: f } };
                    let state = LayoutState { timeline_fraction: f, ..LayoutState::tablet() };
                    let ws = resolve(vp, &state);
                    assert!(ws.docked().iter().all(|r| r.w.is_finite() && r.h.is_finite() && r.w >= 0.0 && r.h >= 0.0));
                }
            }
        }
    }
}
