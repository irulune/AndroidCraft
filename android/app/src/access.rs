//! Android permissions and system pages, reached through JNI.
//!
//! The UI crate only knows the [`Platform`] trait; this is its Android implementation.
//! Everything goes through the activity object that `android-activity` hands us:
//!
//! * `checkSelfPermission` says whether photos and videos (`READ_MEDIA_*` on Android 13+,
//!   `READ_EXTERNAL_STORAGE` before) may be read, and `Environment.isExternalStorageManager()`
//!   whether "all files access" was granted (Android 11+).
//! * `requestPermissions` shows the system permission dialog. Its answer arrives in
//!   `onRequestPermissionsResult`, which a NativeActivity app does not receive, so the UI polls
//!   `checkSelfPermission` instead.
//! * All-files access and the app's own settings page are system screens opened with an intent.
//!
//! Any JNI failure (missing method, pending Java exception) is cleared and reported as "not
//! granted" or "nothing happened"; it never takes the app down.

use std::ffi::c_void;
use std::path::PathBuf;

use effectia_ui_mobile::platform::{Access, Needs, Platform, Root};
use jni::errors::Result as JniResult;
use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

const READ_MEDIA_IMAGES: &str = "android.permission.READ_MEDIA_IMAGES";
const READ_MEDIA_VIDEO: &str = "android.permission.READ_MEDIA_VIDEO";
const READ_MEDIA_AUDIO: &str = "android.permission.READ_MEDIA_AUDIO";
const READ_EXTERNAL_STORAGE: &str = "android.permission.READ_EXTERNAL_STORAGE";

pub struct AndroidAccess {
    vm: *mut c_void,
    activity: *mut c_void,
    roots: Vec<Root>,
}

impl AndroidAccess {
    /// `vm` and `activity` are the JavaVM and Activity pointers from `AndroidApp`; both stay
    /// valid for the life of the process.
    pub fn new(vm: *mut c_void, activity: *mut c_void) -> AndroidAccess {
        let mut a = AndroidAccess { vm, activity, roots: Vec::new() };
        let base = a.storage_root().unwrap_or_else(|| PathBuf::from("/storage/emulated/0"));
        // The standard folders are listed whether or not they can be read yet: before the
        // permission is granted they may look empty, and the picker must still know that photos
        // and videos are what it is waiting for.
        a.roots = vec![
            Root::new("Camera", base.join("DCIM/Camera"), Needs::Media),
            Root::new("Pictures", base.join("Pictures"), Needs::Media),
            Root::new("Movies", base.join("Movies"), Needs::Media),
            Root::new("Download", base.join("Download"), Needs::Media),
            Root::new("Music", base.join("Music"), Needs::Media),
            Root::new("All files", base, Needs::AllFiles),
        ];
        a
    }

    /// Runs `f` with a JNI environment attached to this thread and the activity.
    fn with<R>(&self, f: impl FnOnce(&mut JNIEnv, &JObject) -> JniResult<R>) -> Option<R> {
        if self.vm.is_null() || self.activity.is_null() {
            return None;
        }
        // SAFETY: both pointers come from `android-activity` and outlive this struct; the
        // activity reference is only borrowed for the duration of the call and never deleted.
        let vm = unsafe { JavaVM::from_raw(self.vm.cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let activity = unsafe { JObject::from_raw(self.activity.cast()) };
        let r = f(&mut env, &activity);
        if r.is_err() {
            // A Java exception left pending would crash the next JNI call.
            let _ = env.exception_clear();
        }
        r.ok()
    }

    fn storage_root(&self) -> Option<PathBuf> {
        self.with(|env, _| {
            let dir = env.call_static_method("android/os/Environment", "getExternalStorageDirectory", "()Ljava/io/File;", &[])?.l()?;
            let path = env.call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?.l()?;
            let s: String = env.get_string(&JString::from(path))?.into();
            Ok(PathBuf::from(s))
        })
    }

    fn sdk_int(env: &mut JNIEnv) -> JniResult<i32> {
        env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()
    }

    fn granted(env: &mut JNIEnv, activity: &JObject, permission: &str) -> JniResult<bool> {
        let name = env.new_string(permission)?;
        let r = env.call_method(activity, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&name)])?.i()?;
        Ok(r == 0)
    }

    fn media_permissions(sdk: i32) -> &'static [&'static str] {
        if sdk >= 33 { &[READ_MEDIA_IMAGES, READ_MEDIA_VIDEO, READ_MEDIA_AUDIO] } else { &[READ_EXTERNAL_STORAGE] }
    }

    /// Opens a system settings screen. `with_package` aims it at this app (`package:<name>`).
    fn open_settings(&self, action: &str, with_package: bool) -> bool {
        self.with(|env, activity| {
            let action = env.new_string(action)?;
            let intent = if with_package {
                let pkg = env.call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])?.l()?;
                let pkg: String = env.get_string(&JString::from(pkg))?.into();
                let text = env.new_string(format!("package:{pkg}"))?;
                let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&text)])?.l()?;
                env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&action), JValue::Object(&uri)])?
            } else {
                env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?
            };
            env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
            Ok(())
        })
        .is_some()
    }
}

impl Platform for AndroidAccess {
    fn media_access(&self) -> Access {
        let ok = self
            .with(|env, activity| {
                let sdk = Self::sdk_int(env)?;
                for p in Self::media_permissions(sdk) {
                    // Photos or videos is enough to list them; audio alone does not count.
                    if *p != READ_MEDIA_AUDIO && Self::granted(env, activity, p)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .unwrap_or(false);
        if ok { Access::Granted } else { Access::NotGranted }
    }

    fn request_media_access(&self) {
        let _ = self.with(|env, activity| {
            let sdk = Self::sdk_int(env)?;
            let perms = Self::media_permissions(sdk);
            let string = env.find_class("java/lang/String")?;
            let arr = env.new_object_array(perms.len() as i32, &string, JObject::null())?;
            for (i, p) in perms.iter().enumerate() {
                let s = env.new_string(*p)?;
                env.set_object_array_element(&arr, i as i32, &s)?;
            }
            env.call_method(activity, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&arr), JValue::Int(4201)])?;
            Ok(())
        });
    }

    fn all_files_access(&self) -> Access {
        // Android 11+ has a separate "all files access"; before that the storage permission
        // already covers every file.
        let sdk = self.with(|env, _| Self::sdk_int(env)).unwrap_or(0);
        let ok = if sdk >= 30 {
            self.with(|env, _| env.call_static_method("android/os/Environment", "isExternalStorageManager", "()Z", &[])?.z()).unwrap_or(false)
        } else {
            self.media_access() == Access::Granted
        };
        if ok { Access::Granted } else { Access::NotGranted }
    }

    fn request_all_files_access(&self) {
        let sdk = self.with(|env, _| Self::sdk_int(env)).unwrap_or(0);
        if sdk >= 30 {
            if !self.open_settings("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION", true) {
                // Some builds only have the list of apps.
                self.open_settings("android.settings.MANAGE_ALL_FILES_ACCESS_PERMISSION", false);
            }
        } else {
            self.request_media_access();
        }
    }

    fn open_app_settings(&self) {
        self.open_settings("android.settings.APPLICATION_DETAILS_SETTINGS", true);
    }

    fn roots(&self) -> Vec<Root> {
        self.roots.clone()
    }
}
