//! Updating a copy unpacked from the portable zip in place: unpack the new zip beside the program,
//! swap its files in, start the new version, and let the old one quit.
//!
//! Windows doesn't let a running program's file be overwritten or deleted, but it does let it be
//! renamed. So the running process swaps its own files, with no script or helper process:
//!
//! 1. The zip is unpacked into a fresh folder beside the program (`.<name>-update-<pid>`, on the same
//!    drive, so each later move is a rename). An entry that would land outside it, a link, a zip
//!    without the program, or more than [`MAX_ENTRIES`] files or [`MAX_UNPACKED`] bytes is refused
//!    before anything is moved. The zip's single top folder (the release zips have one) is dropped.
//! 2. Each unpacked file takes its place: the current file is renamed to `<file>.update-old`
//!    ([`OLD_SUFFIX`]), the new one renamed in. Only the zip's files are touched: `portable.txt`, the
//!    portable data folder and anything else the user put there stay as they are. A rename that
//!    fails is retried for a moment (antivirus software holds new files briefly); if it still fails,
//!    every file moved so far is put back.
//! 3. The new program is started with `<PREFIX>_AFTER_UPDATE` set ([`AFTER_UPDATE`]) and this one quits. The new one waits until
//!    the old program's file can be deleted, which is once the old process has ended, before it
//!    opens the library ([`finish_previous`]); if the old one is still running after
//!    [`WAIT_FOR_EXIT`] (the user cancelled quitting), the new one ends without opening anything.
//!    If it can't be started, the swap is undone.
//!
//! On the next start, [`clean_up`] deletes whatever `.update-old` files and unpack folders are left,
//! and puts back a `.update-old` file whose replacement never arrived (an update cut off half way).
//!
//! The steps use only renames and file copies, so they run (and are tested) on every system; only
//! Windows uses them.

use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::UpdateError;

/// What a replaced file is renamed to until the next start deletes it.
pub const OLD_SUFFIX: &str = ".update-old";

/// The environment variable (`<PREFIX>_AFTER_UPDATE`, through [`lcc_brand::env_var`]) the new
/// version is started with: it waits for the old one to quit. A variable, not an argument, so a
/// version that doesn't know it (an older release) ignores it instead of refusing to start.
pub const AFTER_UPDATE: &str = "AFTER_UPDATE";

/// How long the new version waits for the old one to quit. Quitting can wait on the user (a
/// running export, changes that couldn't be saved), so this is long; a new version still waiting
/// after it does not start (the old one stays, and the next start is the new version).
pub const WAIT_FOR_EXIT: Duration = Duration::from_secs(120);

/// The most files a zip may hold (the release zip has about ten).
pub const MAX_ENTRIES: usize = 4096;

/// The most bytes a zip may unpack to.
pub const MAX_UNPACKED: u64 = 4 << 30;

/// How long a rename or delete that Windows refuses for a moment is retried.
const RETRY_FOR: Duration = Duration::from_secs(3);
const RETRY_PAUSE: Duration = Duration::from_millis(100);

