//! Browsing the device for media to add: folder listings, a "recent" scan, and cached thumbnails.
//!
//! Plain files and engine sessions only, so it is testable without a window or a phone. Photos are
//! decoded with the `image` crate (honouring their EXIF rotation); videos get a frame from a
//! scratch engine session; audio has no picture, only a length read the same way. Thumbnails are cached under one folder, keyed by
//! the file's path, size and modified time, so an edited or replaced file is drawn again.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Value, json};

use crate::editor::Editor;

/// Edge of a stored thumbnail, in pixels.
pub const THUMB_PX: u32 = 224;

const IMAGE_EXT: [&str; 11] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "svg", "psd", "exr"];
const PHOTO_EXT: [&str; 6] = ["png", "jpg", "jpeg", "webp", "gif", "bmp"];
const VIDEO_EXT: [&str; 6] = ["mp4", "mov", "m4v", "webm", "mkv", "avi"];
const AUDIO_EXT: [&str; 6] = ["mp3", "wav", "ogg", "flac", "aac", "m4a"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
    Audio,
}

fn ext_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}

impl Kind {
    /// The kind the engine can import for this file name, or `None` for anything else.
    pub fn of(path: &Path) -> Option<Kind> {
        let e = ext_of(path);
        let e = e.as_str();
        if IMAGE_EXT.contains(&e) {
            Some(Kind::Image)
        } else if VIDEO_EXT.contains(&e) {
            Some(Kind::Video)
        } else if AUDIO_EXT.contains(&e) {
            Some(Kind::Audio)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub path: PathBuf,
    pub name: String,
    pub kind: Kind,
    pub bytes: u64,
    /// Seconds since the Unix epoch.
    pub modified: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub path: PathBuf,
    pub name: String,
}

/// The contents of one folder: its sub-folders and the files the engine can import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub dir: PathBuf,
    pub folders: Vec<Folder>,
    pub files: Vec<File>,
}

fn file_of(path: &Path, meta: &fs::Metadata) -> Option<File> {
    let kind = Kind::of(path)?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    Some(File { path: path.to_path_buf(), name: path.file_name()?.to_string_lossy().into_owned(), kind, bytes: meta.len(), modified })
}

/// Lists a folder: folders A to Z (hidden ones left out), then importable files, newest first.
pub fn list(dir: &Path) -> Result<Listing, String> {
    let rd = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut folders, mut files) = (Vec::new(), Vec::new());
    for entry in rd.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else { continue };
        if meta.is_dir() {
            folders.push(Folder { path, name });
        } else if meta.is_file()
            && let Some(f) = file_of(&path, &meta)
        {
            files.push(f);
        }
    }
    folders.sort_by_key(|f| f.name.to_lowercase());
    files.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing { dir: dir.to_path_buf(), folders, files })
}

/// The newest photos, videos and audio under `roots`, looking at most four folders deep and
/// skipping hidden folders and other apps' `Android/` data. Bounded: it gives up after 20 000
/// entries. Up to `limit` photos and videos are kept, plus up to `limit / 2` audio files, so a big
/// music library cannot push the photos out of the list.
pub fn recent(roots: &[PathBuf], limit: usize) -> Vec<File> {
    let mut out: Vec<File> = Vec::new();
    let mut seen = 0usize;
    let mut queue: VecDeque<(PathBuf, u8)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.filter_map(Result::ok) {
            seen += 1;
            if seen > 20_000 {
                return finish(out, limit);
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(meta) = fs::metadata(&path) else { continue };
            if meta.is_dir() {
                if depth < 4 && name != "Android" {
                    queue.push_back((path, depth + 1));
                }
            } else if meta.is_file() {
                let e = ext_of(&path);
                let wanted = PHOTO_EXT.contains(&e.as_str()) || VIDEO_EXT.contains(&e.as_str()) || AUDIO_EXT.contains(&e.as_str());
                if wanted && let Some(f) = file_of(&path, &meta) {
                    out.push(f);
                }
            }
        }
    }
    finish(out, limit)
}

