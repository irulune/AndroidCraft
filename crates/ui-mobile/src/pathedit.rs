//! Geometry for editing a mask or shape path on the picture: where its points are on screen, which
//! point, handle or edge a finger is on, and the way back from a screen point to the path's own
//! space. Pure maths, so it is tested without a window.

use crate::ops::PathData;

/// A 2D point.
pub type P = [f64; 2];

/// What a touch is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// A vertex.
    Vertex(usize),
    /// A handle of a vertex (`out` is the one leading to the next vertex).
    Handle { vertex: usize, out: bool },
    /// An edge: the segment starting at this vertex, at this fraction along it.
    Edge { segment: usize, t: f64 },
}

fn cubic(a: P, b: P, c: P, d: P, t: f64) -> P {
    let u = 1.0 - t;
    let k = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    [k[0] * a[0] + k[1] * b[0] + k[2] * c[0] + k[3] * d[0], k[0] * a[1] + k[1] * b[1] + k[2] * c[1] + k[3] * d[1]]
}

/// The four Bezier points of the segment that starts at vertex `i` (none past the end of an open path).
pub fn segment(path: &PathData, i: usize) -> Option<[P; 4]> {
    let n = path.vertices.len();
    let j = if i + 1 < n {
        i + 1
    } else if path.closed && i + 1 == n && n > 1 {
        0
    } else {
        return None;
    };
    let (a, d) = (*path.vertices.get(i)?, *path.vertices.get(j)?);
    let o = path.outs.get(i).copied().unwrap_or([0.0; 2]);
    let n_in = path.ins.get(j).copied().unwrap_or([0.0; 2]);
    Some([a, [a[0] + o[0], a[1] + o[1]], [d[0] + n_in[0], d[1] + n_in[1]], d])
}

/// The path as a polyline through `map` (path space to screen), for drawing; a closed path repeats its first point.
pub fn polyline(path: &PathData, map: &impl Fn(P) -> P) -> Vec<P> {
    let n = path.vertices.len();
    let mut out = Vec::new();
    for i in 0..n {
        let Some([a, b, c, d]) = segment(path, i) else { break };
        if out.is_empty() {
            out.push(map(a));
        }
        for k in 1..=16 {
            out.push(map(cubic(a, b, c, d, k as f64 / 16.0)));
        }
    }
    if out.is_empty()
        && let Some(v) = path.vertices.first()
    {
        out.push(map(*v));
    }
    out
}

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// What `at` (screen) touches. Handles of the `selected` vertex win over vertices, vertices over edges;
/// `reach` is how close a finger has to be, in screen units. `map` takes path space to screen.
pub fn hit(path: &PathData, selected: Option<usize>, at: P, reach: f64, map: &impl Fn(P) -> P) -> Option<Hit> {
    if let Some(i) = selected
        && let Some(v) = path.vertices.get(i)
    {
        let mut best: Option<(f64, Hit)> = None;
        for (out, tg) in [(false, path.ins.get(i)), (true, path.outs.get(i))] {
            let Some(tg) = tg.filter(|t| **t != [0.0; 2]) else { continue };
            let d = dist(map([v[0] + tg[0], v[1] + tg[1]]), at);
            if d <= reach && best.is_none_or(|b| d < b.0) {
                best = Some((d, Hit::Handle { vertex: i, out }));
            }
        }
        if let Some((_, h)) = best {
            return Some(h);
        }
    }
    let nearest = path.vertices.iter().enumerate().map(|(i, v)| (dist(map(*v), at), i)).filter(|(d, _)| *d <= reach).min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, i)) = nearest {
        return Some(Hit::Vertex(i));
    }
    let mut best: Option<(f64, Hit)> = None;
    for i in 0..path.vertices.len() {
        let Some([a, b, c, d]) = segment(path, i) else { continue };
        for k in 0..=24 {
            let t = k as f64 / 24.0;
            let dd = dist(map(cubic(a, b, c, d, t)), at);
            if dd <= reach && best.is_none_or(|b| dd < b.0) {
                best = Some((dd, Hit::Edge { segment: i, t }));
            }
        }
    }
    best.map(|b| b.1)
}

