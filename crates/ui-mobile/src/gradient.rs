//! A gradient as the editor works on it: colour stops and opacity stops along 0..1. Pure data and
//! maths (adding, moving and removing stops, sampling a colour), kept apart from the sheet that
//! draws it so it can be tested on its own. The JSON form is the engine's property value
//! (`{"t":"Gradient","v":{"colors":[[pos,[r,g,b,a]]…],"opacities":[[pos,a]…]}}`).

use serde_json::{Value, json};

/// Which row of stops: colour (below the bar) or opacity (above it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track {
    Color,
    Opacity,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GradientData {
    /// `(position 0..1, straight rgba 0..1)`, in order along the bar.
    pub colors: Vec<(f64, [f32; 4])>,
    /// `(position 0..1, opacity 0..1)`, in order along the bar.
    pub opacities: Vec<(f64, f32)>,
}

fn unit(p: f64) -> f64 {
    if p.is_finite() { p.clamp(0.0, 1.0) } else { 0.0 }
}

fn lerp(a: f32, b: f32, f: f64) -> f32 {
    a + (b - a) * f as f32
}

/// The value of a track of stops at `t`: the end stops hold outside their range.
fn along<T: Copy>(stops: &[(f64, T)], t: f64, mix: impl Fn(T, T, f64) -> T) -> Option<T> {
    let first = stops.first()?;
    let last = stops.last()?;
    if t <= first.0 {
        return Some(first.1);
    }
    if t >= last.0 {
        return Some(last.1);
    }
    stops.windows(2).find(|w| t >= w[0].0 && t <= w[1].0).map(|w| {
        let span = w[1].0 - w[0].0;
        if span <= 1e-12 { w[1].1 } else { mix(w[0].1, w[1].1, (t - w[0].0) / span) }
    })
}

impl Default for GradientData {
    fn default() -> Self {
        GradientData { colors: vec![(0.0, [1.0, 1.0, 1.0, 1.0]), (1.0, [0.0, 0.0, 0.0, 1.0])], opacities: vec![(0.0, 1.0), (1.0, 1.0)] }
    }
}