fn finish(mut files: Vec<File>, limit: usize) -> Vec<File> {
    files.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.path.cmp(&b.path)));
    files.dedup_by(|a, b| a.path == b.path);
    let (mut audio, mut seen_visual) = (0usize, 0usize);
    files.retain(|f| {
        let (count, cap) = if f.kind == Kind::Audio { (&mut audio, limit / 2) } else { (&mut seen_visual, limit) };
        *count += 1;
        *count <= cap
    });
    files
}

/// "812 KB", "4.2 MB", "1.3 GB".
pub fn size_label(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.1} GB", b / (KB * KB * KB))
    }
}

/// A finished thumbnail.
#[derive(Clone, Debug, PartialEq)]
pub struct Thumb {
    /// The picture; audio has none.
    pub png: Option<PathBuf>,
    /// Length in seconds, for videos and audio.
    pub duration: Option<f64>,
}

/// The thumbnail cache folder.
#[derive(Clone, Debug)]
pub struct Thumbs {
    dir: PathBuf,
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

impl Thumbs {
    pub fn new(dir: impl Into<PathBuf>) -> Thumbs {
        Thumbs { dir: dir.into() }
    }

    fn key(f: &File) -> String {
        let id = format!("{}|{}|{}", f.path.display(), f.bytes, f.modified);
        format!("{:016x}", fnv(id.as_bytes()))
    }

    fn paths(&self, f: &File) -> (PathBuf, PathBuf) {
        let k = Self::key(f);
        (self.dir.join(format!("{k}.png")), self.dir.join(format!("{k}.json")))
    }

    /// The cached thumbnail, if it has been made.
    pub fn lookup(&self, f: &File) -> Option<Thumb> {
        let (png, meta) = self.paths(f);
        let duration =
            fs::read_to_string(meta).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v.get("duration").and_then(Value::as_f64));
        if f.kind == Kind::Audio {
            return duration.map(|d| Thumb { png: None, duration: Some(d) });
        }
        png.is_file().then_some(Thumb { png: Some(png), duration })
    }

    /// Makes the thumbnail (for audio, reads the length). `scratch` is a lazily created engine
    /// session reused across videos and audio.
    pub fn make(&self, f: &File, scratch: &mut Option<Editor>) -> Result<Thumb, String> {
        if let Some(t) = self.lookup(f) {
            return Ok(t);
        }
        fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let (png, meta) = self.paths(f);
        let (img, duration) = match f.kind {
            Kind::Image => (decode_image(&f.path)?, None),
            Kind::Video => {
                let ed = scratch.get_or_insert_with(|| Editor::new(effectcraft_host::session()));
                let (img, dur) = video_frame(ed, &f.path)?;
                (img, Some(dur))
            }
            Kind::Audio => {
                let ed = scratch.get_or_insert_with(|| Editor::new(effectcraft_host::session()));
                let dur = audio_length(ed, &f.path)?;
                fs::write(&meta, json!({ "duration": dur }).to_string()).map_err(|e| format!("audio length: {e}"))?;
                return Ok(Thumb { png: None, duration: Some(dur) });
            }
        };
        let tmp = png.with_extension("png.tmp");
        img.save_with_format(&tmp, image::ImageFormat::Png).map_err(|e| format!("thumbnail: {e}"))?;
        fs::rename(&tmp, &png).map_err(|e| format!("thumbnail: {e}"))?;
        if let Some(d) = duration {
            let _ = fs::write(&meta, json!({ "duration": d }).to_string());
        }
        Ok(Thumb { png: Some(png), duration })
    }

    /// Keeps the cache from growing for ever: past 1500 files the oldest are removed.
    pub fn prune(&self) {
        let Ok(rd) = fs::read_dir(&self.dir) else { return };
        let mut files: Vec<(std::time::SystemTime, PathBuf)> =
            rd.filter_map(Result::ok).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
        if files.len() <= 1500 {
            return;
        }
        files.sort();
        for (_, p) in files.iter().take(files.len() - 1000) {
            let _ = fs::remove_file(p);
        }
    }
}

fn decode_image(path: &Path) -> Result<image::RgbaImage, String> {
    let reader = image::ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = image::ImageDecoder::orientation(&mut decoder).unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    img.apply_orientation(orientation);
    Ok(img.thumbnail(THUMB_PX, THUMB_PX).to_rgba8())
}

