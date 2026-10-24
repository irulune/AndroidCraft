//! Where things are in the picture: which layer lies under a point, and how a layer's own pixels
//! map onto the composition. Both ask the engine's evaluator, so they follow transforms, parents
//! and expressions exactly as the renderer does.

use effectcraft_engine::Session;
use effectcraft_engine::geom::{Mat3, vec2};
use effectcraft_engine::render::{EvalCtx, content_bounds};
use effectcraft_project::Layer;

/// A homogeneous depth below this means the point is at or behind a 3D camera, where the
/// projection flips: such a layer is not hit-tested.
const NEAR_W: f64 = 0.5;

fn depth(m: &Mat3, p: [f64; 2]) -> f64 {
    let row = m.0.get(2).copied().unwrap_or([0.0, 0.0, 1.0]);
    row[0] * p[0] + row[1] * p[1] + row[2]
}

fn inside(p: [f64; 2], q: &[[f64; 2]; 4]) -> bool {
    let mut sign = 0.0;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let c = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        if c.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// The outline of a layer's content in composition pixels (clockwise from its top left), or
/// `None` when it has no visible content, or reaches to or behind the camera.
fn quad(ctx: &EvalCtx, layer: &Layer) -> Option<[[f64; 2]; 4]> {
    let b = content_bounds(ctx, layer)?;
    let (m, _) = ctx.layer_to_comp(layer);
    let corners = [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]];
    if corners.iter().any(|c| depth(&m, *c) < NEAR_W) {
        return None;
    }
    Some(corners.map(|c| {
        let q = m.apply(vec2(c[0], c[1]));
        [q.x, q.y]
    }))
}

/// The layers under the composition point `at`, front to back (ids).
pub fn layers_at(session: &Session, at: [f64; 2]) -> Vec<u64> {
    let (Some(cid), Some(comp)) = (session.active_comp_id(), session.active_comp()) else { return vec![] };
    let time = session.time_of(cid);
    let ctx = EvalCtx::new(&session.project, cid, comp, time);
    let any_solo = comp.layers.iter().any(|l| l.switches.solo && l.source.is_av() && l.is_active_at(time));
    comp.layers
        .iter()
        .filter(|l| l.is_active_at(time) && l.switches.video && !l.switches.locked && !l.is_camera() && !l.is_light() && (!any_solo || l.switches.solo))
        .filter(|l| quad(&ctx, l).is_some_and(|q| inside(at, &q)))
        .map(|l| l.id.0)
        .collect()
}

/// A layer's content outline in composition pixels (clockwise from its top left), for drawing a selection.
pub fn layer_outline(session: &Session, layer: u64) -> Option<[[f64; 2]; 4]> {
    let (cid, comp) = (session.active_comp_id()?, session.active_comp()?);
    let ctx = EvalCtx::new(&session.project, cid, comp, session.time_of(cid));
    quad(&ctx, comp.layers.iter().find(|l| l.id.0 == layer)?)
}

/// How a layer's own pixels map onto the composition (layer pixels to composition pixels).
pub fn layer_to_comp(session: &Session, layer: u64) -> Option<Mat3> {
    let (cid, comp) = (session.active_comp_id()?, session.active_comp()?);
    let ctx = EvalCtx::new(&session.project, cid, comp, session.time_of(cid));
    let l = comp.layers.iter().find(|l| l.id.0 == layer)?;
    let (m, _) = ctx.layer_to_comp(l);
    Some(m)
}

/// How a mask's or shape path's own space maps onto the composition: the layer's transform, and for
/// a shape path the transforms of the groups around it. `group` is the mask's or path item's id.
pub fn path_to_comp(session: &Session, layer: u64, group: u64) -> Option<Mat3> {
    let (cid, comp) = (session.active_comp_id()?, session.active_comp()?);
    let ctx = EvalCtx::new(&session.project, cid, comp, session.time_of(cid));
    let l = comp.layers.iter().find(|l| l.id.0 == layer)?;
    let (m, _) = ctx.layer_to_comp(l);
    // Masks are not among a shape layer's contents, so they get the layer's matrix alone.
    let item = effectcraft_engine::render::shapes::item_matrix(&ctx, l, group).unwrap_or(Mat3::IDENTITY);
    Some(m * item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use crate::ops::NewLayer;

    /// A full-frame solid with a small star in front of it, in a 320 x 180 composition.
    fn scene() -> (Editor, u64, u64) {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        e.new_comp("Pick", 320, 180, 24.0, 2.0).unwrap();
        let solid = e.add_layer(NewLayer::Solid).unwrap();
        let star = e.add_layer(NewLayer::Star).unwrap();
        (e, solid, star)
    }

    #[test]
    fn a_point_finds_the_layers_under_it_front_to_back() {
        let (e, solid, star) = scene();
        let centre = layers_at(e.session(), [160.0, 90.0]);
        assert_eq!(centre, vec![star, solid], "the star is in front of the solid at the centre");
        assert_eq!(layers_at(e.session(), [4.0, 4.0]), vec![solid], "the corner is only the solid");
        assert!(layers_at(e.session(), [-50.0, 90.0]).is_empty(), "outside the picture is nothing");
    }

    #[test]
    fn locked_and_hidden_layers_are_not_hit() {
        let (mut e, solid, star) = scene();
        e.toggle_switch(star, "lock", true).unwrap();
        assert_eq!(layers_at(e.session(), [160.0, 90.0]), vec![solid]);
        e.toggle_switch(star, "lock", false).unwrap();
        e.toggle_switch(star, "video", false).unwrap();
        assert_eq!(layers_at(e.session(), [160.0, 90.0]), vec![solid]);
    }

    #[test]
    fn a_layer_follows_its_transform() {
        let (mut e, _, star) = scene();
        let at = |e: &Editor| {
            let m = layer_to_comp(e.session(), star).unwrap();
            let o = m.apply(effectcraft_engine::geom::vec2(0.0, 0.0));
            (o.x, o.y)
        };
        let home = at(&e);
        e.set_position(star, 40.0, 40.0).unwrap();
        let moved = at(&e);
        assert!((moved.0 - home.0 - (40.0 - 160.0)).abs() < 1e-6 && (moved.1 - home.1 - (40.0 - 90.0)).abs() < 1e-6, "{home:?} -> {moved:?}");
        assert_eq!(layers_at(e.session(), [40.0, 40.0]).first(), Some(&star), "it is hit at its new position");
    }

    #[test]
    fn nothing_is_found_without_a_composition() {
        let mut e = Editor::new(effectcraft_host::session());
        e.new_project().unwrap();
        assert!(layers_at(e.session(), [1.0, 1.0]).is_empty());
        assert!(layer_to_comp(e.session(), 1).is_none());
    }
}
