use effectcraft_ui_mobile::editor::Editor;
use std::time::Instant;
fn main() {
    let mut e = Editor::new(effectcraft_host::session());
    e.open_demo().ok();
    let t = Instant::now();
    for _ in 0..20 {
        e.comp().ok();
    }
    println!("comp() x20: {:?}", t.elapsed());
    let comp = e.comp().ok().flatten().unwrap();
    for l in &comp.layers {
        let t = Instant::now();
        let tree = e.layer_tree(l.id, 8).ok();
        println!("tree {} {:?} nodes={}", l.name, t.elapsed(), tree.map_or(0, |t| t.all_keys().len()));
    }
    let t = Instant::now();
    for _ in 0..5 {
        e.render(540).ok();
    }
    println!("render 540 x5: {:?}", t.elapsed());
    let t = Instant::now();
    for _ in 0..3 {
        e.set_time(1.0 + 0.1).ok();
        e.render(960).ok();
        e.step_frames(3).ok();
    }
    println!("render 960 x3: {:?}", t.elapsed());
}