/// The length of an audio file in seconds, from a scratch project.
fn audio_length(ed: &mut Editor, path: &Path) -> Result<f64, String> {
    let p = path.to_string_lossy();
    let run = |ed: &mut Editor| -> crate::editor::Result<f64> {
        ed.new_project()?;
        ed.command("file.import", json!({ "paths": [p] }))?;
        let summary = ed.command("project.summary", json!({}))?;
        summary
            .get("items")
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find(|i| i.get("type").and_then(Value::as_str) == Some("Audio")))
            .and_then(|i| i.get("duration").and_then(Value::as_f64))
            .ok_or_else(|| crate::editor::EditorError("not audio the engine can open".into()))
    };
    run(ed).map_err(|e| e.0)
}

/// One frame of a video, from a scratch project sized like the video, and the video's length.
fn video_frame(ed: &mut Editor, path: &Path) -> Result<(image::RgbaImage, f64), String> {
    let p = path.to_string_lossy();
    let run = |ed: &mut Editor| -> crate::editor::Result<(image::RgbaImage, f64)> {
        ed.new_project()?;
        ed.command("file.import", json!({ "paths": [p] }))?;
        let summary = ed.command("project.summary", json!({}))?;
        let item = summary
            .get("items")
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find(|i| i.get("type").and_then(Value::as_str) == Some("Video")))
            .cloned()
            .ok_or_else(|| crate::editor::EditorError("not a video the engine can open".into()))?;
        let id = item.get("id").and_then(Value::as_u64).unwrap_or(0);
        let dur = item.get("duration").and_then(Value::as_f64).unwrap_or(1.0).max(0.1);
        let size = item.get("size").and_then(Value::as_array);
        let dim = |i: usize| size.and_then(|s| s.get(i)).and_then(Value::as_u64).unwrap_or(0).clamp(16, 8192) as u32;
        let (w, h) = (dim(0), dim(1));
        ed.new_comp("Preview", w, h, 24.0, dur)?;
        ed.command("layer.addItem", json!({ "item": id }))?;
        ed.set_time((dur * 0.1).min(1.0).min((dur - 0.05).max(0.0)))?;
        let frame = ed.render(THUMB_PX)?;
        let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba).ok_or_else(|| crate::editor::EditorError("bad frame".into()))?;
        Ok((img, dur))
    };
    run(ed).map_err(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("effectia-browser-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn png(path: &Path, w: u32, h: u32, rgb: [u8; 3]) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        image::RgbImage::from_pixel(w, h, image::Rgb(rgb)).save(path).unwrap();
    }

    fn set_age(path: &Path, secs_ago: u64) {
        let t = std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago);
        fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    /// A mono 8 kHz sine tone, `secs` long.
    fn wav(path: &Path, secs: f32) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        let n = (8000.0 * secs) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n * 2).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&8000u32.to_le_bytes());
        b.extend_from_slice(&16000u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n * 2).to_le_bytes());
        for i in 0..n {
            let v = ((i as f32 * 0.2).sin() * 8000.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
        }
        fs::write(path, b).unwrap();
    }

    fn clip() -> PathBuf {
        use crate::ops::{NewLayer, RenderFormat};
        let dir = std::env::temp_dir().join(format!("effectia-browser-clip-{}", std::process::id()));
        let path = dir.join("clip.mp4");
        if !path.exists() {
            fs::create_dir_all(&dir).unwrap();
            let mut e = Editor::new(effectcraft_host::session());
            e.new_project().unwrap();
            e.new_comp("Clip", 320, 180, 24.0, 1.0).unwrap();
            e.add_layer(NewLayer::Star).unwrap();
            e.render_to_file(path.to_str().unwrap(), RenderFormat::Mp4, false).unwrap();
        }
        path
    }

    #[test]
    fn kinds_come_from_the_extension() {
        assert_eq!(Kind::of(Path::new("a/B.JPG")), Some(Kind::Image));
        assert_eq!(Kind::of(Path::new("x.mp4")), Some(Kind::Video));
        assert_eq!(Kind::of(Path::new("x.M4A")), Some(Kind::Audio));
        assert_eq!(Kind::of(Path::new("notes.txt")), None);
        assert_eq!(Kind::of(Path::new("noext")), None);
    }

    #[test]
    fn a_folder_lists_folders_first_then_importable_files_newest_first() {
        let d = tmp("list");
        png(&d.join("old.png"), 4, 4, [1, 2, 3]);
        png(&d.join("new.png"), 4, 4, [1, 2, 3]);
        png(&d.join(".hidden.png"), 4, 4, [1, 2, 3]);
        fs::write(d.join("notes.txt"), "x").unwrap();
        fs::create_dir_all(d.join("Zeta")).unwrap();
        fs::create_dir_all(d.join("alpha")).unwrap();
        fs::create_dir_all(d.join(".cache")).unwrap();
        set_age(&d.join("old.png"), 1000);
        set_age(&d.join("new.png"), 10);
        let l = list(&d).unwrap();
        assert_eq!(l.folders.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["alpha", "Zeta"]);
        assert_eq!(l.files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["new.png", "old.png"]);
        assert!(l.files.iter().all(|f| f.kind == Kind::Image && f.bytes > 0));
        assert!(list(&d.join("missing")).is_err(), "a folder that cannot be read is an error, not a panic");
    }

    #[test]
    fn recent_finds_the_newest_photos_videos_and_audio_in_nested_folders() {
        let d = tmp("recent");
        png(&d.join("DCIM/Camera/a.jpg.png"), 4, 4, [9, 9, 9]);
        png(&d.join("Pictures/Screenshots/b.png"), 4, 4, [9, 9, 9]);
        png(&d.join("Pictures/deep/1/2/3/4/5/too-deep.png"), 4, 4, [9, 9, 9]);
        png(&d.join("Android/data/other-app/secret.png"), 4, 4, [9, 9, 9]);
        png(&d.join(".thumbnails/t.png"), 4, 4, [9, 9, 9]);
        fs::write(d.join("DCIM/Camera/c.mp4"), "not really a video").unwrap();
        fs::write(d.join("DCIM/Camera/song.mp3"), "x").unwrap();
        set_age(&d.join("DCIM/Camera/a.jpg.png"), 500);
        set_age(&d.join("Pictures/Screenshots/b.png"), 5);
        set_age(&d.join("DCIM/Camera/c.mp4"), 50);
        set_age(&d.join("DCIM/Camera/song.mp3"), 20);
        let r = recent(std::slice::from_ref(&d), 10);
        let names: Vec<&str> = r.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["b.png", "song.mp3", "c.mp4", "a.jpg.png"], "newest first, audio included; hidden, deep and other apps' data left out");
        let two = recent(std::slice::from_ref(&d), 2);
        assert_eq!(two.iter().filter(|f| f.kind != Kind::Audio).count(), 2, "the limit applies to photos and videos");
        assert_eq!(two.iter().filter(|f| f.kind == Kind::Audio).count(), 1, "and to audio at half that");
        assert!(recent(&[d.join("nope")], 5).is_empty());

        // A big music library cannot push the photos out.
        for i in 0..30 {
            let p = d.join(format!("Music/t{i}.mp3"));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, "x").unwrap();
            set_age(&p, 1);
        }
        let r = recent(std::slice::from_ref(&d), 10);
        assert_eq!(r.iter().filter(|f| f.kind == Kind::Audio).count(), 5, "audio gets half the limit");
        assert_eq!(r.iter().filter(|f| f.kind != Kind::Audio).count(), 3, "all the photos and videos are still there");
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size_label(512), "512 B");
        assert_eq!(size_label(2048), "2 KB");
        assert_eq!(size_label(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(size_label(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn a_photo_thumbnail_is_scaled_cached_and_redrawn_when_the_file_changes() {
        let d = tmp("thumb-photo");
        let p = d.join("wide.png");
        png(&p, 800, 400, [200, 30, 30]);
        let cache = Thumbs::new(d.join("cache"));
        let f = list(&d).unwrap().files.remove(0);
        assert!(cache.lookup(&f).is_none());
        let mut scratch = None;
        let t = cache.make(&f, &mut scratch).unwrap();
        let img = image::open(t.png.as_ref().unwrap()).unwrap();
        assert_eq!((img.width(), img.height()), (224, 112), "fits in 224 and keeps its shape");
        assert!(t.duration.is_none() && scratch.is_none(), "photos never start an engine session");
        assert_eq!(cache.lookup(&f), Some(t.clone()));

        // A replaced file has a new size and time, so its old picture no longer applies.
        png(&p, 100, 100, [30, 30, 200]);
        set_age(&p, 5);
        let f2 = list(&d).unwrap().files.remove(0);
        assert_ne!(f, f2);
        assert!(cache.lookup(&f2).is_none());
    }

    #[test]
    fn a_rotated_photo_is_turned_upright() {
        // A 40x20 JPEG tagged as rotated a quarter turn should come out portrait.
        let d = tmp("thumb-exif");
        let p = d.join("rot.jpg");
        let mut buf = Vec::new();
        image::RgbImage::from_pixel(40, 20, image::Rgb([10, 200, 10])).write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Jpeg).unwrap();
        // Splice in an EXIF APP1 segment with Orientation = 6 (rotate 90 clockwise).
        let exif: [u8; 36] =
            [0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, b'M', b'M', 0, 0x2A, 0, 0, 0, 8, 0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, 0, 0, 0, 0];
        let mut with = buf[..2].to_vec();
        with.extend_from_slice(&exif);
        with.extend_from_slice(&buf[2..]);
        fs::write(&p, with).unwrap();
        let img = decode_image(&p).unwrap();
        assert!(img.height() > img.width(), "{}x{}", img.width(), img.height());
    }

    #[test]
    fn a_video_thumbnail_comes_from_the_engine_with_its_length() {
        let d = tmp("thumb-video");
        let cache = Thumbs::new(d.join("cache"));
        let dir = clip().parent().unwrap().to_path_buf();
        let f = list(&dir).unwrap().files.into_iter().find(|f| f.name == "clip.mp4").unwrap();
        let mut scratch = None;
        let t = cache.make(&f, &mut scratch).unwrap();
        assert!(scratch.is_some());
        let dur = t.duration.unwrap();
        assert!((dur - 1.0).abs() < 0.2, "{dur}");
        let img = image::open(t.png.as_ref().unwrap()).unwrap().to_rgba8();
        assert_eq!((img.width(), img.height()), (224, 126), "16:9 like the clip");
        assert!(img.pixels().any(|p| p.0 != img.get_pixel(0, 0).0), "a real frame, not one flat colour");
        // The second look needs no session.
        let mut none = None;
        assert_eq!(cache.make(&f, &mut none).unwrap(), t);
        assert!(none.is_none());
    }

    #[test]
    fn broken_files_are_errors_not_panics() {
        let d = tmp("thumb-bad");
        fs::write(d.join("broken.png"), "not an image").unwrap();
        fs::write(d.join("broken.mp4"), "not a video").unwrap();
        fs::write(d.join("song.mp3"), "x").unwrap();
        let cache = Thumbs::new(d.join("cache"));
        let mut scratch = None;
        for f in list(&d).unwrap().files {
            assert!(cache.make(&f, &mut scratch).is_err(), "{}", f.name);
        }
    }

    #[test]
    fn an_audio_file_has_a_length_but_no_picture_and_the_length_is_cached() {
        let d = tmp("thumb-audio");
        wav(&d.join("tone.wav"), 2.0);
        let cache = Thumbs::new(d.join("cache"));
        let f = list(&d).unwrap().files.remove(0);
        assert_eq!(f.kind, Kind::Audio);
        assert!(cache.lookup(&f).is_none());
        let mut scratch = None;
        let t = cache.make(&f, &mut scratch).unwrap();
        assert!(t.png.is_none());
        let dur = t.duration.unwrap();
        assert!((dur - 2.0).abs() < 0.1, "{dur}");
        let mut none = None;
        assert_eq!(cache.make(&f, &mut none).unwrap(), t, "the second look needs no session");
        assert!(none.is_none());
    }

    #[test]
    fn prune_keeps_the_cache_bounded() {
        let d = tmp("prune");
        let cache = Thumbs::new(d.clone());
        for i in 0..1600 {
            fs::write(d.join(format!("{i:04}.png")), "x").unwrap();
        }
        cache.prune();
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1000);
    }
}
