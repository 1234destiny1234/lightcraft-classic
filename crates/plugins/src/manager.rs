//! The Plugin Manager: installed plug-ins, their grants, enabled state, logs and data.
//!
//! On disk (a folder, usually `<settings>/plugins`): `<id>.wasm` per plug-in, `plugins.json` with
//! each plug-in's `{enabled, grant}`, and `data/<id>.json`, its metadata namespace. A manager
//! without a folder keeps everything in memory (tests, the web build).

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::context::{CallContext, HostApi};
use crate::manifest::{dialog_args, parse_version};
use crate::permissions::Permissions;
use crate::runtime::{Limits, Plugin};
use crate::store::{NamespaceStore, write_atomic};
use crate::{Error, Result};

/// Log lines kept per plug-in.
const MAX_LOG: usize = 500;
/// Most plug-ins loaded from a folder.
const MAX_PLUGINS: usize = 256;
/// Most photos / files one hook request carries.
const MAX_BATCH: usize = 10_000;

/// One installed plug-in.
#[derive(Debug)]
pub struct Installed {
    pub plugin: Arc<Plugin>,
    pub enabled: bool,
    /// What the user granted (the effective set is this ∩ the manifest's request).
    pub grant: Permissions,
    pub log: VecDeque<String>,
    store: Option<NamespaceStore>,
    /// A call is running off the manager's lock ([`Manager::begin`] … [`Manager::end`]): its store is lent to it.
    busy: bool,
}

impl Installed {
    /// The capabilities calls actually get.
    pub fn effective(&self) -> Permissions {
        self.grant.intersect(&self.plugin.manifest().permissions)
    }