impl GradientData {
    /// Reads the engine's JSON value (the `{"t":"Gradient","v":…}` form).
    pub fn from_json(value: &Value) -> Option<GradientData> {
        let v = value.get("v").unwrap_or(value);
        let colors: Vec<(f64, [f32; 4])> = v
            .get("colors")?
            .as_array()?
            .iter()
            .filter_map(|s| {
                let c = s.get(1)?.as_array()?;
                let ch = |i: usize| c.get(i).and_then(Value::as_f64).map(|x| x as f32);
                Some((unit(s.get(0)?.as_f64()?), [ch(0)?, ch(1)?, ch(2)?, ch(3).unwrap_or(1.0)]))
            })
            .collect();
        let opacities: Vec<(f64, f32)> = v
            .get("opacities")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| Some((unit(s.get(0)?.as_f64()?), s.get(1)?.as_f64()? as f32))).collect())
            .unwrap_or_default();
        if colors.is_empty() {
            return None;
        }
        let mut g = GradientData { colors, opacities: if opacities.is_empty() { vec![(0.0, 1.0)] } else { opacities } };
        g.colors.sort_by(|a, b| a.0.total_cmp(&b.0));
        g.opacities.sort_by(|a, b| a.0.total_cmp(&b.0));
        Some(g)
    }

    pub fn to_json(&self) -> Value {
        json!({ "t": "Gradient", "v": { "colors": self.colors.iter().map(|(p, c)| json!([p, c])).collect::<Vec<_>>(),
                                         "opacities": self.opacities.iter().map(|(p, a)| json!([p, a])).collect::<Vec<_>>() } })
    }

    pub fn len(&self, track: Track) -> usize {
        match track {
            Track::Color => self.colors.len(),
            Track::Opacity => self.opacities.len(),
        }
    }

    pub fn position(&self, track: Track, i: usize) -> Option<f64> {
        match track {
            Track::Color => self.colors.get(i).map(|s| s.0),
            Track::Opacity => self.opacities.get(i).map(|s| s.0),
        }
    }

    /// The straight colour (opacity stops not applied) at `t`.
    pub fn color_at(&self, t: f64) -> [f32; 4] {
        along(&self.colors, unit(t), |a, b, f| [lerp(a[0], b[0], f), lerp(a[1], b[1], f), lerp(a[2], b[2], f), lerp(a[3], b[3], f)]).unwrap_or([1.0; 4])
    }

    pub fn opacity_at(&self, t: f64) -> f32 {
        along(&self.opacities, unit(t), lerp).unwrap_or(1.0)
    }

    /// The colour a viewer sees at `t`: the colour with the opacity stops applied to its alpha.
    pub fn sample(&self, t: f64) -> [f32; 4] {
        let c = self.color_at(t);
        [c[0], c[1], c[2], c[3] * self.opacity_at(t)]
    }

    /// Adds a stop at `pos` that takes the gradient's value there, and returns its index.
    pub fn add(&mut self, track: Track, pos: f64) -> usize {
        let pos = unit(pos);
        match track {
            Track::Color => {
                let c = self.color_at(pos);
                let at = self.colors.iter().position(|s| s.0 > pos).unwrap_or(self.colors.len());
                self.colors.insert(at, (pos, c));
                at
            }
            Track::Opacity => {
                let a = self.opacity_at(pos);
                let at = self.opacities.iter().position(|s| s.0 > pos).unwrap_or(self.opacities.len());
                self.opacities.insert(at, (pos, a));
                at
            }
        }
    }

    /// Removes a stop; the last stop of a track stays. True when one was removed.
    pub fn remove(&mut self, track: Track, i: usize) -> bool {
        match track {
            Track::Color if self.colors.len() > 1 && i < self.colors.len() => {
                self.colors.remove(i);
                true
            }
            Track::Opacity if self.opacities.len() > 1 && i < self.opacities.len() => {
                self.opacities.remove(i);
                true
            }
            _ => false,
        }
    }

    /// Moves a stop to `pos`, keeping the track in order, and returns where the stop is now.
    pub fn move_to(&mut self, track: Track, i: usize, pos: f64) -> usize {
        let pos = unit(pos);
        match track {
            Track::Color => {
                let Some(mut s) = self.colors.get(i).copied() else { return i };
                self.colors.remove(i);
                s.0 = pos;
                let at = self.colors.iter().position(|o| o.0 > pos).unwrap_or(self.colors.len());
                self.colors.insert(at, s);
                at
            }
            Track::Opacity => {
                let Some(mut s) = self.opacities.get(i).copied() else { return i };
                self.opacities.remove(i);
                s.0 = pos;
                let at = self.opacities.iter().position(|o| o.0 > pos).unwrap_or(self.opacities.len());
                self.opacities.insert(at, s);
                at
            }
        }
    }

    /// Sets a colour stop's colour (rgb; its alpha stays).
    pub fn set_rgb(&mut self, i: usize, rgb: [f32; 3]) {
        if let Some(s) = self.colors.get_mut(i) {
            s.1 = [rgb[0].clamp(0.0, 1.0), rgb[1].clamp(0.0, 1.0), rgb[2].clamp(0.0, 1.0), s.1[3]];
        }
    }

    pub fn set_opacity(&mut self, i: usize, a: f32) {
        if let Some(s) = self.opacities.get_mut(i) {
            s.1 = if a.is_finite() { a.clamp(0.0, 1.0) } else { 1.0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trips_and_is_put_in_order() {
        let j = json!({"t":"Gradient","v":{"colors":[[1.0,[0.0,0.0,1.0,1.0]],[0.0,[1.0,0.0,0.0,1.0]]],"opacities":[[0.5,0.25]]}});
        let g = GradientData::from_json(&j).unwrap();
        assert_eq!(g.colors[0].0, 0.0, "stops come out in order along the bar");
        assert_eq!(GradientData::from_json(&g.to_json()), Some(g));
        assert!(GradientData::from_json(&json!({"v":{"colors":[]}})).is_none(), "a gradient needs a colour");
        assert!(GradientData::from_json(&json!(5)).is_none());
    }

    #[test]
    fn colours_blend_between_stops_and_hold_outside_them() {
        let g = GradientData::default();
        assert_eq!(g.color_at(0.0), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(g.color_at(1.0), [0.0, 0.0, 0.0, 1.0]);
        assert!((g.color_at(0.5)[0] - 0.5).abs() < 1e-6);
        assert_eq!(g.color_at(-3.0), g.color_at(0.0));
        assert_eq!(g.color_at(f64::NAN), g.color_at(0.0));
    }

    #[test]
    fn a_new_stop_takes_the_value_already_there() {
        let mut g = GradientData::default();
        let i = g.add(Track::Color, 0.5);
        assert_eq!(i, 1);
        assert!((g.colors[1].1[0] - 0.5).abs() < 1e-6, "grey in the middle");
        assert_eq!(g.colors.len(), 3);
        let before = g.sample(0.25);
        let j = g.add(Track::Opacity, 0.25);
        assert_eq!(g.opacities[j].1, 1.0);
        assert_eq!(g.sample(0.25), before, "adding a stop changes nothing you can see");
    }

    #[test]
    fn moving_a_stop_past_another_keeps_the_track_in_order() {
        let mut g = GradientData::default();
        g.add(Track::Color, 0.5);
        g.set_rgb(1, [0.0, 1.0, 0.0]);
        let at = g.move_to(Track::Color, 1, 1.0);
        assert_eq!(at, 2, "now the last stop");
        assert!(g.colors.windows(2).all(|w| w[0].0 <= w[1].0));
        assert_eq!(g.colors[2].1[..3], [0.0, 1.0, 0.0], "the green stop moved, the others kept their colours");
        assert_eq!(g.move_to(Track::Color, 9, 0.2), 9, "a missing stop is left alone");
        assert_eq!(g.colors.len(), 3);
    }

    #[test]
    fn the_last_stop_of_a_track_cannot_be_removed() {
        let mut g = GradientData::default();
        assert!(g.remove(Track::Color, 0));
        assert!(!g.remove(Track::Color, 0), "one colour must stay");
        assert!(g.remove(Track::Opacity, 1));
        assert!(!g.remove(Track::Opacity, 0));
        assert!(!g.remove(Track::Opacity, 7));
        assert_eq!((g.len(Track::Color), g.len(Track::Opacity)), (1, 1));
    }

    #[test]
    fn opacity_stops_scale_the_alpha_a_viewer_sees() {
        let mut g = GradientData { opacities: vec![(0.0, 1.0), (1.0, 0.0)], ..GradientData::default() };
        assert!((g.sample(0.5)[3] - 0.5).abs() < 1e-6);
        g.set_opacity(0, f32::NAN);
        assert_eq!(g.opacities[0].1, 1.0, "a bad value cannot get in");
    }
}
