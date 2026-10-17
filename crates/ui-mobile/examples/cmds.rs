//! Prints engine commands whose id contains the given text (with their parameter docs).
fn main() {
    let q = std::env::args().nth(1).unwrap_or_default();
    let mut s = effectcraft_host::session();
    s.execute("file.openDemoProject", serde_json::json!({})).ok();
    if let Ok(v) = s.execute("command.list", serde_json::json!({ "filter": q })) {
        for c in v.as_array().into_iter().flatten() {
            println!("{}", c);
        }
    }
}
