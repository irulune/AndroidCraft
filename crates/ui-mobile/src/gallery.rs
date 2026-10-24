//! The project gallery: the saved projects in the app's `projects` folder, each with a cached
//! thumbnail and a line of facts (size, length, layer count).
//!
//! Everything here is plain files and engine sessions, so it is testable without a window:
//!
//! * [`Library::scan`] lists `projects/*.ecproj` and attaches whatever is already cached.
//! * [`Library::make_thumbnail`] opens a project on a *scratch* session (never the one being
//!   edited), renders one frame and writes `thumbs/<file>.png` plus `thumbs/<file>.json`. The cache
//!   is keyed by the project file's modified time and size, so saving a project makes it stale and
//!   the next visit redraws it. A project that cannot be opened is cached as an error, so it is not
//!   retried on every visit.
//! * [`Library::rename`], [`Library::duplicate`] and [`Library::delete`] keep the thumbnails in step.
//! * [`order`] filters and sorts for the search box and the sort chips.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Value, json};

use crate::editor::{Editor, EditorError, Result};

/// Longest side of a stored thumbnail, in pixels.
pub const THUMB_SIDE: u32 = 320;
/// The cache key (and `path`) of the bundled demo project, which is not a file in `projects/`.
pub const DEMO_KEY: &str = "_demo";
/// Bumped when the cache layout or the way a thumbnail is drawn changes, to invalidate old ones.
const CACHE_VERSION: u32 = 1;
const EXT: &str = "ecproj";

fn err(msg: impl Into<String>) -> EditorError {
    EditorError(msg.into())
}

/// A file-system-safe project name: letters, digits, space, `-` and `_`, at most 60 characters.
/// Never empty.
pub fn safe_name(s: &str) -> String {
    let cleaned: String = s.trim().trim_matches('.').chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let t: String = cleaned.chars().take(60).collect();
    let t = t.trim().to_string();
    if t.is_empty() { "Untitled".into() } else { t }
}