/// The unpack folder's name prefix for the program `exe_name` (`.app-update-`).
fn stage_prefix(exe_name: &str) -> String {
    let stem = Path::new(exe_name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    format!(".{stem}-update-")
}

/// `<file>.update-old` beside `file`.
fn old_path(file: &Path) -> PathBuf {
    let mut name = file.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(OLD_SUFFIX);
    file.with_file_name(name)
}

fn install_error(what: impl std::fmt::Display) -> UpdateError {
    UpdateError::Install(what.to_string())
}

/// Whether files can be created in `dir` (a file created and deleted again).
pub fn writable(dir: &Path, exe_name: &str) -> bool {
    let probe = dir.join(format!("{}probe-{}", stage_prefix(exe_name), std::process::id()));
    match File::options().write(true).create_new(true).open(&probe) {
        Ok(f) => {
            drop(f);
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Replace the files of the copy in `dir` (whose program is `exe_name`) with those in `zip`, then
/// start the new program through `start`. `Ok(n)`: `n` files were replaced or added and the new
/// version is starting; the caller quits. `Err`: nothing in `dir` changed.
pub fn swap_in(zip: &Path, dir: &Path, exe_name: &str, start: &mut dyn FnMut(&Path) -> Result<(), String>) -> Result<usize, UpdateError> {
    let stage = dir.join(format!("{}{}", stage_prefix(exe_name), std::process::id()));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| install_error(format!("could not clear {}: {e}", stage.display())))?;
    }
    fs::create_dir(&stage).map_err(|e| install_error(format!("could not unpack the update beside the program ({}): {e}", stage.display())))?;
    let result = unpack(zip, &stage, exe_name).and_then(|files| {
        let mut moved = Vec::new();
        let swapped = swap(&stage, dir, &files, &mut moved).and_then(|()| {
            let exe =
                files.iter().find(|f| is_program(f, exe_name)).map(|f| dir.join(f)).ok_or_else(|| install_error("the update has no program"))?;
            start(&exe).map_err(|e| install_error(format!("the new version could not be started ({e})")))
        });
        match swapped {
            Ok(()) => Ok(files.len()),
            Err(e) => {
                let problems = roll_back(&moved);
                let e = match e {
                    UpdateError::Install(why) => why,
                    other => other.to_string(),
                };
                if problems.is_empty() {
                    Err(install_error(format!("{e}; this copy was left as it was")))
                } else {
                    Err(install_error(format!("{e}; putting the previous files back also failed: {}", problems.join("; "))))
                }
            }
        }
    });
    if let Err(e) = fs::remove_dir_all(&stage) {
        log::warn!("updates: could not delete {}: {e}", stage.display());
    }
    result
}

/// Whether `rel` (a path inside the zip) is the program itself.
fn is_program(rel: &Path, exe_name: &str) -> bool {
    rel.components().count() == 1 && rel.to_string_lossy().eq_ignore_ascii_case(exe_name)
}

/// Unpack `zip` into `stage`, its single top folder dropped; the files' paths, relative to `stage`.
fn unpack(zip: &Path, stage: &Path, exe_name: &str) -> Result<Vec<PathBuf>, UpdateError> {
    let file = File::open(zip).map_err(|e| install_error(format!("could not open {}: {e}", zip.display())))?;
    let mut archive =
        zip::ZipArchive::new(BufReader::new(file)).map_err(|e| install_error(format!("{} is not a zip that can be read: {e}", zip.display())))?;
    if archive.len() > MAX_ENTRIES {
        return Err(install_error(format!("the update zip holds {} entries, more than an update has", archive.len())));
    }
    // the files, by index, and where each goes; folders are made as the files need them
    let mut entries: Vec<(usize, PathBuf)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| install_error(format!("the update zip is damaged: {e}")))?;
        let Some(path) = entry.enclosed_name() else {
            return Err(install_error(format!("the update zip names a file outside its folder ({:?})", entry.name())));
        };
        if entry.is_symlink() {
            return Err(install_error(format!("the update zip holds a link ({:?})", entry.name())));
        }
        if !entry.is_dir() && path.components().all(|c| matches!(c, Component::Normal(_))) && path.components().count() > 0 {
            entries.push((i, path));
        }
    }
    let top = single_top_folder(entries.iter().map(|(_, p)| p.as_path()));
    let files: Vec<(usize, PathBuf)> =
        entries.into_iter().map(|(i, p)| (i, top.as_ref().and_then(|t| p.strip_prefix(t).ok()).map(Path::to_path_buf).unwrap_or(p))).collect();
    if !files.iter().any(|(_, p)| is_program(p, exe_name)) {
        return Err(install_error(format!("the update zip has no {exe_name} at its top")));
    }
    if let Some((_, p)) =
        files.iter().find(|(_, p)| p.to_string_lossy().ends_with(OLD_SUFFIX) || p.to_string_lossy().starts_with(&stage_prefix(exe_name)))
    {
        return Err(install_error(format!("the update zip holds {} (a name the updater uses itself)", p.display())));
    }
    let mut left = MAX_UNPACKED;
    for (i, rel) in &files {
        let mut entry = archive.by_index(*i).map_err(|e| install_error(format!("the update zip is damaged: {e}")))?;
        let out = stage.join(rel);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| install_error(format!("could not create {}: {e}", parent.display())))?;
        }
        let mut file =
            File::options().write(true).create_new(true).open(&out).map_err(|e| install_error(format!("could not write {}: {e}", out.display())))?;
        // one byte past the budget shows that the zip is larger than allowed
        let n = io::copy(&mut (&mut entry).take(left.saturating_add(1)), &mut file)
            .map_err(|e| install_error(format!("could not unpack {}: {e}", rel.display())))?;
        if n > left {
            return Err(install_error(format!("the update zip unpacks to more than {} MB", MAX_UNPACKED >> 20)));
        }
        left -= n;
        file.sync_all().map_err(|e| install_error(format!("could not write {}: {e}", out.display())))?;
    }
    // the program last: an update stopped half way still has a program to start
    let (program, mut rest): (Vec<_>, Vec<_>) = files.into_iter().map(|(_, p)| p).partition(|p| is_program(p, exe_name));
    rest.extend(program);
    Ok(rest)
}

