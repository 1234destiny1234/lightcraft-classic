//! The Android app's calls into its activity, over JNI, and the activity's answers.
//!
//! The app runs as a native library inside a small Java activity (`packaging/android`, a
//! `NativeActivity` subclass). What only Java can do goes through it: the system pickers (the
//! Storage Access Framework, which hands out `content://` documents rather than paths) and opening
//! links. The activity copies picked documents into the app's own folders, so the app only ever
//! sees file paths, and watches a saved file to copy it to the document the user chose. A folder
//! or file on the device's own storage that the app may read (with "All files access") is answered
//! with its real path instead, so nothing is copied. Copies that remain report through
//! [`CopyProgress`], so the app can show them in its activity stack and stop them.
//!
//! A picker answers asynchronously: [`Host::pick`] returns a channel at once, and the activity
//! calls back into native code (`nativePicked`, registered at start-up, so no symbol depends on
//! the package name) when the user is done. The registered waker then asks the UI for a frame.
//!
//! This is one of the crates allowed `unsafe`: the `jvm` module adopts the JVM and activity
//! handles android-activity gives the app. Its API is safe. Every JNI failure, a Java exception
//! included, comes back as an [`Error`]; nothing here panics, and the native callback never lets a
//! panic cross into Java.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::fmt;

/// What a picker chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    /// Any number of files.
    Files,
    /// One file.
    File,
    /// One folder: its files are copied in (the app gets the copy's folder).
    Folder,
    /// Where to save one file: the app gets a path to write; the activity copies the file to the
    /// chosen document once it is written.
    Save,
}

impl PickKind {
    /// The number the activity's `startPick` takes.
    pub fn code(self) -> i32 {
        match self {
            PickKind::Files => 0,
            PickKind::File => 1,
            PickKind::Folder => 2,
            PickKind::Save => 3,
        }
    }
}

/// A picker to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickRequest {
    pub kind: PickKind,
    /// MIME types the picker offers (`image/*`, …); empty = any file.
    pub mime_types: Vec<String>,
    /// [`PickKind::Save`]: the suggested file name.
    pub file_name: Option<String>,
}

impl PickRequest {
    /// MIME types for a list of file extensions (without dots). Extensions with no well-known type
    /// widen the picker to any file, so a raw or preset file is never greyed out.
    pub fn mime_types_for(extensions: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for ext in extensions {
            let Some(mime) = mime_for(ext) else { return Vec::new() };
            if !out.iter().any(|m| m == mime) {
                out.push(mime.to_string());
            }
        }
        out
    }
}

