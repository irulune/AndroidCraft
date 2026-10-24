//! What the touch UI needs from the device that the engine does not provide: permission to read
//! photos, videos and files, and the folders to start browsing from.
//!
//! The UI crate stays free of any Android types. The Android entry crate implements [`Platform`]
//! over JNI (`checkSelfPermission`, `requestPermissions`, the all-files-access settings page);
//! everywhere else [`Desktop`] answers "granted" and points at ordinary folders, which is what the
//! tests and the screenshot tool use.

use std::path::PathBuf;

/// Whether the app may read a class of files right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Granted,
    NotGranted,
}

/// What a folder needs before its files can be listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Needs {
    /// The app's own folders: always readable.
    Nothing,
    /// Photos and videos (`READ_MEDIA_IMAGES` / `READ_MEDIA_VIDEO`, or `READ_EXTERNAL_STORAGE`).
    Media,
    /// "All files access": any file on shared storage, not only photos and videos.
    AllFiles,
}

/// A folder the picker offers to start from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub label: String,
    pub path: PathBuf,
    pub needs: Needs,
}

impl Root {
    pub fn new(label: impl Into<String>, path: impl Into<PathBuf>, needs: Needs) -> Root {
        Root { label: label.into(), path: path.into(), needs }
    }
}

/// The device services behind the media picker.
pub trait Platform {
    /// Permission to read photos and videos from shared storage.
    fn media_access(&self) -> Access;
    /// Asks the system for that permission. The answer arrives later: poll [`Platform::media_access`].
    fn request_media_access(&self);
    /// "All files access", which lets the picker browse every folder.
    fn all_files_access(&self) -> Access;
    /// Opens the system page where the user can grant all-files access.
    fn request_all_files_access(&self);
    /// Opens this app's page in the system settings (the way back after a permission was refused).
    fn open_app_settings(&self);
    /// The shared folders worth offering, whether or not they can be read yet.
    fn roots(&self) -> Vec<Root>;
}

/// A desktop-like platform: everything is readable, folders are the ones given.
#[derive(Clone, Debug, Default)]
pub struct Desktop {
    roots: Vec<Root>,
}

impl Desktop {
    pub fn with_roots(roots: Vec<Root>) -> Desktop {
        Desktop { roots }
    }

    /// The user's usual folders, when they exist.
    pub fn home() -> Desktop {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let roots = home
            .map(|h| {
                ["Pictures", "Videos", "Movies", "Downloads", "Music"]
                    .into_iter()
                    .map(|n| Root::new(n, h.join(n), Needs::Nothing))
                    .filter(|r| r.path.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        Desktop { roots }
    }
}

impl Platform for Desktop {
    fn media_access(&self) -> Access {
        Access::Granted
    }
    fn request_media_access(&self) {}
    fn all_files_access(&self) -> Access {
        Access::Granted
    }
    fn request_all_files_access(&self) {}
    fn open_app_settings(&self) {}
    fn roots(&self) -> Vec<Root> {
        self.roots.clone()
    }
}