    fn push_log(&mut self, line: String) {
        if self.log.len() >= MAX_LOG {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// The plug-in as JSON for lists (`plugin.list`).
    pub fn info(&self) -> Value {
        let m = self.plugin.manifest();
        json!({
            "id": m.id, "name": m.name, "version": m.version, "description": m.description, "author": m.author,
            "requires": m.requires,
            "enabled": self.enabled, "size": self.plugin.size(),
            "requested": m.permissions, "granted": self.effective(),
            "commands": m.commands, "hooks": m.hooks, "publish": m.publish,
            "exportDestinations": m.export_destinations, "metadataFields": m.metadata_fields,
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    plugins: BTreeMap<String, Entry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    enabled: bool,
    grant: Permissions,
}

/// A plug-in call prepared by [`Manager::begin`]: it owns what the call needs, so it can run
/// without the manager (on another thread), and goes back with [`Manager::end`].
pub struct PendingCall {
    id: String,
    plugin: Arc<Plugin>,
    perms: Permissions,
    store: NamespaceStore,
    request: Value,
    lent: Vec<PathBuf>,
    /// Folders lent for reading (a program run's folder in `program.end`).
    lent_dirs: Vec<PathBuf>,
    /// Where `program.start` makes run folders (`None`: [`crate::programs::runs_root`]).
    runs_root: Option<PathBuf>,
}

impl PendingCall {
    /// The plug-in this call is for.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Runs the call (the plug-in's code: possibly long, e.g. an upload).
    pub fn run(&mut self, host: &mut dyn HostApi) -> crate::Outcome {
        let manifest = self.plugin.manifest();
        let mut ctx = CallContext {
            plugin: &self.id,
            requested: &manifest.permissions,
            perms: &self.perms,
            lent: &self.lent,
            lent_dirs: &self.lent_dirs,
            runs_root: self.runs_root.as_deref(),
            host,
            store: &mut self.store,
            notes: Vec::new(),
        };
        let mut outcome = self.plugin.invoke(&self.request, &mut ctx);
        // what the host did on the plug-in's behalf that the user should see in its log
        let notes = std::mem::take(&mut ctx.notes);
        outcome.logs.extend(notes);
        outcome
    }
}

/// Refuses a grant from a command that names a program the user hasn't allowed in `granted`, or
/// more of one (a wider entry, [`crate::ProgramDecl::within`]). Repeating or leaving out granted
/// programs is fine: only the user adds them.
fn programs_kept(grant: &Permissions, granted: &Permissions) -> Result<()> {
    match grant.programs.iter().find(|p| !granted.programs.iter().any(|g| p.within(g))) {
        Some(p) => Err(Error::Params(format!(
            "programs can only be allowed by the user, in the Plugin Manager (File ▸ Plug-ins ▸ Plug-in Manager…); this grant names {:?}, which the user hasn't allowed as it stands",
            p.id
        ))),
        None => Ok(()),
    }
}

/// A module in the plug-in folder that didn't load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed {
    pub file: String,
    pub error: String,
    /// The host features it requires that this version lacks (empty: it failed for another reason).
    pub missing: Vec<String>,
    /// The plug-in's id, to remove it by ([`Manager::uninstall`]): its manifest's when that could be
    /// read, else the file's name without `.wasm` (installed modules are `<id>.wasm`). Empty: not a
    /// module in the folder.
    pub id: String,
    /// The module file, deleted when the plug-in is removed.
    pub path: Option<PathBuf>,
}

impl Failed {
    /// A failure that isn't a module in the folder (the folder itself couldn't be opened).
    pub fn new(file: String, error: String) -> Failed {
        Failed { file, error, missing: Vec::new(), id: String::new(), path: None }
    }
}

/// The set of installed plug-ins.
#[derive(Debug)]
pub struct Manager {
    dir: Option<PathBuf>,
    limits: Limits,
    /// The host features plug-ins may require ([`crate::HOST_FEATURES`]).
    features: Vec<String>,
    plugins: BTreeMap<String, Installed>,
    /// Modules in the folder that failed to load.
    pub failed: Vec<Failed>,
    /// `plugins.json` entries of modules that didn't load, written back unchanged, so a plug-in that
    /// loads again (the app updated, the module replaced) keeps its enabled state and grant.
    kept: BTreeMap<String, Entry>,
    /// The versions of modules that didn't load because they need features this version lacks, by
    /// id: installing an older one over them is still a downgrade.
    unloaded: BTreeMap<String, String>,
    /// The open library's id: plug-in data is kept per library (photo ids are per library).
    library: String,
    /// Where `program.start` makes run folders (`None`: [`crate::programs::runs_root`]).
    runs_root: Option<PathBuf>,
}

impl Default for Manager {
    fn default() -> Self {
        Manager {
            dir: None,
            limits: Limits::default(),
            features: crate::HOST_FEATURES.iter().map(|f| f.to_string()).collect(),
            plugins: BTreeMap::new(),
            failed: Vec::new(),
            kept: BTreeMap::new(),
            unloaded: BTreeMap::new(),
            library: String::new(),
            runs_root: None,
        }
    }
}

impl Manager {
    /// A manager that keeps nothing on disk.
    pub fn in_memory() -> Manager {
        Manager::default()
    }

    /// Opens the plug-in folder `dir` (created when missing) and loads every installed module;
    /// a module that fails to load is listed in [`Manager::failed`] and skipped.
    pub fn open(dir: &Path) -> Result<Manager> {
        Manager::open_with_features(dir, crate::HOST_FEATURES)
    }

    /// [`Manager::open`] for a host that has `features` (tests; the app has [`crate::HOST_FEATURES`]).
    pub fn open_with_features(dir: &Path, features: &[&str]) -> Result<Manager> {
        let io = |e: std::io::Error| Error::Io(format!("{}: {e}", dir.display()));
        std::fs::create_dir_all(dir).map_err(io)?;
        let state: State = match std::fs::read(dir.join("plugins.json")) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
                log::warn!("plugins.json is unreadable ({e}); plug-ins load disabled");
                State::default()
            }),
            Err(_) => State::default(),
        };
        // run folders a program still used when the app last quit
        #[cfg(not(target_arch = "wasm32"))]
        crate::programs::sweep_old_runs(&crate::programs::runs_root(), crate::programs::STALE_RUN_AGE);
        let mut m = Manager { dir: Some(dir.to_path_buf()), ..Manager::default() };
        m.set_features(features);
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(io)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wasm")))
            .collect();
        files.sort();
        files.truncate(MAX_PLUGINS);
        for f in files {
            let file = f.display().to_string();
            // installed modules are `<id>.wasm`: the id when the manifest can't be read
            let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let loaded = read_module(&f, &m.limits).and_then(|b| Plugin::load(&b, m.limits.clone()));
            let p = match loaded {
                Ok(p) => p,
                Err(e) => {
                    m.keep(&state, &stem);
                    m.failed.push(Failed { file, error: e.to_string(), missing: Vec::new(), id: stem, path: Some(f) });
                    continue;
                }
            };
            let id = p.id().to_string();
            let missing = p.manifest().missing_features(&m.features);
            if !missing.is_empty() {
                // the app was downgraded: keep its grant and data for when it's updated again
                m.keep(&state, &id);
                m.unloaded.insert(id.clone(), p.manifest().version.clone());
                m.failed.push(Failed { file, error: Error::Unsupported(missing.clone()).to_string(), missing, id, path: Some(f) });
                continue;
            }
            // not in plugins.json (copied in by hand): disabled, nothing granted
            let (enabled, grant) = state.plugins.get(&id).map_or((false, Permissions::default()), |e| (e.enabled, e.grant.clone()));
            let mut log = VecDeque::new();
            if let Err(e) = p.manifest().semver() {
                log.push_back(format!("loaded without a valid version ({e}); installing a new version will need one"));
            }
            m.plugins.insert(id, Installed { plugin: Arc::new(p), enabled, grant, log, store: None, busy: false });
        }
        Ok(m)
    }

    /// Keeps `id`'s `plugins.json` entry from `state` (its module didn't load).
    fn keep(&mut self, state: &State, id: &str) {
        if let Some(e) = state.plugins.get(id) {
            self.kept.insert(id.to_string(), e.clone());
        }
    }

    /// The host features plug-ins may require (default [`crate::HOST_FEATURES`]). Affects installs
    /// from now on; [`Manager::open_with_features`] sets them for loading.
    pub fn set_features(&mut self, features: &[&str]) {
        self.features = features.iter().map(|f| f.to_string()).collect();
    }

    /// The host features plug-ins may require.
    pub fn features(&self) -> &[String] {
        &self.features
    }

    /// The version of plug-in `id` in the folder: loaded, or kept there because it needs features
    /// this version lacks.
    pub fn installed_version(&self, id: &str) -> Option<String> {
        self.plugins.get(id).map(|i| i.plugin.manifest().version.clone()).or_else(|| self.unloaded.get(id).cloned())
    }

    /// A plug-in in the folder that didn't load because it needs features this version lacks: its
    /// version and the grant it keeps.
    pub fn unloaded(&self, id: &str) -> Option<(String, Permissions)> {
        let version = self.unloaded.get(id)?.clone();
        Some((version, self.kept.get(id).map(|e| e.grant.clone()).unwrap_or_default()))
    }

    /// Why `manifest` can't be installed over what is installed now, as `(missing features,
    /// version problem, downgrade from)`, for the install dialog ([`Manager::install_bytes`]
    /// refuses each one; a downgrade only without `allow_downgrade`).
    pub fn install_check(&self, manifest: &crate::Manifest) -> (Vec<String>, Option<String>, Option<String>) {
        let missing = manifest.missing_features(&self.features);
        let new = manifest.semver();
        let old = self.installed_version(&manifest.id);
        let downgrade = match (&new, old) {
            (Ok(new), Some(old)) => parse_version(&old).ok().filter(|o| new.cmp_precedence(o) == std::cmp::Ordering::Less).map(|_| old),
            _ => None,
        };
        (missing, new.err(), downgrade)
    }

    /// Limits for plug-ins installed from now on.
    /// The library now open (a stable id; `""`: none). Plug-in data is kept per library, since
    /// photo ids are only unique within one: every read and write from now on goes to this one's.
    /// The first library with an id claims data saved before data was kept per library.
    pub fn set_library(&mut self, library: &str) {
        self.library = library.to_string();
        for p in self.plugins.values_mut() {
            if let Some(store) = p.store.as_mut() {
                scope(store, library);
            }
        }
    }

    /// The open library's id (see [`Manager::set_library`]).
    pub fn library(&self) -> &str {
        &self.library
    }

    /// Where `program.start` makes run folders from now on (tests; `None`, the default:
    /// [`crate::programs::runs_root`]).
    pub fn set_runs_root(&mut self, root: Option<PathBuf>) {
        self.runs_root = root;
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Installs (or updates) a module. `grant` = what the user approved, one by one, on the
    /// install dialog. `None` grants nothing new: a fresh install gets no permissions, an update
    /// keeps the previous grant (cut back to what the new version still asks for), so an update
    /// can never gain a capability without the user approving it. The plug-in starts enabled.
    ///
    /// Refused, with nothing written: a plug-in that requires host features this version lacks, a
    /// `version` that isn't semver, and a lower version than the installed one unless
    /// `allow_downgrade` (the same version again is a repair, and allowed).
    ///
    /// `grant` can't add a program (`programs`): only the user grants those, in the Plugin
    /// Manager ([`Manager::install_bytes_by_user`]). It may repeat or leave out the programs the
    /// installed version has granted; one it hasn't, or a wider entry, is an error and installs
    /// nothing. A module whose code differs from the installed one (another digest) keeps none
    /// of its programs, and its log says so: the user allowed them for the code they had.
    pub fn install_bytes(&mut self, bytes: &[u8], grant: Option<Permissions>, allow_downgrade: bool) -> Result<&Installed> {
        self.install_checked(bytes, grant, allow_downgrade, false)
    }

    /// [`Manager::install_bytes`] as the user approved it in the Plugin Manager's install dialog:
    /// the grant may include programs. Never reachable from a command.
    pub fn install_bytes_by_user(&mut self, bytes: &[u8], grant: Option<Permissions>, allow_downgrade: bool) -> Result<&Installed> {
        self.install_checked(bytes, grant, allow_downgrade, true)
    }

    fn install_checked(&mut self, bytes: &[u8], grant: Option<Permissions>, allow_downgrade: bool, by_user: bool) -> Result<&Installed> {
        let plugin = Plugin::load(bytes, self.limits.clone())?;
        let (missing, version, downgrade) = self.install_check(plugin.manifest());
        if !missing.is_empty() {
            return Err(Error::Unsupported(missing));
        }
        if let Some(e) = version {
            return Err(Error::Manifest(e));
        }
        let new = plugin.manifest().version.clone();
        if let Some(installed) = &downgrade
            && !allow_downgrade
        {
            return Err(Error::Downgrade { installed: installed.clone(), new });
        }
        let id = plugin.id().to_string();
        // a module that stopped loading keeps its grant for the version that replaces it
        let mut previous = self.plugins.get(&id).map(|i| i.grant.clone()).or_else(|| self.kept.get(&id).map(|e| e.grant.clone())).unwrap_or_default();
        // programs were allowed for the code the user saw: different code from a command keeps
        // none (a module that stopped loading counts as different)
        let same_code = self.plugins.get(&id).is_some_and(|i| i.plugin.digest() == plugin.digest());
        let mut dropped = Vec::new();
        if !by_user && !same_code {
            let declared = &plugin.manifest().permissions.programs;
            dropped = std::mem::take(&mut previous.programs)
                .into_iter()
                .filter(|p| declared.iter().any(|d| d.id == p.id))
                .map(|p| format!("{}: this version's code is different, so it may no longer start it; allow it again in the Plug-in Manager", p.name))
                .collect();
        }
        if let Some(g) = &grant
            && !by_user
        {
            programs_kept(g, &previous)?;
        }
        let grant = grant.unwrap_or_else(|| previous.clone()).intersect(&plugin.manifest().permissions);
        // a granted program whose entry the update widened is unticked: say why
        let widened: Vec<String> = plugin
            .manifest()
            .permissions
            .programs
            .iter()
            .filter(|p| previous.program(&p.id).is_some_and(|g| !p.within(g)) && grant.program(&p.id).is_none())
            .map(|p| format!("{}: this version finds it in more places than you allowed; allow it again in the Plug-in Manager", p.name))
            .collect();
        if let Some(dir) = &self.dir {
            write_atomic(&dir.join(format!("{id}.wasm")), bytes)?;
            let file = dir.join(format!("{id}.wasm")).display().to_string();
            self.failed.retain(|f| f.file != file);
        }
        self.kept.remove(&id);
        self.unloaded.remove(&id);
        let log = self.plugins.remove(&id).map(|i| i.log).unwrap_or_default();
        self.plugins.insert(id.clone(), Installed { plugin: Arc::new(plugin), enabled: true, grant, log, store: None, busy: false });
        self.save()?;
        self.plugins
            .get_mut(&id)
            .map(|i| {
                i.push_log(match downgrade {
                    Some(old) => format!("downgraded from {old} to {new}"),
                    None => format!("installed version {new}"),
                });
                for line in dropped.into_iter().chain(widened) {
                    i.push_log(line);
                }
                &*i
            })
            .ok_or(Error::NotFound(id))
    }

    /// Reads a `.wasm` file and installs it (see [`Manager::install_bytes`]).
    pub fn install_file(&mut self, path: &Path, grant: Option<Permissions>, allow_downgrade: bool) -> Result<&Installed> {
        let bytes = read_module(path, &self.limits)?;
        self.install_bytes(&bytes, grant, allow_downgrade)
    }

    /// [`Manager::install_file`] as the user approved it ([`Manager::install_bytes_by_user`]).
    pub fn install_file_by_user(&mut self, path: &Path, grant: Option<Permissions>, allow_downgrade: bool) -> Result<&Installed> {
        let bytes = read_module(path, &self.limits)?;
        self.install_bytes_by_user(&bytes, grant, allow_downgrade)
    }

    /// Removes a plug-in, its module and its data.
    ///
    /// A plug-in in the folder that didn't load ([`Manager::failed`]: it needs features this version
    /// lacks, or its module can't be read) is removed the same way, by its id there.
    pub fn uninstall(&mut self, id: &str) -> Result<()> {
        let is_failed = |f: &Failed| !id.is_empty() && f.id == id;
        if !self.plugins.contains_key(id) && !self.failed.iter().any(is_failed) {
            return Err(Error::NotFound(id.into()));
        }
        // the files first: if one can't be deleted, nothing is forgotten (its entry stays saved)
        if let Some(dir) = &self.dir {
            let modules: Vec<PathBuf> = self.failed.iter().filter(|f| is_failed(f)).filter_map(|f| f.path.clone()).collect();
            // the modules before the data: a module that can't be deleted keeps its data too
            let files = std::iter::once(dir.join(format!("{id}.wasm"))).chain(modules).chain([dir.join("data").join(format!("{id}.json"))]);
            for f in files {
                match std::fs::remove_file(&f) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(Error::Io(format!("{}: {e}", f.display()))),
                }
            }
        }
        self.plugins.remove(id);
        self.failed.retain(|f| !is_failed(f));
        self.kept.remove(id);
        self.unloaded.remove(id);
        self.save()
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        let p = self.plugins.get_mut(id).ok_or_else(|| Error::NotFound(id.into()))?;
        p.enabled = enabled;
        p.push_log(if enabled { "enabled".into() } else { "disabled".into() });
        self.save()
    }

    /// Replaces the grant (revoking what it leaves out); it can never exceed the request. It can't
    /// add a program: one the plug-in hasn't been granted, or a wider entry, is an error and
    /// changes nothing (only the user grants programs, [`Manager::set_grant_by_user`]). Repeating
    /// granted programs keeps them; leaving them out revokes them.
    pub fn set_grant(&mut self, id: &str, grant: Permissions) -> Result<Permissions> {
        let granted = self.plugins.get(id).ok_or_else(|| Error::NotFound(id.into()))?.effective();
        programs_kept(&grant, &granted)?;
        self.set_grant_by_user(id, grant)
    }

    /// [`Manager::set_grant`] as the user set it in the Plugin Manager: it may grant programs.
    /// Never reachable from a command.
    pub fn set_grant_by_user(&mut self, id: &str, grant: Permissions) -> Result<Permissions> {
        let p = self.plugins.get_mut(id).ok_or_else(|| Error::NotFound(id.into()))?;
        p.grant = grant.intersect(&p.plugin.manifest().permissions);
        let e = p.effective();
        p.push_log(format!("permissions set to {}", serde_json::to_string(&e).unwrap_or_default()));
        self.save()?;
        Ok(e)
    }

    pub fn get(&self, id: &str) -> Option<&Installed> {
        self.plugins.get(id)
    }

    /// Installed plug-ins, by id.
    pub fn list(&self) -> impl Iterator<Item = &Installed> {
        self.plugins.values()
    }

    fn save(&self) -> Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let mut plugins = self.kept.clone();
        plugins.extend(self.plugins.iter().map(|(id, p)| (id.clone(), Entry { enabled: p.enabled, grant: p.grant.clone() })));
        let state = State { plugins };
        let bytes = serde_json::to_vec_pretty(&state).map_err(|e| Error::Io(e.to_string()))?;
        write_atomic(&dir.join("plugins.json"), &bytes)
    }