/// The one folder every path starts in, when they all sit inside the same one.
fn single_top_folder<'a>(paths: impl Iterator<Item = &'a Path>) -> Option<PathBuf> {
    let mut top: Option<&std::ffi::OsStr> = None;
    let mut any = false;
    for p in paths {
        any = true;
        let mut parts = p.components();
        let (Some(Component::Normal(first)), Some(_)) = (parts.next(), parts.next()) else { return None };
        match top {
            None => top = Some(first),
            Some(t) if t == first => {}
            Some(_) => return None,
        }
    }
    if any { top.map(PathBuf::from) } else { None }
}

/// One file the swap moved, so it can be put back.
#[derive(Debug)]
struct Moved {
    /// The file's place in the copy.
    target: PathBuf,
    /// Where the previous file went (`None`: the update added the file).
    old: Option<PathBuf>,
    /// Whether the new file is in place.
    placed: bool,
}

/// Move each unpacked file from `stage` into `dir`, keeping the file it replaces as `.update-old`;
/// `moved` records each step for [`roll_back`].
fn swap(stage: &Path, dir: &Path, files: &[PathBuf], moved: &mut Vec<Moved>) -> Result<(), UpdateError> {
    for rel in files {
        let target = dir.join(rel);
        let staged = stage.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| install_error(format!("could not create {}: {e}", parent.display())))?;
        }
        if target.is_dir() {
            return Err(install_error(format!("{} is a folder; the update has a file there", target.display())));
        }
        let old = if target.exists() {
            let old = old_path(&target);
            if old.exists() {
                // left by an earlier update; still in use when that copy is still running
                retry(|| fs::remove_file(&old)).map_err(|e| {
                    install_error(format!(
                        "{} from an earlier update can't be deleted ({e}); quit every copy of the app and try again",
                        old.display()
                    ))
                })?;
            }
            retry(|| fs::rename(&target, &old)).map_err(|e| install_error(format!("could not move {} aside: {e}", target.display())))?;
            // a read-only file can't be deleted later, by the new version's wait or the clean-up
            clear_read_only(&old);
            Some(old)
        } else {
            None
        };
        moved.push(Moved { target: target.clone(), old, placed: false });
        retry(|| fs::rename(&staged, &target)).map_err(|e| install_error(format!("could not put the new {} in place: {e}", rel.display())))?;
        if let Some(m) = moved.last_mut() {
            m.placed = true;
        }
    }
    Ok(())
}

/// Clear `file`'s read-only attribute (Windows refuses to delete a read-only file); a file that
/// isn't read-only, or can't be changed, is left as it is.
fn clear_read_only(file: &Path) {
    let Ok(meta) = fs::metadata(file) else { return };
    let mut perms = meta.permissions();
    if perms.readonly() {
        // on Windows this clears FILE_ATTRIBUTE_READONLY only; on Unix (where the copy is never
        // swapped outside the tests) it makes the file writable by its owner's group and others too,
        // which doesn't matter for a file about to be deleted
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        if let Err(e) = fs::set_permissions(file, perms) {
            log::debug!("updates: {} stays read-only: {e}", file.display());
        }
    }
}