/// `dir/stem.ext`, or `dir/stem-2.ext`, `-3`, ... when that exists.
pub fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..10_000).map(|n| dir.join(format!("{stem}-{n}.{ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

/// What the gallery shows about a project besides its picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    /// The active composition's name.
    pub comp: String,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub frame_rate: f64,
    /// Layers in the active composition.
    pub layers: usize,
}

/// One project in the gallery.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The `.ecproj` file, or [`DEMO_KEY`] for the bundled demo.
    pub path: PathBuf,
    /// The file name without extension.
    pub name: String,
    /// Last modified, seconds since the Unix epoch (0 for the demo).
    pub modified: u64,
    pub bytes: u64,
    /// Facts from the cache; `None` until the project has been looked at.
    pub info: Option<Info>,
    /// The cached picture, when there is one and it matches the file.
    pub thumb: Option<PathBuf>,
    /// Why the project cannot be opened, from the cache.
    pub error: Option<String>,
}

impl Entry {
    pub fn is_demo(&self) -> bool {
        self.path.as_os_str() == DEMO_KEY
    }

    /// True when the cache does not describe the file as it is now.
    pub fn needs_thumbnail(&self) -> bool {
        self.info.is_none() && self.error.is_none()
    }

    /// "1920×1080 · 10 s", or the reason the project cannot be opened.
    pub fn facts(&self) -> String {
        if let Some(e) = &self.error {
            return format!("Can't be opened: {e}");
        }
        match &self.info {
            Some(i) if i.width > 0 => {
                format!("{}\u{00d7}{} \u{00b7} {}", i.width, i.height, duration_label(i.duration))
            }
            Some(_) => "No composition yet".into(),
            None => "Preparing preview\u{2026}".into(),
        }
    }
}

impl Entry {
    /// "8 layers", when the project has a composition.
    pub fn layers_label(&self) -> Option<String> {
        self.info.as_ref().filter(|i| i.width > 0).map(|i| plural(i.layers, "layer"))
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

/// "10 s", "1:05", "1:02:03".
pub fn duration_label(seconds: f64) -> String {
    let s = if seconds.is_finite() { seconds.max(0.0).round() as u64 } else { 0 };
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{}:{:02}", s / 60, s % 60),
        _ => format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60),
    }
}

/// "Just now", "5 min ago", "3 h ago", "Yesterday", "4 days ago", "2 months ago". A time in the
/// future (clock changes) reads as "Just now".
pub fn age_label(now: u64, then: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..=59 => "Just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86_399 => format!("{} h ago", d / 3600),
        86_400..=172_799 => "Yesterday".into(),
        172_800..=2_591_999 => format!("{} days ago", d / 86_400),
        2_592_000..=31_535_999 => {
            let m = d / 2_592_000;
            if m == 1 { "1 month ago".into() } else { format!("{m} months ago") }
        }
        _ => {
            let y = d / 31_536_000;
            if y == 1 { "1 year ago".into() } else { format!("{y} years ago") }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sort {
    /// Most recently saved first.
    #[default]
    Recent,
    /// A to Z.
    Name,
}

/// The indices of `entries` to show, for a search `query` and a sort order. The query matches the
/// project name or its composition name, ignoring case.
pub fn order(entries: &[Entry], query: &str, sort: Sort) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    let mut idx: Vec<usize> = (0..entries.len())
        .filter(|&i| {
            let Some(e) = entries.get(i) else { return false };
            needle.is_empty() || e.name.to_lowercase().contains(&needle) || e.info.as_ref().is_some_and(|i| i.comp.to_lowercase().contains(&needle))
        })
        .collect();
    let key = |i: usize| entries.get(i).map(|e| (e.modified, e.name.to_lowercase())).unwrap_or_default();
    match sort {
        Sort::Recent => idx.sort_by(|&a, &b| {
            let (ka, kb) = (key(a), key(b));
            kb.0.cmp(&ka.0).then_with(|| ka.1.cmp(&kb.1))
        }),
        Sort::Name => idx.sort_by(|&a, &b| key(a).1.cmp(&key(b).1)),
    }
    idx
}

/// What to draw a thumbnail of.
#[derive(Clone, Debug)]
pub enum Source {
    Project(PathBuf),
    Demo,
}

/// The app's storage folder, seen as a library of projects.
#[derive(Clone, Debug)]
pub struct Library {
    root: PathBuf,
}

fn stamp_of(path: &Path) -> Result<(u64, u64)> {
    let m = fs::metadata(path).map_err(|e| err(format!("{}: {e}", path.display())))?;
    let secs = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    Ok((secs, m.len()))
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn is_project(path: &Path) -> bool {
    path.is_file() && path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(EXT))
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Library {
        Library { root: root.into() }
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.root.join("projects")
    }

    pub fn thumbs_dir(&self) -> PathBuf {
        self.root.join("thumbs")
    }

    fn sidecars(&self, key: &str) -> (PathBuf, PathBuf) {
        let t = self.thumbs_dir();
        (t.join(format!("{key}.png")), t.join(format!("{key}.json")))
    }

    fn stamp_string(secs: u64, bytes: u64) -> String {
        format!("v{CACHE_VERSION}-{secs}-{bytes}")
    }

    /// Reads the cache for `key` if it describes the file stamped `stamp`.
    fn cached(&self, key: &str, stamp: &str) -> (Option<Info>, Option<PathBuf>, Option<String>) {
        let (png, json_path) = self.sidecars(key);
        let Some(v) = fs::read_to_string(&json_path).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) else {
            return (None, None, None);
        };
        if v.get("stamp").and_then(Value::as_str) != Some(stamp) {
            return (None, None, None);
        }
        if let Some(e) = v.get("error").and_then(Value::as_str) {
            return (None, None, Some(e.to_string()));
        }
        let num = |k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let info = Info {
            comp: v.get("comp").and_then(Value::as_str).unwrap_or_default().to_string(),
            width: num("width") as u32,
            height: num("height") as u32,
            duration: num("duration"),
            frame_rate: num("frameRate"),
            layers: num("layers") as usize,
        };
        (Some(info), png.is_file().then_some(png), None)
    }

    /// The bundled demo as a gallery entry (its picture comes from the cache like any other).
    pub fn demo(&self) -> Entry {
        let (info, thumb, error) = self.cached(DEMO_KEY, &Self::stamp_string(0, 0));
        Entry { path: PathBuf::from(DEMO_KEY), name: "Effectia Intro".into(), modified: 0, bytes: 0, info, thumb, error }
    }

    /// Every project in `projects/`, newest first, with what the cache knows.
    pub fn scan(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = fs::read_dir(self.projects_dir())
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .filter(|p| is_project(p))
            .filter_map(|p| {
                let (modified, bytes) = stamp_of(&p).ok()?;
                let (info, thumb, error) = self.cached(&file_name(&p), &Self::stamp_string(modified, bytes));
                let name = p.file_stem()?.to_string_lossy().into_owned();
                Some(Entry { path: p, name, modified, bytes, info, thumb, error })
            })
            .collect();
        out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        out
    }

    /// The sources whose cache is missing or stale, the demo first.
    pub fn stale(&self, entries: &[Entry]) -> Vec<Source> {
        let demo = self.demo();
        let mut v = Vec::new();
        if demo.needs_thumbnail() {
            v.push(Source::Demo);
        }
        v.extend(entries.iter().filter(|e| e.needs_thumbnail()).map(|e| Source::Project(e.path.clone())));
        v
    }

    fn write_cache(&self, key: &str, stamp: &str, body: Value) -> Result<()> {
        let dir = self.thumbs_dir();
        fs::create_dir_all(&dir).map_err(|e| err(format!("{}: {e}", dir.display())))?;
        let mut v = body;
        if let Some(o) = v.as_object_mut() {
            o.insert("stamp".into(), json!(stamp));
        }
        let (_, json_path) = self.sidecars(key);
        let tmp = json_path.with_extension("json.tmp");
        fs::write(&tmp, v.to_string()).map_err(|e| err(format!("{}: {e}", tmp.display())))?;
        fs::rename(&tmp, &json_path).map_err(|e| err(format!("{}: {e}", json_path.display())))
    }

    /// Draws one thumbnail on a scratch session and records the facts. A project that cannot be
    /// opened is recorded as an error and reported as `Ok`: the gallery shows it, not hides it.
    pub fn make_thumbnail(&self, source: &Source) -> Result<()> {
        let mut e = Editor::new(effectcraft_host::session());
        let (key, stamp) = match source {
            Source::Demo => (DEMO_KEY.to_string(), Self::stamp_string(0, 0)),
            Source::Project(p) => {
                let (secs, bytes) = stamp_of(p)?;
                (file_name(p), Self::stamp_string(secs, bytes))
            }
        };
        let opened = match source {
            Source::Demo => e.open_demo(),
            Source::Project(p) => e.open(&p.to_string_lossy()),
        };
        if let Err(why) = opened {
            return self.write_cache(&key, &stamp, json!({ "error": short(&why.0) }));
        }
        let Some(comp) = e.comp()? else {
            return self.write_cache(&key, &stamp, json!({ "comp": "", "width": 0, "height": 0, "duration": 0.0, "frameRate": 0.0, "layers": 0 }));
        };
        // Where the project was left, else a quarter of the way in: frame zero is often still empty.
        let at = if comp.time > 0.0 { comp.time } else { (comp.duration * 0.25).min(3.0) };
        e.set_time(at.min((comp.duration - 0.05).max(0.0)))?;
        let frame = e.render(THUMB_SIDE)?;
        let (png, _) = self.sidecars(&key);
        let dir = self.thumbs_dir();
        fs::create_dir_all(&dir).map_err(|er| err(format!("{}: {er}", dir.display())))?;
        let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba).ok_or_else(|| err("the preview has the wrong size"))?;
        let tmp = png.with_extension("png.tmp");
        img.save_with_format(&tmp, image::ImageFormat::Png).map_err(|er| err(format!("preview: {er}")))?;
        fs::rename(&tmp, &png).map_err(|er| err(format!("{}: {er}", png.display())))?;
        self.write_cache(
            &key,
            &stamp,
            json!({
                "comp": comp.name, "width": comp.width, "height": comp.height,
                "duration": comp.duration, "frameRate": comp.frame_rate, "layers": comp.layers.len(),
            }),
        )
    }

    fn move_sidecars(&self, from: &str, to: &str) {
        let (a_png, a_json) = self.sidecars(from);
        let (b_png, b_json) = self.sidecars(to);
        let _ = fs::rename(a_png, b_png);
        let _ = fs::rename(a_json, b_json);
    }

    fn remove_sidecars(&self, key: &str) {
        let (png, json_path) = self.sidecars(key);
        let _ = fs::remove_file(png);
        let _ = fs::remove_file(json_path);
    }

    fn check_inside(&self, path: &Path) -> Result<()> {
        if path.parent() != Some(self.projects_dir().as_path()) || !is_project(path) {
            return Err(err("that is not one of your projects"));
        }
        Ok(())
    }

    /// Renames a project file (and its thumbnail). Returns the new path.
    pub fn rename(&self, path: &Path, new_name: &str) -> Result<PathBuf> {
        self.check_inside(path)?;
        if new_name.trim().is_empty() {
            return Err(err("the name cannot be empty"));
        }
        let name = safe_name(new_name);
        let target = self.projects_dir().join(format!("{name}.{EXT}"));
        if target == path {
            return Ok(target);
        }
        if target.exists() {
            return Err(err(format!("a project named \u{201c}{name}\u{201d} already exists")));
        }
        fs::rename(path, &target).map_err(|e| err(format!("rename: {e}")))?;
        self.move_sidecars(&file_name(path), &file_name(&target));
        Ok(target)
    }

    /// Copies a project to "<name> copy" (or "copy 2", ...). Returns the new path.
    pub fn duplicate(&self, path: &Path) -> Result<PathBuf> {
        self.check_inside(path)?;
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let stem: String = stem.chars().take(50).collect();
        let dir = self.projects_dir();
        let target = (1..10_000)
            .map(|n| if n == 1 { format!("{stem} copy") } else { format!("{stem} copy {n}") })
            .map(|s| dir.join(format!("{s}.{EXT}")))
            .find(|p| !p.exists())
            .ok_or_else(|| err("too many copies"))?;
        fs::copy(path, &target).map_err(|e| err(format!("duplicate: {e}")))?;
        Ok(target)
    }

    /// Deletes a project file and its thumbnail.
    pub fn delete(&self, path: &Path) -> Result<()> {
        self.check_inside(path)?;
        fs::remove_file(path).map_err(|e| err(format!("delete: {e}")))?;
        self.remove_sidecars(&file_name(path));
        Ok(())
    }

    /// Removes thumbnails whose project no longer exists (deleted outside the app).
    pub fn prune(&self, entries: &[Entry]) {
        let keep: Vec<String> = entries.iter().map(|e| file_name(&e.path)).chain(std::iter::once(DEMO_KEY.to_string())).collect();
        for e in fs::read_dir(self.thumbs_dir()).into_iter().flatten().filter_map(std::result::Result::ok) {
            let p = e.path();
            let Some(stem) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) else { continue };
            // `Name.ecproj.png` has the stem `Name.ecproj`, which is the key.
            if !keep.contains(&stem) {
                let _ = fs::remove_file(p);
            }
        }
    }
}