    /// Sends `request` to plug-in `id` with its effective capabilities, `lent` files added for this
    /// call. Logs what it printed and how the call ended.
    pub fn invoke(&mut self, id: &str, request: &Value, host: &mut dyn HostApi, lent: &[PathBuf]) -> Result<Value> {
        let mut call = self.begin(id, request.clone(), lent.to_vec())?;
        let outcome = call.run(host);
        self.end(call, Some(outcome))
    }

    /// Prepares a call of plug-in `id` that runs without this manager ([`PendingCall::run`], e.g. on a
    /// worker thread while the manager's lock is free), then is handed back with [`Manager::end`]. The
    /// plug-in's data is lent to the call: until then, other calls of the same plug-in are refused as busy.
    pub fn begin(&mut self, id: &str, request: Value, lent: Vec<PathBuf>) -> Result<PendingCall> {
        self.begin_with_dirs(id, request, lent, Vec::new())
    }

    /// [`Manager::begin`] with folders lent to the call for reading.
    pub fn begin_with_dirs(&mut self, id: &str, request: Value, lent: Vec<PathBuf>, lent_dirs: Vec<PathBuf>) -> Result<PendingCall> {
        let library = self.library.clone();
        let dir = self.dir.clone();
        let p = self.plugins.get_mut(id).ok_or_else(|| Error::NotFound(id.into()))?;
        if !p.enabled {
            return Err(Error::Disabled(id.into()));
        }
        if p.busy {
            return Err(Error::Busy(id.into()));
        }
        if p.store.is_none() {
            p.store = Some(match &dir {
                Some(d) => NamespaceStore::open(d.join("data").join(format!("{id}.json")))?,
                None => NamespaceStore::in_memory(),
            });
        }
        if let Some(store) = p.store.as_mut() {
            scope(store, &library);
        }
        let perms = p.effective();
        let plugin = p.plugin.clone();
        let store = p.store.take().unwrap_or_default();
        p.busy = true;
        Ok(PendingCall { id: id.to_string(), plugin, perms, store, request, lent, lent_dirs, runs_root: self.runs_root.clone() })
    }