/// The MIME type Android's document providers report for an extension, where one is well known.
fn mime_for(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "heic" | "heif" => "image/heif",
        "avif" => "image/avif",
        "gif" => "image/gif",
        "json" => "application/json",
        "txt" => "text/plain",
        "zip" => "application/zip",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

/// Where the activity's copies of picked documents report to (the app's activity stack).
///
/// A copy that takes a while (a folder on an SD card, files from a cloud provider) calls
/// [`CopyProgress::progress`] every so often with its own task number, starting with the first
/// file it really copies, and [`CopyProgress::ended`] once when it is done. The copy loop stops
/// at its next check once `progress` answers `true`.
pub trait CopyProgress: Send + Sync {
    /// `done` of `total` files (`total` 0 = not known yet), `detail` the file being copied.
    /// Returns whether the copy should stop.
    fn progress(&self, task: i64, done: u64, total: u64, detail: &str) -> bool;
    /// The copy `task` is over (finished, failed or stopped).
    fn ended(&self, task: i64);
}

/// A failed call into the activity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// The running activity, as android-activity (and so winit / eframe) know it.
#[cfg(target_os = "android")]
pub use android_activity::AndroidApp;
#[cfg(target_os = "android")]
pub use imp::Host;

#[cfg(target_os = "android")]
mod imp {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Mutex, OnceLock, PoisonError};

    use android_activity::AndroidApp;
    use jni::objects::{GlobalRef, JObject, JObjectArray, JString, JValue};
    use jni::sys::jlong;
    use jni::{JNIEnv, JavaVM, NativeMethod};

    use super::{CopyProgress, Error, PickRequest};

    /// Pickers that are up, by the id the activity answers with.
    static PENDING: Mutex<Option<HashMap<i64, Sender<Vec<String>>>>> = Mutex::new(None);
    static NEXT_ID: AtomicI64 = AtomicI64::new(1);
    /// Asks the UI for a frame (set by the app: an answer must not wait for the next touch).
    static WAKER: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
    /// Where copies report (set by the app once its session exists).
    static COPIES: Mutex<Option<std::sync::Arc<dyn CopyProgress>>> = Mutex::new(None);

    fn jni_error(what: &str, e: jni::errors::Error) -> Error {
        Error(format!("{what}: {e}"))
    }

    /// The app's handle on its activity.
    pub struct Host {
        vm: JavaVM,
        activity: GlobalRef,
    }

    impl Host {
        /// Adopts the activity android-activity started the app in and registers the native
        /// callbacks on its class.
        pub fn new(app: &AndroidApp) -> Result<Host, Error> {
            let vm = super::jvm::vm(app)?;
            let activity = {
                let mut env = vm.attach_current_thread_permanently().map_err(|e| jni_error("attach to the JVM", e))?;
                let local = super::jvm::activity(app)?;
                let global = env.new_global_ref(&local).map_err(|e| jni_error("reference the activity", e))?;
                let class = env.get_object_class(&global).map_err(|e| jni_error("the activity's class", e))?;
                let methods = [
                    NativeMethod {
                        name: "nativePicked".into(),
                        sig: "(J[Ljava/lang/String;)V".into(),
                        fn_ptr: native_picked as *mut std::ffi::c_void,
                    },
                    NativeMethod {
                        name: "nativeCopyProgress".into(),
                        sig: "(JJJLjava/lang/String;)Z".into(),
                        fn_ptr: native_copy_progress as *mut std::ffi::c_void,
                    },
                    NativeMethod { name: "nativeCopyEnded".into(), sig: "(J)V".into(), fn_ptr: native_copy_ended as *mut std::ffi::c_void },
                ];
                env.register_native_methods(&class, &methods).map_err(|e| jni_error("register the native callbacks", e))?;
                global
            };
            Ok(Host { vm, activity })
        }

        /// Sets what asks the UI for a frame when an answer arrives. Only the first call counts.
        pub fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync>) {
            let _ = WAKER.set(wake);
        }

        /// Shows a picker; the channel answers with the chosen paths (none = cancelled).
        pub fn pick(&self, req: &PickRequest) -> Result<Receiver<Vec<String>>, Error> {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = channel();
            pending(|p| {
                p.insert(id, tx);
            });
            let kind = req.kind.code();
            let result = self.call("startPick", |env, activity| {
                let mimes = string_array(env, &req.mime_types)?;
                let name: JObject = match &req.file_name {
                    Some(n) => env.new_string(n)?.into(),
                    None => JObject::null(),
                };
                env.call_method(
                    activity,
                    "startPick",
                    "(JI[Ljava/lang/String;Ljava/lang/String;)V",
                    &[JValue::Long(id), JValue::Int(kind), JValue::Object(&mimes), JValue::Object(&name)],
                )?;
                Ok(())
            });
            if let Err(e) = result {
                pending(|p| {
                    p.remove(&id);
                });
                return Err(e);
            }
            Ok(rx)
        }

        /// Sets where the activity's copies of picked documents report their progress.
        pub fn set_copy_progress(&self, sink: std::sync::Arc<dyn CopyProgress>) {
            *COPIES.lock().unwrap_or_else(PoisonError::into_inner) = Some(sink);
        }

        /// Whether the app may read every file on shared storage ("All files access" on Android
        /// 11+, the storage permission before), so folders can be read in place.
        pub fn has_all_files_access(&self) -> Result<bool, Error> {
            let mut granted = false;
            self.call("hasAllFilesAccess", |env, activity| {
                granted = env.call_method(activity, "hasAllFilesAccess", "()Z", &[])?.z()?;
                Ok(())
            })?;
            Ok(granted)
        }

        /// Asks for "All files access" (the system's settings page on Android 11+, the storage
        /// permission before). The answer is read later with [`Host::has_all_files_access`].
        pub fn ask_all_files_access(&self) -> Result<(), Error> {
            self.call("askAllFilesAccess", |env, activity| {
                env.call_method(activity, "askAllFilesAccess", "()V", &[])?;
                Ok(())
            })
        }

        /// Opens a web link in the browser.
        pub fn open_url(&self, url: &str) -> Result<(), Error> {
            self.call("openUrl", |env, activity| {
                let url = env.new_string(url)?;
                env.call_method(activity, "openUrl", "(Ljava/lang/String;)V", &[JValue::Object(&url)])?;
                Ok(())
            })
        }

        /// Runs `f` on this thread's JNI environment; a Java exception becomes an [`Error`].
        fn call(&self, what: &str, f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<()>) -> Result<(), Error> {
            let mut env = self.vm.attach_current_thread_permanently().map_err(|e| jni_error("attach to the JVM", e))?;
            // this thread stays attached for the life of the app, so local references would never
            // be freed on their own: give each call its own frame, dropped when it returns
            let activity = self.activity.as_obj();
            let result = env.with_local_frame(16, |env| f(env, activity));
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
                return Err(Error(format!("{what}: the activity threw an exception (see logcat)")));
            }
            result.map_err(|e| jni_error(what, e))
        }
    }

    fn pending<R>(f: impl FnOnce(&mut HashMap<i64, Sender<Vec<String>>>) -> R) -> R {
        let mut guard = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
        f(guard.get_or_insert_with(HashMap::new))
    }

    fn string_array<'l>(env: &mut JNIEnv<'l>, items: &[String]) -> jni::errors::Result<JObjectArray<'l>> {
        let len = i32::try_from(items.len()).unwrap_or(i32::MAX);
        let array = env.new_object_array(len, "java/lang/String", JObject::null())?;
        for (i, s) in items.iter().take(len as usize).enumerate() {
            let js = env.new_string(s)?;
            env.set_object_array_element(&array, i as i32, &js)?;
            env.delete_local_ref(js)?;
        }
        Ok(array)
    }

    fn read_strings(env: &mut JNIEnv, array: &JObjectArray) -> jni::errors::Result<Vec<String>> {
        if array.is_null() {
            return Ok(Vec::new());
        }
        let len = env.get_array_length(array)?;
        let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
        for i in 0..len {
            let item = env.get_object_array_element(array, i)?;
            let item = env.auto_local(item);
            if item.is_null() {
                continue;
            }
            let s: String = env.get_string(<&JString>::from(&*item))?.into();
            out.push(s);
        }
        Ok(out)
    }

    /// `void nativePicked(long id, String[] paths)`: the activity's answer to [`Host::pick`].
    extern "system" fn native_picked<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>, id: jlong, paths: JObjectArray<'l>) {
        // a panic must never unwind into the JVM
        let answered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let list = read_strings(&mut env, &paths).unwrap_or_else(|e| {
                log::warn!("picker answer unreadable: {e}");
                Vec::new()
            });
            if let Some(tx) = pending(|p| p.remove(&id)) {
                let _ = tx.send(list);
            }
            if let Some(wake) = WAKER.get() {
                wake();
            }
        }));
        if answered.is_err() {
            log::error!("picker answer: a panic was caught");
        }
    }

    fn copies() -> Option<std::sync::Arc<dyn CopyProgress>> {
        COPIES.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// `boolean nativeCopyProgress(long task, long done, long total, String detail)`: a copy's
    /// progress; answers whether it should stop.
    extern "system" fn native_copy_progress<'l>(
        mut env: JNIEnv<'l>,
        _this: JObject<'l>,
        task: jlong,
        done: jlong,
        total: jlong,
        detail: JString<'l>,
    ) -> jni::sys::jboolean {
        let stop = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let detail: String = if detail.is_null() { String::new() } else { env.get_string(&detail).map(Into::into).unwrap_or_default() };
            let done = u64::try_from(done).unwrap_or(0);
            let total = u64::try_from(total).unwrap_or(0);
            copies().is_some_and(|c| c.progress(task, done, total, &detail))
        }));
        match stop {
            Ok(true) => jni::sys::JNI_TRUE,
            Ok(false) => jni::sys::JNI_FALSE,
            Err(_) => {
                log::error!("copy progress: a panic was caught");
                jni::sys::JNI_FALSE
            }
        }
    }

    /// `void nativeCopyEnded(long task)`.
    extern "system" fn native_copy_ended<'l>(_env: JNIEnv<'l>, _this: JObject<'l>, task: jlong) {
        let ended = std::panic::catch_unwind(|| {
            if let Some(c) = copies() {
                c.ended(task);
            }
        });
        if ended.is_err() {
            log::error!("copy ended: a panic was caught");
        }
    }
}

