//! Renders the touch workspace to a PPM image on the CPU (no GPU needed), for looking at layouts.
//!
//! `cargo run -p effectia-ui-mobile --example shot -- <width> <height> <ppp> <out.ppm> [steps]`
//! where steps is a `;` list of: `mask:<rect|ellipse>`, `picker:<folder>`, `points`, `gradient`, `ease:<drag|->`, `select:<layer id>`, `time:<seconds>`, `props`, `notimeline`.

use std::collections::HashMap;

use effectia_ui_mobile::editor::Editor;
use effectia_ui_mobile::ui::{MobileApp, apply_theme};
use egui::epaint::{ImageData, Primitive};
use egui::{Color32, Event, Pos2, RawInput, Rect, TextureId, vec2};

struct Tex {
    w: usize,
    h: usize,
    px: Vec<Color32>,
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (w, h): (f32, f32) = (a[1].parse().unwrap_or(360.0), a[2].parse().unwrap_or(780.0));
    let ppp: f32 = a[3].parse().unwrap_or(2.0);
    let out = a[4].clone();
    let steps = a.get(5).cloned().unwrap_or_default();

    let ctx = egui::Context::default();
    apply_theme(&ctx);
    let mut editor = Editor::new(effectcraft_host::session());
    editor.open_demo().ok();
    let mut app = MobileApp::new(editor);
    // `picker:<dir>` browses that folder as the phone's shared storage.
    if let Some(dir) = steps.split(';').find_map(|s| s.strip_prefix("picker:")) {
        use effectia_ui_mobile::platform::{Desktop, Needs, Root};
        app = app.with_storage(std::env::temp_dir().join("effectia-shot-app")).with_platform(Desktop::with_roots(vec![Root::new(
            "Samples",
            dir,
            Needs::Nothing,
        )]));
    }
    if w >= 600.0 && h >= 500.0 {
        app.layout.inspector_open = true;
    }
    for s in steps.split(';').filter(|s| !s.is_empty()) {
        match s.split_once(':') {
            Some(("media", path)) => {
                // A fresh project holding one clip, selected.
                app.editor.new_project().ok();
                app.editor.new_comp("Clip", 1280, 720, 24.0, 2.0).ok();
                if let Ok(layer) = app.editor.import_and_add(path) {
                    app.editor.select_layer(layer).ok();
                }
            }
            Some(("ease", preset)) => {
                // A layer fading 100 -> 20 over a second, its first key selected, the ease editor open.
                app.editor.new_project().ok();
                app.editor.new_comp("Fade", 1280, 720, 24.0, 4.0).ok();
                let layer = app.editor.add_layer(effectia_ui_mobile::ops::NewLayer::Solid).unwrap_or(0);
                app.editor.set_time(0.0).ok();
                app.editor.toggle_key(layer, "transform/opacity").ok();
                app.editor.set_time(1.0).ok();
                app.editor.toggle_key(layer, "transform/opacity").ok();
                app.editor.set_scalar(layer, "transform/opacity", 20.0).ok();
                app.editor.select_layer(layer).ok();
                app.select_keyframe(layer, "transform/opacity", 0.0);
                app.open_sheet("ease");
                if preset == "drag" {
                    app.drag_ease_handle(true, 0.62, 1.25);
                    app.drag_ease_handle(false, 0.35, 0.1);
                }
            }
            Some(("expression", path)) => {
                let layer = app.editor.comp().ok().flatten().and_then(|c| c.layers.iter().find(|l| l.selected).map(|l| l.id));
                if let Some(layer) = layer {
                    app.open_expression_editor(layer, path);
                }
            }
            Some(("mask", shape)) => {
                // A mask on the selected layer, its group open in the property list.
                let layer = app.editor.comp().ok().flatten().and_then(|c| c.layers.iter().find(|l| l.selected).map(|l| l.id));
                if let Some(layer) = layer {
                    let shape = if shape == "ellipse" { effectia_ui_mobile::ops::MaskShape::Ellipse } else { effectia_ui_mobile::ops::MaskShape::Rectangle };
                    if let Ok(mask) = app.editor.add_mask(layer, shape) {
                        app.show_new_mask(layer, mask);
                    }
                }
            }
            Some(("points", _)) => {
                // Point editing on the selected layer's first mask (add one with `mask:` first).
                let layer = app.editor.comp().ok().flatten().and_then(|c| c.layers.iter().find(|l| l.selected).map(|l| l.id));
                if let Some(layer) = layer
                    && let Ok(tree) = app.editor.layer_tree(layer, 6)
                    && let Some(m) = tree.masks().first()
                    && let Some(p) = m.path_prop()
                {
                    app.edit_points(layer, m.uid, p.uid);
                }
            }
            Some(("gradient", _)) => {
                // A star with a three-stop gradient fill, its editor open on the middle stop.
                app.editor.new_project().ok();
                app.editor.new_comp("Gradient", 1280, 720, 24.0, 2.0).ok();
                if let Ok(layer) = app.editor.add_layer(effectia_ui_mobile::ops::NewLayer::Star) {
                    app.editor.select_layer(layer).ok();
                    app.editor.add_gradient(layer, false).ok();
                    if let Ok(tree) = app.editor.layer_tree(layer, 8)
                        && let Some(n) = tree.find_kind("gradient")
                    {
                        let path = format!("@{}", n.uid);
                        if let Ok(mut g) = app.editor.gradient(layer, &path) {
                            let i = g.add(effectia_ui_mobile::gradient::Track::Color, 0.45);
                            g.set_rgb(0, [1.0, 0.35, 0.2]);
                            g.set_rgb(i, [0.95, 0.85, 0.2]);
                            g.set_rgb(2, [0.2, 0.3, 0.9]);
                            g.opacities = vec![(0.0, 1.0), (0.8, 1.0), (1.0, 0.25)];
                            app.editor.set_gradient(layer, &path, &g).ok();
                            app.edit_gradient(layer, &path, "Colors");
                        }
                    }
                }
            }
            Some(("select", id)) => {
                app.editor.select_layer(id.parse().unwrap_or(0)).ok();
            }
            Some(("time", t)) => {
                app.editor.set_time(t.parse().unwrap_or(0.0)).ok();
            }
            Some(("picker", _)) => {
                app.open_sheet("media");
            }
            Some(("sheet", name)) => {
                app.open_sheet(name);
            }
            Some(("fx", id)) => {
                if let Ok(Some(c)) = app.editor.comp()
                    && let Some(l) = c.layers.iter().find(|l| l.selected)
                    && let Ok(path) = app.editor.apply_effect(l.id, id)
                {
                    app.expand(&path);
                }
            }
            Some(("expand", path)) => app.expand(path),
            Some(("storage", dir)) => app = app.with_storage(dir),
            Some(("roots", list)) => {
                // roots:Label=/path|Label=/path  (readable folders for the media picker)
                let roots = list
                    .split('|')
                    .filter_map(|e| e.split_once('='))
                    .map(|(l, p)| effectia_ui_mobile::platform::Root::new(l, p, effectia_ui_mobile::platform::Needs::Nothing))
                    .collect();
                app = app.with_platform(effectia_ui_mobile::platform::Desktop::with_roots(roots));
            }
            Some(("saveas", name)) => {
                // Saves the open project into storage/projects/<name>.ecproj.
                let dir = app.storage.join("projects");
                std::fs::create_dir_all(&dir).ok();
                app.editor.save(Some(&dir.join(format!("{name}.ecproj")).to_string_lossy())).ok();
            }
            Some(("key", spec)) => {
                // key:<layer>:<path>:<seconds> adds a keyframe there.
                let parts: Vec<&str> = spec.split(':').collect();
                if let [layer, path, t] = parts[..] {
                    app.editor.set_time(t.parse().unwrap_or(0.0)).ok();
                    app.editor.toggle_key(layer.parse().unwrap_or(0), path).ok();
                }
            }
            Some(("cmd", spec)) => {
                if let Some((id, params)) = spec.split_once('=') {
                    app.editor.command(id, serde_json::from_str(params).unwrap_or_default()).ok();
                }
            }
            _ => match s {
                "fxtab" => app.show_panel(true),
                "gallery" => {
                    app.show_gallery();
                }
                "props" => app.layout.inspector_open = true,
                "notimeline" => app.layout.timeline_open = false,
                _ => {}
            },
        }
    }