    /// The `program.end` call for a run of plug-in `id` ([`crate::programs`]): `{"hook": "program.end",
    /// "run", "program", "status", "code", "tmpDir"}` (`code`: the program's exit code, null when
    /// unknown), with the run's folder lent for reading. Run it like any [`Manager::begin`] call,
    /// then delete the folder once the program has exited too.
    pub fn program_end_call(
        &mut self,
        id: &str,
        run: &str,
        program: &str,
        status: crate::EndStatus,
        code: Option<i32>,
        dir: &Path,
    ) -> Result<PendingCall> {
        let req = json!({"hook": "program.end", "run": run, "program": program, "status": status.as_str(), "code": code, "tmpDir": dir});
        self.begin_with_dirs(id, req, Vec::new(), vec![dir.to_path_buf()])
    }

    /// Hands a call from [`Manager::begin`] back with how it went (`None`: it never ran, e.g.
    /// cancelled): its data is saved and returned to the plug-in, its log lines kept.
    pub fn end(&mut self, call: PendingCall, outcome: Option<crate::Outcome>) -> Result<Value> {
        let PendingCall { id, mut store, request, .. } = call;
        let saved = store.save();
        // uninstalled meanwhile: nothing to hand back to
        let Some(p) = self.plugins.get_mut(&id) else {
            saved?;
            return outcome.map_or_else(|| Err(Error::NotFound(id.clone())), |o| o.value);
        };
        p.store = Some(store);
        p.busy = false;
        let Some(outcome) = outcome else {
            saved?;
            return Err(Error::Failed("cancelled".into()));
        };
        let what = request.get("hook").and_then(Value::as_str).unwrap_or("call");
        for l in outcome.logs {
            p.push_log(l);
        }
        if let Err(e) = &outcome.value {
            p.push_log(format!("{what} failed: {e}"));
        }
        saved?;
        outcome.value
    }