/// Does a handle drag keep the opposite handle mirrored? True when the two were straight across
/// from each other (a smooth point), so dragging one swings the other.
pub fn handles_mirrored(path: &PathData, i: usize) -> bool {
    let (Some(a), Some(b)) = (path.ins.get(i), path.outs.get(i)) else { return false };
    (a[0] + b[0]).abs() < 1e-6 && (a[1] + b[1]).abs() < 1e-6 && (*a != [0.0; 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> PathData {
        PathData { closed: true, vertices: vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]], ins: vec![[0.0; 2]; 4], outs: vec![[0.0; 2]; 4] }
    }

    /// Path space to screen: twice the size, shifted.
    fn map(p: P) -> P {
        [p[0] * 2.0 + 10.0, p[1] * 2.0 + 10.0]
    }

    #[test]
    fn the_nearest_vertex_is_hit_within_reach_only() {
        let p = square();
        assert_eq!(hit(&p, None, [212.0, 14.0], 20.0, &map), Some(Hit::Vertex(1)));
        assert_eq!(hit(&p, None, [10.0, 10.0], 20.0, &map), Some(Hit::Vertex(0)));
        assert_eq!(hit(&p, None, [110.0, 110.0], 20.0, &map), None, "the middle of the square is not on the path");
    }

    #[test]
    fn an_edge_reports_its_segment_and_how_far_along() {
        let p = square();
        match hit(&p, None, [110.0, 10.0], 12.0, &map) {
            Some(Hit::Edge { segment, t }) => {
                assert_eq!(segment, 0);
                assert!((t - 0.5).abs() < 0.05, "{t}");
            }
            other => panic!("{other:?}"),
        }
        // The closing edge runs from the last vertex back to the first.
        match hit(&p, None, [10.0, 110.0], 12.0, &map) {
            Some(Hit::Edge { segment, .. }) => assert_eq!(segment, 3),
            other => panic!("{other:?}"),
        }
        let mut open = square();
        open.closed = false;
        assert_eq!(hit(&open, None, [10.0, 110.0], 12.0, &map), None, "an open path has no closing edge");
    }

    #[test]
    fn a_selected_vertexs_handles_are_hit_before_anything_else() {
        let mut p = square();
        p.outs[1] = [30.0, 0.0];
        p.ins[1] = [-30.0, 0.0];
        let at = map([130.0, 0.0]);
        assert_eq!(hit(&p, Some(1), at, 20.0, &map), Some(Hit::Handle { vertex: 1, out: true }));
        assert_ne!(hit(&p, None, at, 20.0, &map), Some(Hit::Handle { vertex: 1, out: true }), "handles only exist for the selected vertex");
    }

    #[test]
    fn a_curved_segment_follows_its_handles() {
        let mut p = square();
        p.outs[0] = [0.0, 80.0];
        p.ins[1] = [0.0, 80.0];
        let line = polyline(&p, &|q| q);
        assert!(line.iter().any(|q| q[1] > 30.0 && q[0] > 5.0 && q[0] < 95.0), "the first edge bulges away from the straight line");
        assert_eq!(line.first(), Some(&[0.0, 0.0]));
        assert_eq!(line.last(), Some(&[0.0, 0.0]), "a closed path returns to its start");
    }

    #[test]
    fn mirrored_handles_are_recognised() {
        let mut p = square();
        assert!(!handles_mirrored(&p, 1), "a corner has none");
        p.outs[1] = [10.0, 5.0];
        p.ins[1] = [-10.0, -5.0];
        assert!(handles_mirrored(&p, 1));
        p.ins[1] = [-3.0, 1.0];
        assert!(!handles_mirrored(&p, 1));
    }

    #[test]
    fn degenerate_paths_do_not_panic() {
        let empty = PathData::default();
        assert!(polyline(&empty, &|q| q).is_empty());
        assert_eq!(hit(&empty, Some(3), [0.0, 0.0], 10.0, &|q| q), None);
        let one = PathData { closed: true, vertices: vec![[5.0, 5.0]], ins: vec![], outs: vec![] };
        assert_eq!(polyline(&one, &|q| q), vec![[5.0, 5.0]]);
        assert_eq!(hit(&one, None, [5.0, 5.0], 4.0, &|q| q), Some(Hit::Vertex(0)));
    }
}
