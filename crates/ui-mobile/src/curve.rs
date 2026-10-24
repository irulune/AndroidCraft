//! The cubic-Bezier view of one keyframe segment, for the touch ease editor.
//!
//! A segment runs from key A to key B over `dur` seconds and changes the value by `dv`. Drawn in
//! a unit square (time left to right, progress bottom to top) the ease is a Bezier curve whose two
//! inner points are the handles: A's outgoing ease and B's incoming ease. The engine stores each
//! side as an *influence* (how far along the segment the handle reaches, in percent) and a
//! *speed* (the slope there, in value units per second). These functions convert between the two
//! forms, so a dragged handle becomes the `keys.setEase` the engine understands.

/// A point in the unit square (`y` may leave `0..=1`: that is overshoot).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

/// The least influence the engine accepts, in percent.
pub const MIN_INFLUENCE: f64 = 0.1;
/// How far a handle may be dragged above and below the segment.
pub const Y_RANGE: (f64, f64) = (-1.0, 2.0);

fn usable(dur: f64, dv: f64) -> bool {
    dur.is_finite() && dur > 1e-9 && dv.is_finite() && dv.abs() > 1e-9
}

/// Where A's outgoing handle sits, or `None` when the segment has no length or no change.
pub fn out_handle(influence: f64, speed: f64, dur: f64, dv: f64) -> Option<Pt> {
    if !usable(dur, dv) {
        return None;
    }
    let x = (influence / 100.0).clamp(MIN_INFLUENCE / 100.0, 1.0);
    Some(Pt { x, y: (speed * x * dur / dv).clamp(Y_RANGE.0, Y_RANGE.1) })
}

/// Where B's incoming handle sits.
pub fn in_handle(influence: f64, speed: f64, dur: f64, dv: f64) -> Option<Pt> {
    if !usable(dur, dv) {
        return None;
    }
    let x = (influence / 100.0).clamp(MIN_INFLUENCE / 100.0, 1.0);
    Some(Pt { x: 1.0 - x, y: (1.0 - speed * x * dur / dv).clamp(Y_RANGE.0, Y_RANGE.1) })
}

/// `(influence in percent, speed)` for A's outgoing handle dragged to `p`.
pub fn out_ease(p: Pt, dur: f64, dv: f64) -> Option<(f64, f64)> {
    if !usable(dur, dv) || !p.x.is_finite() || !p.y.is_finite() {
        return None;
    }
    let x = p.x.clamp(MIN_INFLUENCE / 100.0, 1.0);
    let y = p.y.clamp(Y_RANGE.0, Y_RANGE.1);
    Some((x * 100.0, y * dv / (x * dur)))
}

/// `(influence in percent, speed)` for B's incoming handle dragged to `p`.
pub fn in_ease(p: Pt, dur: f64, dv: f64) -> Option<(f64, f64)> {
    if !usable(dur, dv) || !p.x.is_finite() || !p.y.is_finite() {
        return None;
    }
    let x = (1.0 - p.x).clamp(MIN_INFLUENCE / 100.0, 1.0);
    let y = 1.0 - p.y.clamp(Y_RANGE.0, Y_RANGE.1);
    Some((x * 100.0, y * dv / (x * dur)))
}

/// The curve through (0,0) and (1,1) with handles `p1` and `p2`, at parameter `t` in `0..=1`.
pub fn sample(p1: Pt, p2: Pt, t: f64) -> Pt {
    let u = 1.0 - t;
    let (b, c, d) = (3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Pt { x: b * p1.x + c * p2.x + d, y: b * p1.y + c * p2.y + d }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_handle_survives_the_round_trip_through_influence_and_speed() {
        let (dur, dv) = (2.0, 300.0);
        for p in [Pt { x: 0.33, y: 0.0 }, Pt { x: 0.7, y: 1.4 }, Pt { x: 0.1, y: -0.5 }] {
            let (inf, speed) = out_ease(p, dur, dv).unwrap();
            let back = out_handle(inf, speed, dur, dv).unwrap();
            assert!(close(back.x, p.x) && close(back.y, p.y), "{p:?} -> {back:?}");
        }
        for p in [Pt { x: 0.66, y: 1.0 }, Pt { x: 0.2, y: -0.4 }, Pt { x: 0.9, y: 1.7 }] {
            let (inf, speed) = in_ease(p, dur, -dv).unwrap();
            let back = in_handle(inf, speed, dur, -dv).unwrap();
            assert!(close(back.x, p.x) && close(back.y, p.y), "{p:?} -> {back:?}");
        }
    }

    #[test]
    fn easy_ease_is_flat_at_both_ends_and_linear_runs_along_the_diagonal() {
        // Easy Ease: 33.3 % influence, zero speed.
        let h = out_handle(100.0 / 3.0, 0.0, 1.0, 50.0).unwrap();
        assert!(close(h.y, 0.0) && (h.x - 1.0 / 3.0).abs() < 1e-9);
        let h = in_handle(100.0 / 3.0, 0.0, 1.0, 50.0).unwrap();
        assert!(close(h.y, 1.0) && (h.x - 2.0 / 3.0).abs() < 1e-9);
        // Linear: speed = dv / dur, so the handle lies on the diagonal.
        let h = out_handle(25.0, 50.0, 1.0, 50.0).unwrap();
        assert!(close(h.x, h.y));
    }

    #[test]
    fn the_speed_follows_the_direction_of_the_change() {
        let (_, up) = out_ease(Pt { x: 0.5, y: 0.5 }, 1.0, 100.0).unwrap();
        let (_, down) = out_ease(Pt { x: 0.5, y: 0.5 }, 1.0, -100.0).unwrap();
        assert!(up > 0.0 && down < 0.0 && close(up, -down));
    }

    #[test]
    fn a_segment_with_no_change_or_no_length_cannot_be_edited() {
        assert!(out_handle(33.0, 1.0, 1.0, 0.0).is_none());
        assert!(in_handle(33.0, 1.0, 0.0, 10.0).is_none());
        assert!(out_ease(Pt { x: 0.3, y: 0.3 }, 1.0, 0.0).is_none());
        assert!(in_ease(Pt { x: f64::NAN, y: 0.3 }, 1.0, 5.0).is_none());
    }

    #[test]
    fn dragging_far_outside_stays_in_range() {
        let (inf, speed) = out_ease(Pt { x: 9.0, y: 99.0 }, 1.0, 10.0).unwrap();
        assert!(close(inf, 100.0) && close(speed, Y_RANGE.1 * 10.0));
        let (inf, _) = in_ease(Pt { x: 5.0, y: 0.0 }, 1.0, 10.0).unwrap();
        assert!(close(inf, MIN_INFLUENCE));
    }

    #[test]
    fn the_curve_starts_at_a_and_ends_at_b() {
        let (p1, p2) = (Pt { x: 0.4, y: 0.0 }, Pt { x: 0.6, y: 1.0 });
        assert_eq!(sample(p1, p2, 0.0), Pt { x: 0.0, y: 0.0 });
        let end = sample(p1, p2, 1.0);
        assert!(close(end.x, 1.0) && close(end.y, 1.0));
        let mid = sample(p1, p2, 0.5);
        assert!(close(mid.y, 0.5), "a symmetric ease is half done at half time: {mid:?}");
    }
}