    /// The calls an export makes, in order: each enabled export hook (`hook: "export"`) on the
    /// written files `[{path, photo?}]`, then the export destination `(plugin, destination, args)`
    /// when one is named. Each is `(plugin id, destination id for the destination call, its
    /// request and lent files or why it can't be made)`; nothing runs.
    #[allow(clippy::type_complexity)]
    pub fn export_calls(
        &self,
        files: &[Value],
        destination: Option<(&str, &str, &Value)>,
    ) -> Vec<(String, Option<String>, Result<(Value, Vec<PathBuf>)>)> {
        let files: Vec<&Value> = files.iter().filter(|f| f.get("path").and_then(Value::as_str).is_some()).take(MAX_BATCH).collect();
        let lent: Vec<PathBuf> = files.iter().filter_map(|f| f.get("path").and_then(Value::as_str)).map(PathBuf::from).collect();
        let mut out: Vec<_> = self
            .with_hook(|m| m.hooks.export)
            .into_iter()
            .map(|id| (id, None, Ok((json!({"hook": "export", "files": files}), lent.clone()))))
            .collect();
        if let Some((id, dest, args)) = destination {
            let req = self.destination_request(id, dest, args, &files);
            out.push((id.to_string(), Some(dest.to_string()), req.map(|r| (r, lent.clone()))));
        }
        out
    }

