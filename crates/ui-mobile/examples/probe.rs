use effectia_ui_mobile::editor::Editor;
use effectia_ui_mobile::ops::RenderFormat;
use serde_json::json;
fn main() {
    let mut e = Editor::new(effectcraft_host::session());
    e.open_demo().ok();
    let dir = std::env::temp_dir().join("effectia-probe");
    std::fs::create_dir_all(&dir).ok();
    let mp4 = dir.join("clip.mp4");
    if !mp4.exists() {
        // short comp: 2 s
        e.new_comp("Clip", 640, 360, 24.0, 2.0).ok();
        e.add_layer(effectia_ui_mobile::ops::NewLayer::Star).ok();
        let r = e.render_to_file(mp4.to_str().unwrap(), RenderFormat::Mp4, false);
        println!("render {:?}", r);
    }
    e.new_project().ok();
    e.new_comp("Main", 640, 360, 24.0, 2.0).ok();
    let r = e.import_and_add(mp4.to_str().unwrap());
    println!("import {:?}", r);
    let comp = e.comp().ok().flatten().unwrap();
    for l in &comp.layers {
        println!("layer {} {} {}", l.id, l.name, l.kind);
    }
    let id = comp.layers[0].id;
    e.select_layer(id).ok();
    let t = e.layer_tree(id, 4).unwrap();
    for n in t.find("transform").map(|n| n.children.clone()).unwrap_or_default() {
        println!("{} {:?} {:?}", n.path, n.name, n.value);
    }
    let r = e.command("objectMatte.apply", json!({"layer": id, "point": [320, 180]}));
    println!("apply {:?}", r.as_ref().map(|v| v.to_string()));
    let eff = r.ok().and_then(|v| v.get("effect").and_then(|x| x.as_u64())).unwrap_or(0);
    let t = std::time::Instant::now();
    let pr = e.command("objectMatte.propagate", json!({"layer": id, "effect": eff, "direction": "both", "wait": false}));
    println!("propagate {:?}", pr.as_ref().map(|v| v.to_string()));
    for i in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let st = e.command("objectMatte.status", json!({"layer": id, "effect": eff})).unwrap();
        println!(
            "{i} running={} pending={} computed={} progress={}",
            st["running"],
            st["pending"],
            st["computed"].as_array().map_or(0, |a| a.len()),
            st["progress"]
        );
        if st["running"] == json!(false) && i > 1 {
            break;
        }
    }
    println!("took {:?}", t.elapsed());
    let a = e.render(320).ok();
    println!("render ok {}", a.is_some());
}