    let mut textures: HashMap<TextureId, Tex> = HashMap::new();
    let mut last = None;
    let mut settle = 0;
    for frame in 0..800 {
        if frame > 0 && (app.gallery_busy() || app.sheet_open() || app.render_busy()) {
            // Gallery previews are drawn on a worker; give it time.
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(w, h))),
            viewports: std::iter::once((egui::ViewportId::ROOT, egui::ViewportInfo { native_pixels_per_point: Some(ppp), ..Default::default() })).collect(),
            events: Vec::<Event>::new(),
            ..Default::default()
        };
        let mut full = ctx.run_ui(raw, |ui| app.show(ui));
        for (id, deltas) in &full.textures_delta.set {
            for delta in deltas.iter() {
                let ImageData::Color(img) = &delta.image;
                let t =
                    textures.entry(*id).or_insert_with(|| Tex { w: img.size[0], h: img.size[1], px: vec![Color32::TRANSPARENT; img.size[0] * img.size[1]] });
                match delta.pos {
                    None => {
                        t.w = img.size[0];
                        t.h = img.size[1];
                        t.px = img.pixels.clone();
                    }
                    Some([x0, y0]) => {
                        for y in 0..img.size[1] {
                            for x in 0..img.size[0] {
                                t.px[(y0 + y) * t.w + x0 + x] = img.pixels[y * img.size[0] + x];
                            }
                        }
                    }
                }
            }
        }
        full.textures_delta.clear();
        last = Some((ctx.tessellate(full.shapes, full.pixels_per_point), full.pixels_per_point));
        settle = if app.gallery_busy() || app.render_busy() || (app.sheet_open() && frame < 40) { 0 } else { settle + 1 };
        if settle >= 4 {
            break;
        }
    }
    let Some((prims, ppp)) = last else { return };
    let (pw, ph) = ((w * ppp) as usize, (h * ppp) as usize);
    let mut fb = vec![[0.0_f32; 4]; pw * ph];
    for cp in &prims {
        let Primitive::Mesh(m) = &cp.primitive else { continue };
        let Some(tex) = textures.get(&m.texture_id) else { continue };
        let clip = (cp.clip_rect.min.x * ppp, cp.clip_rect.min.y * ppp, cp.clip_rect.max.x * ppp, cp.clip_rect.max.y * ppp);
        for tri in m.indices.chunks_exact(3) {
            let v = [m.vertices[tri[0] as usize], m.vertices[tri[1] as usize], m.vertices[tri[2] as usize]];
            let p = [v[0].pos * ppp, v[1].pos * ppp, v[2].pos * ppp];
            let minx = p.iter().map(|q| q.x).fold(f32::MAX, f32::min).max(clip.0).max(0.0).floor() as usize;
            let maxx = p.iter().map(|q| q.x).fold(f32::MIN, f32::max).min(clip.2).min(pw as f32).ceil() as usize;
            let miny = p.iter().map(|q| q.y).fold(f32::MAX, f32::min).max(clip.1).max(0.0).floor() as usize;
            let maxy = p.iter().map(|q| q.y).fold(f32::MIN, f32::max).min(clip.3).min(ph as f32).ceil() as usize;
            let area = (p[1].x - p[0].x) * (p[2].y - p[0].y) - (p[2].x - p[0].x) * (p[1].y - p[0].y);
            if area.abs() < 1e-6 {
                continue;
            }
            for y in miny..maxy {
                for x in minx..maxx {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((p[1].x - fx) * (p[2].y - fy) - (p[2].x - fx) * (p[1].y - fy)) / area;
                    let w1 = ((p[2].x - fx) * (p[0].y - fy) - (p[0].x - fx) * (p[2].y - fy)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < -0.001 || w1 < -0.001 || w2 < -0.001 {
                        continue;
                    }
                    let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                    let vv = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                    // Bilinear texture sample.
                    let (tx, ty) = (u * tex.w as f32 - 0.5, vv * tex.h as f32 - 0.5);
                    let (x0, y0) = (tx.floor(), ty.floor());
                    let (fxx, fyy) = (tx - x0, ty - y0);
                    let at = |xx: f32, yy: f32| {
                        let xi = (xx.max(0.0) as usize).min(tex.w - 1);
                        let yi = (yy.max(0.0) as usize).min(tex.h - 1);
                        let c = tex.px[yi * tex.w + xi];
                        [f32::from(c.r()), f32::from(c.g()), f32::from(c.b()), f32::from(c.a())]
                    };
                    let (c00, c10, c01, c11) = (at(x0, y0), at(x0 + 1.0, y0), at(x0, y0 + 1.0), at(x0 + 1.0, y0 + 1.0));
                    let mut s = [0.0; 4];
                    for k in 0..4 {
                        s[k] = (c00[k] * (1.0 - fxx) + c10[k] * fxx) * (1.0 - fyy) + (c01[k] * (1.0 - fxx) + c11[k] * fxx) * fyy;
                    }
                    let vc = |k: usize| {
                        w0 * f32::from(v[0].color.to_array()[k]) + w1 * f32::from(v[1].color.to_array()[k]) + w2 * f32::from(v[2].color.to_array()[k])
                    };
                    let src = [s[0] * vc(0) / 255.0, s[1] * vc(1) / 255.0, s[2] * vc(2) / 255.0, s[3] * vc(3) / 255.0];
                    let d = &mut fb[y * pw + x];
                    let inv = 1.0 - src[3] / 255.0;
                    for k in 0..4 {
                        d[k] = src[k] + d[k] * inv;
                    }
                }
            }
        }
    }
    let mut data = format!("P6\n{pw} {ph}\n255\n").into_bytes();
    for px in &fb {
        data.extend([px[0].clamp(0.0, 255.0) as u8, px[1].clamp(0.0, 255.0) as u8, px[2].clamp(0.0, 255.0) as u8]);
    }
    std::fs::write(&out, data).ok();
}