/// The JVM handles android-activity gives the app, adopted for the `jni` crate.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
mod jvm {
    use android_activity::AndroidApp;
    use jni::JavaVM;
    use jni::objects::JObject;

    use super::Error;

    /// The process's Java VM.
    pub fn vm(app: &AndroidApp) -> Result<JavaVM, Error> {
        let ptr = app.vm_as_ptr();
        if ptr.is_null() {
            return Err(Error("the activity has no Java VM".into()));
        }
        // SAFETY: android-activity returns the process's `JavaVM*` (from the NativeActivity it was
        // started by), non-null as checked, valid for the life of the process.
        unsafe { JavaVM::from_raw(ptr.cast()) }.map_err(|e| Error(format!("adopt the Java VM: {e}")))
    }

    /// The activity object, as a reference that is only used to make a global reference at once.
    pub fn activity(app: &AndroidApp) -> Result<JObject<'static>, Error> {
        let ptr = app.activity_as_ptr();
        if ptr.is_null() {
            return Err(Error("no activity".into()));
        }
        // SAFETY: android-activity returns its own global reference to the `Activity` (a `jobject`),
        // non-null as checked and valid while the app runs; the caller turns it into a global
        // reference of its own straight away and never deletes this one.
        Ok(unsafe { JObject::from_raw(ptr.cast()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_narrow_the_picker() {
        assert_eq!(PickRequest::mime_types_for(&["jpg", "JPEG", "png"]), vec!["image/jpeg", "image/png"]);
    }

    #[test]
    fn an_unknown_extension_offers_every_file() {
        // a raw or preset file must never be greyed out
        assert!(PickRequest::mime_types_for(&["jpg", "cr3"]).is_empty());
        assert!(PickRequest::mime_types_for(&["lcpreset"]).is_empty());
    }

    #[test]
    fn kinds_have_distinct_codes() {
        let codes = [PickKind::Files, PickKind::File, PickKind::Folder, PickKind::Save].map(PickKind::code);
        for (i, c) in codes.iter().enumerate() {
            assert!(!codes[i + 1..].contains(c));
        }
    }
}