    fn destination_request(&self, id: &str, destination: &str, args: &Value, files: &[&Value]) -> Result<Value> {
        let p = self.plugins.get(id).ok_or_else(|| Error::NotFound(id.into()))?;
        let decl = p
            .plugin
            .manifest()
            .destination(destination)
            .ok_or_else(|| Error::Params(format!("plug-in {id} has no export destination {destination:?}")))?;
        let args = dialog_args(&decl.dialog, args)?;
        Ok(json!({"hook": "export.destination", "destination": destination, "args": args, "files": files}))
    }

    /// Runs a plug-in's menu command: `args` are checked against its dialog, `selection` are the
    /// selected photo ids.
    pub fn run_command(&mut self, id: &str, command: &str, args: &Value, selection: &[String], host: &mut dyn HostApi) -> Result<Value> {
        let p = self.plugins.get(id).ok_or_else(|| Error::NotFound(id.into()))?;
        let decl = p.plugin.manifest().command(command).ok_or_else(|| Error::Params(format!("plug-in {id} has no command {command:?}")))?;
        let args = dialog_args(&decl.dialog, args)?;
        let req = json!({"hook": "command", "command": command, "args": args, "selection": cap(selection)});
        self.invoke(id, &req, host, &[])
    }

    /// Runs every enabled export hook on the exported files `[{path, photo?}]`. Each hook may read
    /// and rewrite those files (lent for the call). Returns one result per plug-in; a failing
    /// hook doesn't stop the others.
    pub fn post_export(&mut self, files: &[Value], host: &mut dyn HostApi) -> Vec<Value> {
        let lent: Vec<PathBuf> = files.iter().filter_map(|f| f.get("path").and_then(Value::as_str)).map(PathBuf::from).take(MAX_BATCH).collect();
        let ids = self.with_hook(|m| m.hooks.export);
        let req = json!({"hook": "export", "files": files.iter().take(MAX_BATCH).collect::<Vec<_>>()});
        ids.into_iter().map(|id| outcome(&id, self.invoke(&id, &req, host, &lent))).collect()
    }