/// Undo `moved`, newest first: delete each new file and rename the previous one back. What could
/// not be put back, one sentence each.
fn roll_back(moved: &[Moved]) -> Vec<String> {
    let mut problems = Vec::new();
    for m in moved.iter().rev() {
        if m.placed
            && let Err(e) = retry(|| fs::remove_file(&m.target))
        {
            problems.push(format!("{} could not be deleted ({e})", m.target.display()));
            continue;
        }
        if let Some(old) = &m.old
            && let Err(e) = retry(|| fs::rename(old, &m.target))
        {
            problems.push(format!("{} could not be renamed back to {} ({e})", old.display(), m.target.display()));
        }
    }
    problems
}

/// `op`, retried for [`RETRY_FOR`] while Windows refuses it for a moment (a file another process,
/// usually a virus scanner, has open).
fn retry<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let start = Instant::now();
    loop {
        match op() {
            Ok(v) => return Ok(v),
            // 32: sharing violation, 33: lock violation
            Err(e) if (e.kind() == io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(32 | 33))) && start.elapsed() < RETRY_FOR => {
                std::thread::sleep(RETRY_PAUSE)
            }
            Err(e) => return Err(e),
        }
    }
}

/// In the new version started by [`swap_in`]: wait until the previous program, `<exe>.update-old`,
/// can be deleted (its process has ended), for at most `wait`, and delete it. `true` once it is
/// gone; `false` when it is still in use after `wait`.
pub fn finish_previous(exe: &Path, wait: Duration) -> bool {
    let old = old_path(exe);
    let start = Instant::now();
    loop {
        match fs::remove_file(&old) {
            Ok(()) => return true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return true,
            Err(_) if start.elapsed() < wait => std::thread::sleep(RETRY_PAUSE),
            Err(e) => {
                log::warn!("updates: the previous version ({}) is still running after {} s: {e}", old.display(), wait.as_secs());
                return false;
            }
        }
    }
}

/// Delete what an update left in the copy in `dir` (whose program is `exe_name`): `.update-old`
/// files, at most a few folders deep, and unpack folders other than this process's. Never fails:
/// what can't be deleted now (a file still in use) is left for the next start.
pub fn clean_up(dir: &Path, exe_name: &str) {
    let prefix = stage_prefix(exe_name);
    let ours = format!("{prefix}{}", std::process::id());
    clean_dir(dir, &prefix, &ours, 0);
}