/// The first line of an error, short enough for a card.
fn short(s: &str) -> String {
    let line = s.lines().next().unwrap_or_default();
    line.chars().take(80).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(tag: &str) -> Library {
        let root = std::env::temp_dir().join(format!("effectia-gallery-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::create_dir_all(root.join("projects"));
        Library::new(root)
    }

    /// Saves the demo as `name` in the library and returns its path.
    fn put_demo(l: &Library, name: &str) -> PathBuf {
        let mut e = Editor::new(effectcraft_host::session());
        assert!(e.open_demo().is_ok());
        let p = l.projects_dir().join(format!("{name}.ecproj"));
        assert!(e.save(Some(&p.to_string_lossy())).is_ok());
        p
    }

    fn entry(name: &str, modified: u64, comp: &str) -> Entry {
        Entry {
            path: PathBuf::from(format!("/x/{name}.ecproj")),
            name: name.into(),
            modified,
            bytes: 1,
            info: Some(Info { comp: comp.into(), width: 1920, height: 1080, duration: 10.0, frame_rate: 30.0, layers: 3 }),
            thumb: None,
            error: None,
        }
    }

    #[test]
    fn names_are_made_safe_and_never_empty() {
        assert_eq!(safe_name("My: cool/project?"), "My_ cool_project_");
        assert_eq!(safe_name("   "), "Untitled");
        assert_eq!(safe_name("..."), "Untitled");
        assert_eq!(safe_name(&"a".repeat(200)).chars().count(), 60);
    }

    #[test]
    fn durations_and_ages_read_naturally() {
        assert_eq!(duration_label(10.0), "10 s");
        assert_eq!(duration_label(65.0), "1:05");
        assert_eq!(duration_label(3723.0), "1:02:03");
        assert_eq!(duration_label(f64::NAN), "0 s");
        assert_eq!(age_label(1000, 990), "Just now");
        assert_eq!(age_label(1000, 100), "15 min ago");
        assert_eq!(age_label(100_000, 100_000 - 7200), "2 h ago");
        assert_eq!(age_label(1_000_000, 1_000_000 - 100_000), "Yesterday");
        assert_eq!(age_label(1_000_000, 1_000_000 - 400_000), "4 days ago");
        assert_eq!(age_label(100_000_000, 100_000_000 - 5_200_000), "2 months ago");
        assert_eq!(age_label(10, 500), "Just now", "a time in the future is not negative");
    }

    #[test]
    fn search_and_sort_order_the_cards() {
        let es = vec![entry("Beta", 10, "Intro"), entry("alpha", 30, "Outro"), entry("Gamma", 20, "Beta scene")];
        let names = |ix: Vec<usize>| ix.into_iter().map(|i| es[i].name.clone()).collect::<Vec<_>>();
        assert_eq!(names(order(&es, "", Sort::Recent)), ["alpha", "Gamma", "Beta"]);
        assert_eq!(names(order(&es, "", Sort::Name)), ["alpha", "Beta", "Gamma"]);
        assert_eq!(names(order(&es, "  BETA ", Sort::Name)), ["Beta", "Gamma"], "name or composition name, any case");
        assert!(order(&es, "zzz", Sort::Recent).is_empty());
    }

    #[test]
    fn an_empty_library_scans_to_nothing_and_a_missing_folder_is_fine() {
        let l = lib("empty");
        assert!(l.scan().is_empty());
        let missing = Library::new(std::env::temp_dir().join("effectia-gallery-no-such-folder"));
        assert!(missing.scan().is_empty());
    }

    #[test]
    fn a_thumbnail_is_drawn_once_cached_and_redrawn_after_a_save() {
        let l = lib("thumb");
        let p = put_demo(&l, "Title card");
        let before = l.scan();
        assert_eq!(before.len(), 1);
        assert!(before[0].needs_thumbnail() && before[0].thumb.is_none());
        assert!(l.stale(&before).iter().any(|s| matches!(s, Source::Project(x) if *x == p)));

        l.make_thumbnail(&Source::Project(p.clone())).unwrap();
        let after = l.scan();
        let e = &after[0];
        assert!(!e.needs_thumbnail());
        let info = e.info.clone().unwrap();
        assert_eq!((info.width, info.height, info.layers), (1920, 1080, 8));
        assert_eq!(info.comp, "Effectia Intro");
        let png = e.thumb.clone().unwrap();
        let img = image::open(&png).unwrap();
        assert_eq!((img.width(), img.height()), (320, 180));
        assert!(img.to_rgba8().pixels().any(|px| px.0 != img.to_rgba8().get_pixel(0, 0).0), "the picture is not one flat colour");
        assert_eq!(e.facts(), "1920\u{00d7}1080 \u{00b7} 10 s");
        assert_eq!(e.layers_label().as_deref(), Some("8 layers"));
        assert!(l.stale(&after).iter().all(|s| matches!(s, Source::Demo)), "no project is left to draw");

        // Saving changes the file, so the cache no longer matches it.
        let mut ed = Editor::new(effectcraft_host::session());
        ed.open(&p.to_string_lossy()).unwrap();
        ed.set_scalar(362, "transform/opacity", 10.0).unwrap();
        ed.save(None).unwrap();
        let stale = l.scan();
        assert!(stale[0].needs_thumbnail() && stale[0].thumb.is_none(), "an edited project is stale");
    }

    #[test]
    fn the_demo_gets_a_thumbnail_too() {
        let l = lib("demo");
        assert!(l.demo().needs_thumbnail());
        l.make_thumbnail(&Source::Demo).unwrap();
        let d = l.demo();
        assert!(d.is_demo() && d.thumb.is_some() && d.info.is_some_and(|i| i.layers == 8));
    }

    #[test]
    fn a_broken_project_is_shown_with_its_error_and_not_retried() {
        let l = lib("broken");
        let p = l.projects_dir().join("Broken.ecproj");
        fs::write(&p, "this is not a project").unwrap();
        l.make_thumbnail(&Source::Project(p)).unwrap();
        let es = l.scan();
        assert!(es[0].error.is_some() && es[0].info.is_none());
        assert!(es[0].facts().starts_with("Can't be opened"));
        assert!(!es[0].needs_thumbnail());
        assert!(l.stale(&es).iter().all(|s| matches!(s, Source::Demo)), "the broken file is not drawn again");
    }

    #[test]
    fn rename_moves_the_thumbnail_and_refuses_clashes() {
        let l = lib("rename");
        let a = put_demo(&l, "One");
        let b = put_demo(&l, "Two");
        l.make_thumbnail(&Source::Project(a.clone())).unwrap();
        let moved = l.rename(&a, "Renamed: take 2").unwrap();
        assert_eq!(moved.file_name().unwrap().to_string_lossy(), "Renamed_ take 2.ecproj");
        assert!(!a.exists() && moved.exists());
        let e = l.scan().into_iter().find(|e| e.path == moved).unwrap();
        assert!(e.thumb.is_some(), "the picture followed the file");
        assert!(l.rename(&moved, "Two").unwrap_err().0.contains("already exists"));
        assert!(l.rename(&moved, "   ").is_err());
        assert_eq!(l.rename(&b, "Two").unwrap(), b, "keeping the name is a no-op");
        assert!(l.rename(Path::new("/etc/passwd"), "x").is_err(), "only projects in the folder");
    }

    #[test]
    fn duplicate_makes_numbered_copies_and_delete_removes_everything() {
        let l = lib("dup");
        let a = put_demo(&l, "Reel");
        l.make_thumbnail(&Source::Project(a.clone())).unwrap();
        let c1 = l.duplicate(&a).unwrap();
        let c2 = l.duplicate(&a).unwrap();
        assert_eq!(c1.file_name().unwrap().to_string_lossy(), "Reel copy.ecproj");
        assert_eq!(c2.file_name().unwrap().to_string_lossy(), "Reel copy 2.ecproj");
        assert_eq!(fs::read(&a).unwrap(), fs::read(&c1).unwrap());
        assert_eq!(l.scan().len(), 3);

        let (png, json_path) = l.sidecars("Reel.ecproj");
        assert!(png.exists() && json_path.exists());
        l.delete(&a).unwrap();
        assert!(!a.exists() && !png.exists() && !json_path.exists());
        assert!(l.delete(&a).is_err(), "already gone");
        assert_eq!(l.scan().len(), 2);
    }

    #[test]
    fn prune_drops_pictures_of_projects_deleted_elsewhere() {
        let l = lib("prune");
        let a = put_demo(&l, "Gone");
        let b = put_demo(&l, "Kept");
        l.make_thumbnail(&Source::Project(a.clone())).unwrap();
        l.make_thumbnail(&Source::Project(b)).unwrap();
        l.make_thumbnail(&Source::Demo).unwrap();
        fs::remove_file(&a).unwrap();
        l.prune(&l.scan());
        assert!(!l.sidecars("Gone.ecproj").0.exists());
        assert!(l.sidecars("Kept.ecproj").0.exists() && l.sidecars(DEMO_KEY).0.exists());
    }
}