    /// Asks every enabled metadata provider about `photos` (photo JSON, as `photo.inspect`
    /// returns it). Each result is `{plugin, ok: {suggestions: [{photo, fields?, keywords?}]}}`
    /// or `{plugin, error}`; nothing is written.
    pub fn suggest_metadata(&mut self, photos: &[Value], host: &mut dyn HostApi) -> Vec<Value> {
        let ids = self.with_hook(|m| m.hooks.metadata);
        let req = json!({"hook": "metadata", "photos": photos.iter().take(MAX_BATCH).collect::<Vec<_>>()});
        ids.into_iter().map(|id| outcome(&id, self.invoke(&id, &req, host, &[]))).collect()
    }

    /// Enabled plug-ins' export destinations: `(plugin id, destination)`.
    pub fn export_destinations(&self) -> Vec<(String, crate::DestinationDecl)> {
        self.plugins
            .values()
            .filter(|p| p.enabled)
            .flat_map(|p| p.plugin.manifest().export_destinations.iter().map(|d| (p.plugin.id().to_string(), d.clone())))
            .collect()
    }

    /// Hands exported files `[{path, photo?}]` to a plug-in's export destination. `args` are
    /// checked against the destination's dialog; the files are lent to the call.
    pub fn export_to(&mut self, id: &str, destination: &str, args: &Value, files: &[Value], host: &mut dyn HostApi) -> Result<Value> {
        let files: Vec<&Value> = files.iter().filter(|f| f.get("path").and_then(Value::as_str).is_some()).take(MAX_BATCH).collect();
        let lent: Vec<PathBuf> = files.iter().filter_map(|f| f.get("path").and_then(Value::as_str)).map(PathBuf::from).collect();
        let req = self.destination_request(id, destination, args, &files)?;
        self.invoke(id, &req, host, &lent)
    }

    fn store_mut(&mut self, id: &str) -> Result<&mut Installed> {
        let library = self.library.clone();
        let dir = self.dir.clone();
        let p = self.plugins.get_mut(id).ok_or_else(|| Error::NotFound(id.into()))?;
        // its data is lent to a call running elsewhere: opening it again would fork it
        if p.busy {
            return Err(Error::Busy(id.into()));
        }
        if p.store.is_none() {
            p.store = Some(match &dir {
                Some(d) => NamespaceStore::open(d.join("data").join(format!("{id}.json")))?,
                None => NamespaceStore::in_memory(),
            });
        }
        if let Some(store) = p.store.as_mut() {
            scope(store, &library);
        }
        Ok(p)
    }

    /// The custom metadata fields of every enabled plug-in, with their values for `photo`
    /// (from each plug-in's namespace; no plug-in code runs). A plug-in whose data can't be read
    /// is listed with an `error`.
    pub fn metadata_fields(&mut self, photo: &str) -> Vec<Value> {
        let ids: Vec<String> = self.with_hook(|m| !m.metadata_fields.is_empty());
        let mut out = Vec::new();
        for id in ids {
            let r = self.store_mut(&id).map(|p| {
                let data = p.store.as_ref().map(|s| s.get(photo)).unwrap_or(Value::Null);
                let m = p.plugin.manifest();
                let fields: Vec<Value> = m
                    .metadata_fields
                    .iter()
                    .map(|f| {
                        let mut v = serde_json::to_value(f).unwrap_or(Value::Null);
                        // a stored value the field no longer accepts shows as empty
                        v["value"] = data.get(&f.key).and_then(|x| f.as_field().coerce(x).ok()).unwrap_or(Value::Null);
                        v
                    })
                    .collect();
                json!({"plugin": m.id, "pluginName": m.name, "fields": fields})
            });
            out.push(r.unwrap_or_else(|e| json!({"plugin": id, "error": e.to_string()})));
        }
        out
    }

    /// Every enabled plug-in's custom metadata field values, by photo: `photo → {"<plug-in>/<key>": value}`
    /// (values the field accepts; empty ones left out). A plug-in whose data can't be read (or that is
    /// busy) is skipped and named in the second list.
    pub fn field_values(&mut self) -> (BTreeMap<String, BTreeMap<String, Value>>, Vec<String>) {
        let ids: Vec<String> = self.with_hook(|m| !m.metadata_fields.is_empty());
        let mut out: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
        let mut skipped = Vec::new();
        for id in ids {
            let Ok(p) = self.store_mut(&id) else {
                skipped.push(id);
                continue;
            };
            let Some(store) = p.store.as_ref() else {
                skipped.push(id);
                continue;
            };
            let fields = &p.plugin.manifest().metadata_fields;
            for photo in store.photos() {
                let data = store.get(&photo);
                for f in fields {
                    if let Some(v) = data.get(&f.key).and_then(|x| f.as_field().coerce(x).ok()).filter(|v| !v.is_null()) {
                        out.entry(photo.clone()).or_default().insert(format!("{id}/{}", f.key), v);
                    }
                }
            }
        }
        (out, skipped)
    }