fn clean_dir(dir: &Path, prefix: &str, ours: &str, depth: usize) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_file() && name.ends_with(OLD_SUFFIX) {
            // an update stopped between moving a file aside and putting the new one in (the power
            // went off): the previous file is all there is, so it goes back
            let original = path.with_file_name(name.trim_end_matches(OLD_SUFFIX));
            if !original.exists() {
                match fs::rename(&path, &original) {
                    Ok(()) => log::warn!("updates: an interrupted update left {} aside; put back", original.display()),
                    Err(e) => log::warn!("updates: {} could not be put back as {}: {e}", path.display(), original.display()),
                }
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => log::info!("updates: deleted the previous version's {}", path.display()),
                Err(e) => log::debug!("updates: {} is still in use ({e}); deleted next time", path.display()),
            }
        } else if kind.is_file() && depth == 0 && name.starts_with(&format!("{prefix}probe-")) {
            // a write probe left by a process that ended before deleting it
            let _ = fs::remove_file(&path);
        } else if kind.is_dir() && depth == 0 && name.starts_with(prefix) && name != ours {
            if let Err(e) = fs::remove_dir_all(&path) {
                log::debug!("updates: could not delete {}: {e}", path.display());
            }
        } else if kind.is_dir() && depth < 3 {
            clean_dir(&path, prefix, ours, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lcc-zipswap-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// A zip of `files` (path, contents), deflated.
    fn zip_of(path: &Path, files: &[(&str, &str)]) {
        let mut w = zip::ZipWriter::new(File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    /// A copy unpacked from an older zip, with the user's own files beside it.
    fn copy_in(dir: &Path) {
        fs::write(dir.join("app.exe"), "old app").unwrap();
        fs::write(dir.join("app-cli.exe"), "old cli").unwrap();
        fs::write(dir.join("LICENSE.txt"), "old licence").unwrap();
        fs::write(dir.join("portable.txt"), "").unwrap();
        fs::create_dir_all(dir.join("AppData")).unwrap();
        fs::write(dir.join("AppData").join("settings.json"), "{}").unwrap();
    }

    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap()
    }

    #[test]
    fn the_zip_replaces_its_files_and_starts_the_new_program() {
        let root = scratch("swap");
        let dir = root.join("copy");
        fs::create_dir_all(&dir).unwrap();
        copy_in(&dir);
        let zip = root.join("app-2.0.0-windows-x64-portable.zip");
        zip_of(
            &zip,
            &[
                ("app-2.0.0-windows-x64-portable/app.exe", "new app"),
                ("app-2.0.0-windows-x64-portable/app-cli.exe", "new cli"),
                ("app-2.0.0-windows-x64-portable/LICENSE.txt", "new licence"),
                ("app-2.0.0-windows-x64-portable/OFL-Inter.txt", "new font licence"),
            ],
        );
        let mut started = Vec::new();
        let n = swap_in(&zip, &dir, "app.exe", &mut |exe| {
            started.push(exe.to_path_buf());
            Ok(())
        })
        .unwrap();
        assert_eq!(n, 4);
        assert_eq!(started, [dir.join("app.exe")]);
        assert_eq!(read(&dir.join("app.exe")), "new app");
        assert_eq!(read(&dir.join("app-cli.exe")), "new cli");
        assert_eq!(read(&dir.join("OFL-Inter.txt")), "new font licence", "a file the update adds is added");
        // the previous files wait for the next start; the user's files are untouched
        assert_eq!(read(&dir.join("app.exe.update-old")), "old app");
        assert!(dir.join("portable.txt").is_file());
        assert_eq!(read(&dir.join("AppData").join("settings.json")), "{}");
        // no unpack folder is left
        let leftovers: Vec<_> =
            fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(".app-update-")).collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        // the new version's start: the previous program is deleted, then the rest
        assert!(finish_previous(&dir.join("app.exe"), Duration::ZERO));
        assert!(!dir.join("app.exe.update-old").exists());
        clean_up(&dir, "app.exe");
        assert!(!dir.join("app-cli.exe.update-old").exists() && !dir.join("LICENSE.txt.update-old").exists());
        assert!(dir.join("portable.txt").is_file() && dir.join("AppData").join("settings.json").is_file());
    }

    #[test]
    fn a_zip_without_a_top_folder_works_too_and_a_second_update_replaces_the_first_ones_leftovers() {
        let root = scratch("flat");
        let dir = root.join("copy");
        fs::create_dir_all(&dir).unwrap();
        copy_in(&dir);
        fs::write(dir.join("app.exe.update-old"), "older app").unwrap();
        let zip = root.join("u.zip");
        zip_of(&zip, &[("app.exe", "new app"), ("docs/readme.txt", "hello")]);
        swap_in(&zip, &dir, "app.exe", &mut |_| Ok(())).unwrap();
        assert_eq!(read(&dir.join("app.exe")), "new app");
        assert_eq!(read(&dir.join("app.exe.update-old")), "old app");
        assert_eq!(read(&dir.join("docs").join("readme.txt")), "hello");
    }

    #[test]
    fn a_program_that_cannot_start_puts_everything_back() {
        let root = scratch("rollback");
        let dir = root.join("copy");
        fs::create_dir_all(&dir).unwrap();
        copy_in(&dir);
        let zip = root.join("u.zip");
        zip_of(&zip, &[("top/app.exe", "new app"), ("top/app-cli.exe", "new cli"), ("top/NEW.txt", "added")]);
        let err = swap_in(&zip, &dir, "app.exe", &mut |_| Err("refused".into())).unwrap_err();
        assert!(err.to_string().contains("refused") && err.to_string().contains("left as it was"), "{err}");
        assert_eq!(read(&dir.join("app.exe")), "old app");
        assert_eq!(read(&dir.join("app-cli.exe")), "old cli");
        assert!(!dir.join("NEW.txt").exists(), "a file the update added is removed again");
        assert!(!dir.join("app.exe.update-old").exists());
    }

    #[test]
    fn a_hostile_or_wrong_zip_changes_nothing() {
        let root = scratch("hostile");
        let dir = root.join("copy");
        fs::create_dir_all(&dir).unwrap();
        copy_in(&dir);
        let cases: Vec<(&str, Vec<(&str, &str)>, &str)> = vec![
            ("escape", vec![("app.exe", "new"), ("../evil.txt", "x")], "outside its folder"),
            ("no-program", vec![("top/other.exe", "x"), ("top/LICENSE.txt", "x")], "no app.exe"),
            ("program-not-at-top", vec![("a/b/app.exe", "x"), ("a/c.txt", "x")], "no app.exe"),
            ("our-own-name", vec![("app.exe", "new"), ("app-cli.exe.update-old", "x")], "updater uses itself"),
            ("empty", vec![], "no app.exe"),
        ];
        for (name, files, why) in cases {
            let zip = root.join(format!("{name}.zip"));
            zip_of(&zip, &files);
            let err = swap_in(&zip, &dir, "app.exe", &mut |_| panic!("{name}: nothing may start")).unwrap_err();
            assert!(err.to_string().contains(why), "{name}: {err}");
            assert_eq!(read(&dir.join("app.exe")), "old app", "{name}");
            assert!(!root.join("evil.txt").exists() && !dir.join("app.exe.update-old").exists(), "{name}");
        }
        // not a zip at all, and a zip that isn't there
        let junk = root.join("junk.zip");
        fs::write(&junk, b"PK\x03\x04 truncated").unwrap();
        for zip in [junk, root.join("missing.zip")] {
            let err = swap_in(&zip, &dir, "app.exe", &mut |_| panic!("nothing may start")).unwrap_err();
            assert!(matches!(err, UpdateError::Install(_)), "{err}");
            assert_eq!(read(&dir.join("app.exe")), "old app");
        }
    }

    /// A name with a leading `/` is read as relative to the zip (the zip format has no absolute
    /// names): it lands inside the copy, never at the drive's root.
    #[test]
    fn a_rooted_name_stays_inside_the_copy() {
        let root = scratch("rooted");
        let dir = root.join("copy");
        fs::create_dir_all(&dir).unwrap();
        copy_in(&dir);
        let zip = root.join("u.zip");
        zip_of(&zip, &[("/app.exe", "new app"), ("/rooted.txt", "x")]);
        swap_in(&zip, &dir, "app.exe", &mut |_| Ok(())).unwrap();
        assert_eq!(read(&dir.join("rooted.txt")), "x");
        assert_eq!(read(&dir.join("app.exe")), "new app");
    }

    #[test]
    fn the_top_folder_is_dropped_only_when_everything_is_in_it() {
        let p = |v: &[&'static str]| single_top_folder(v.iter().map(Path::new));
        assert_eq!(p(&["a/x", "a/y/z"]), Some(PathBuf::from("a")));
        assert_eq!(p(&["a/x", "b/y"]), None);
        assert_eq!(p(&["a/x", "y"]), None);
        assert_eq!(p(&["x"]), None);
        assert_eq!(p(&[]), None);
    }

    #[test]
    fn clean_up_deletes_only_what_updates_leave() {
        let dir = scratch("clean");
        fs::write(dir.join("app.exe"), "app").unwrap();
        fs::write(dir.join("app.exe.update-old"), "old").unwrap();
        fs::write(dir.join("notes.old"), "the user's").unwrap();
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub").join("x.dll.update-old"), "old").unwrap();
        fs::create_dir_all(dir.join(".app-update-1")).unwrap();
        fs::write(dir.join(".app-update-1").join("app.exe"), "half unpacked").unwrap();
        let ours = dir.join(format!(".app-update-{}", std::process::id()));
        fs::create_dir_all(&ours).unwrap();
        clean_up(&dir, "app.exe");
        assert!(!dir.join("app.exe.update-old").exists() && !dir.join("sub").join("x.dll.update-old").exists());
        assert!(!dir.join(".app-update-1").exists());
        assert!(ours.exists(), "this process's own unpack folder is left alone");
        assert!(dir.join("notes.old").exists() && dir.join("app.exe").exists());
    }

    #[test]
    fn the_program_is_swapped_last() {
        let root = scratch("order");
        let stage = root.join("stage");
        fs::create_dir_all(&stage).unwrap();
        let zip = root.join("u.zip");
        zip_of(&zip, &[("top/app.exe", "new"), ("top/a.txt", "a"), ("top/z.txt", "z")]);
        let files = unpack(&zip, &stage, "app.exe").unwrap();
        assert_eq!(files.last().map(PathBuf::as_path), Some(Path::new("app.exe")), "{files:?}");
        assert_eq!(files.len(), 3);
    }

    /// The power went off between moving a file aside and putting its replacement in: the next
    /// start puts the previous file back instead of deleting the only copy.
    #[test]
    fn clean_up_puts_back_a_file_an_interrupted_update_left_aside() {
        let dir = scratch("interrupted");
        fs::write(dir.join("app-cli.exe.update-old"), "old cli").unwrap();
        fs::write(dir.join("app.exe"), "app").unwrap();
        fs::write(dir.join("app.exe.update-old"), "old app").unwrap();
        clean_up(&dir, "app.exe");
        assert_eq!(read(&dir.join("app-cli.exe")), "old cli", "the file with no replacement is back");
        assert!(!dir.join("app-cli.exe.update-old").exists());
        assert!(!dir.join("app.exe.update-old").exists(), "a replaced file's previous copy is deleted");
        assert_eq!(read(&dir.join("app.exe")), "app");
    }

    #[test]
    fn clean_up_deletes_a_write_probe_left_behind() {
        let dir = scratch("probe");
        fs::write(dir.join(".app-update-probe-1"), "").unwrap();
        clean_up(&dir, "app.exe");
        assert!(!dir.join(".app-update-probe-1").exists());
        assert!(writable(&dir, "app.exe"));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "the probe deletes itself");
    }

    /// A read-only previous file (a copy restored from a backup) is made deletable when it is moved
    /// aside, so the new version's wait and the clean-up can delete it.
    #[test]
    fn a_read_only_file_is_replaced_and_its_previous_copy_deleted() {
        let root = scratch("readonly");
        let dir = root.join("copy");
        fs::create_dir_all(&dir).unwrap();
        copy_in(&dir);
        for f in ["app.exe", "LICENSE.txt"] {
            let mut p = fs::metadata(dir.join(f)).unwrap().permissions();
            p.set_readonly(true);
            fs::set_permissions(dir.join(f), p).unwrap();
        }
        let zip = root.join("u.zip");
        zip_of(&zip, &[("top/app.exe", "new app"), ("top/LICENSE.txt", "new licence")]);
        swap_in(&zip, &dir, "app.exe", &mut |_| Ok(())).unwrap();
        assert_eq!(read(&dir.join("app.exe")), "new app");
        assert!(finish_previous(&dir.join("app.exe"), Duration::ZERO), "the read-only program is deleted at once");
        clean_up(&dir, "app.exe");
        assert!(!dir.join("LICENSE.txt.update-old").exists());
    }

    #[test]
    fn waiting_for_a_previous_version_that_is_not_there_returns_at_once() {
        let dir = scratch("wait");
        let started = Instant::now();
        assert!(finish_previous(&dir.join("app.exe"), Duration::from_secs(30)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    /// On Windows the running program can be renamed but not deleted: the swap works while it runs,
    /// and the new version's wait sees the old one as still running.
    #[cfg(windows)]
    #[test]
    fn a_running_program_is_renamed_aside_and_waited_for() {
        let root = scratch("running");
        let dir = root.join("copy");
        fs::create_dir_all(&dir).unwrap();
        // a real program that keeps running: a copy of Windows' own `timeout`-free waiter, ping
        let ping = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32").join("PING.EXE");
        fs::copy(&ping, dir.join("app.exe")).unwrap();
        let mut child =
            std::process::Command::new(dir.join("app.exe")).args(["-n", "30", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap();
        let zip = root.join("u.zip");
        zip_of(&zip, &[("top/app.exe", "new app")]);
        let swapped = swap_in(&zip, &dir, "app.exe", &mut |_| Ok(()));
        let waited = swapped.is_ok().then(|| finish_previous(&dir.join("app.exe"), Duration::from_millis(300)));
        let _ = child.kill();
        let _ = child.wait();
        swapped.unwrap();
        assert_eq!(read(&dir.join("app.exe")), "new app");
        assert_eq!(waited, Some(false), "the old program is still running");
        assert!(finish_previous(&dir.join("app.exe"), Duration::from_secs(10)), "it can be deleted once it has quit");
    }
}