    /// Sets (or with `null`, clears) a custom metadata field the user edited. Read-only fields
    /// and values the field doesn't accept are refused.
    pub fn set_metadata_field(&mut self, id: &str, photo: &str, key: &str, value: &Value) -> Result<Value> {
        let p = self.plugins.get(id).ok_or_else(|| Error::NotFound(id.into()))?;
        if !p.enabled {
            return Err(Error::Disabled(id.into()));
        }
        let f = p.plugin.manifest().metadata_field(key).ok_or_else(|| Error::Params(format!("plug-in {id} has no metadata field {key:?}")))?;
        if f.read_only {
            return Err(Error::Params(format!("metadata field {key:?} is read-only")));
        }
        let v = if value.is_null() { Value::Null } else { f.as_field().coerce(value).map_err(Error::Params)? };
        let p = self.store_mut(id)?;
        let store = p.store.get_or_insert_with(NamespaceStore::in_memory);
        store.set(photo, key, v.clone()).map_err(Error::Params)?;
        store.save()?;
        Ok(v)
    }

    /// Restore a metadata field value read from an XMP sidecar (`text`, as the sidecar writes it) into
    /// plug-in `id`'s namespace for `photo`. Unlike [`Manager::set_metadata_field`] read-only fields
    /// are restored too (the plug-in set them). `Ok(false)`: nothing to do (no such plug-in or field,
    /// disabled, or the value is already there); a value the field doesn't take is an error.
    pub fn restore_metadata_field(&mut self, id: &str, photo: &str, key: &str, text: &str) -> Result<bool> {
        let Some(p) = self.plugins.get(id).filter(|p| p.enabled) else { return Ok(false) };
        let Some(f) = p.plugin.manifest().metadata_field(key) else { return Ok(false) };
        let v = f.as_field().from_text(text).map_err(Error::Params)?;
        let p = self.store_mut(id)?;
        let store = p.store.get_or_insert_with(NamespaceStore::in_memory);
        if store.get(photo).get(key) == Some(&v) {
            return Ok(false);
        }
        store.set(photo, key, v).map_err(Error::Params)?;
        store.save()?;
        Ok(true)
    }

    fn with_hook(&self, f: impl Fn(&crate::Manifest) -> bool) -> Vec<String> {
        self.plugins.values().filter(|p| p.enabled && f(p.plugin.manifest())).map(|p| p.plugin.id().to_string()).collect()
    }

    /// Enabled plug-ins that provide a publish service: `(plugin id, service)`.
    pub fn publish_services(&self) -> Vec<(String, crate::PublishDecl)> {
        self.plugins
            .values()
            .filter(|p| p.enabled)
            .filter_map(|p| p.plugin.manifest().publish.clone().map(|s| (p.plugin.id().to_string(), s)))
            .collect()
    }
}

/// Points `store` at `library`; data it claims from before is saved at once, so another library
/// can't claim it on the next start.
fn scope(store: &mut NamespaceStore, library: &str) {
    if store.set_scope(library)
        && let Err(e) = store.save()
    {
        log::warn!("plug-in data: {e}");
    }
}

fn cap(ids: &[String]) -> &[String] {
    ids.get(..MAX_BATCH).unwrap_or(ids)
}

fn outcome(id: &str, r: Result<Value>) -> Value {
    match r {
        Ok(v) => json!({"plugin": id, "ok": v}),
        Err(e) => json!({"plugin": id, "error": e.to_string()}),
    }
}

/// Reads a module file: a regular file within the size limit.
fn read_module(path: &Path, limits: &Limits) -> Result<Vec<u8>> {
    use std::io::Read;
    let io = |e: std::io::Error| Error::Io(format!("{}: {e}", path.display()));
    let meta = std::fs::metadata(path).map_err(io)?;
    if !meta.is_file() {
        return Err(Error::Io(format!("{} is not a file", path.display())));
    }
    if meta.len() > limits.max_module_bytes as u64 {
        return Err(Error::Module(format!("{} is {} bytes (limit {})", path.display(), meta.len(), limits.max_module_bytes)));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(io)?.take(limits.max_module_bytes as u64 + 1).read_to_end(&mut bytes).map_err(io)?;
    Ok(bytes)
}
