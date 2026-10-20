//! Start-up and the frame loop of the Android app.

use std::path::PathBuf;
use std::sync::Arc;

use lcc_android_host::{AndroidApp, Host, PickKind, PickRequest};
use lcc_engine::Session;
use lcc_ui_egui::panels::library_problem::LibraryProblem;
use lcc_ui_egui::{LccApp, Services};

use crate::prefs;

/// How long closing waits for renders that are running (as on the desktop).
const EXIT_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

pub fn main(activity: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag(lcc_brand::BINARY));
    let internal = activity.internal_data_path();
    let external = activity.external_data_path();
    let Some(config_root) = internal.clone().or_else(|| external.clone()) else {
        log::error!("the activity has no data folder; nothing can be saved");
        return;
    };
    // settings in the app-private folder; the library where the Files app can reach it
    lcc_brand::set_platform_dirs(lcc_brand::PlatformDirs {
        config_root: config_root.clone(),
        pictures: external.unwrap_or_else(|| config_root.clone()),
    });
    lcc_engine::guard::install_hook(config_root.join(format!("{}-panics.log", lcc_brand::BINARY)));

    let host = match Host::new(&activity) {
        Ok(h) => Some(Arc::new(h)),
        Err(e) => {
            log::error!("no pickers or links: {e}");
            None
        }
    };
    let prefs_path = prefs::path();
    let loaded = prefs::load(prefs_path.as_deref());
    if let Some(ui) = &loaded.ui {
        lcc_ui_egui::i18n::set_language(ui.language);
    }
    let library = lcc_engine::library::default_dir();

    let options = eframe::NativeOptions { android_app: Some(activity), renderer: eframe::Renderer::Wgpu, ..Default::default() };
    let result = eframe::run_native(
        lcc_brand::DISPLAY_NAME,
        options,
        Box::new(move |cc| {
            let (session, problem) = open_session(library);
            let mut app = LccApp::new(session.with_default_sam3_model(), services(cc, host.clone()));
            if let Some(ui) = loaded.ui {
                app.ui = ui;
            }
            lcc_ui_egui::i18n::set_language(app.ui.language);
            app.notices.extend(loaded.warning);
            if let Some(p) = problem {
                app.library_problem = Some(p);
            } else {
                lcc_ui_egui::panels::empty_library::open_on_empty_library(&mut app);
            }
            let writer = prefs::Writer::new(prefs_path, &app, loaded.keep_file);
            Ok(Box::new(App { app, writer, host }))
        }),
    );
    if let Err(e) = result {
        log::error!("the app stopped: {e}");
    }
}

/// The library session in the default folder (`<external data>/<library name>`).
fn open_session(dir: Option<PathBuf>) -> (Session, Option<LibraryProblem>) {
    let fresh = || {
        let mut s = Session::new().with_fs().with_default_denoise_models().with_default_connections().with_system_clock();
        s.face_models_dir = lcc_engine::config::default_face_models_dir();
        s.lens_db = lcc_engine::lens_download::LensSettings::load(lcc_engine::config::config_dir());
        s.slide_templates_dir = lcc_engine::config::config_dir().map(|d| d.join(lcc_engine::slide_template::DIR_NAME));
        s
    };
    let Some(dir) = dir else {
        return (fresh(), Some(LibraryProblem::new("", "There is no folder to keep the library in.")));
    };
    let mut s = fresh();
    match s.open_library(&dir, false) {
        Ok(r) => {
            let replayed = r.replayed;
            log::info!("library {}: {} photos, {replayed} log records replayed", dir.display(), s.catalog.len());
            (s, None)
        }
        Err(e) => {
            log::error!("can't open library {}: {e}", dir.display());
            (fresh(), Some(LibraryProblem::new(dir.to_string_lossy(), e.to_string())))
        }
    }
}

/// What the activity does for the shared UI.
fn services(cc: &eframe::CreationContext<'_>, host: Option<Arc<Host>>) -> Services {
    if let Some(h) = &host {
        let ctx = cc.egui_ctx.clone();
        h.set_waker(Box::new(move || ctx.request_repaint()));
    }
    let picker_host = host.clone();
    let url_host = host;
    Services {
        // the system picker (Storage Access Framework); the activity copies what is picked into
        // the app's folders and answers with paths
        picker: picker_host.map(|h| -> lcc_ui_egui::pick::Picker {
            Box::new(move |req: lcc_ui_egui::pick::PickRequest| {
                use lcc_ui_egui::pick::PickKind as Ui;
                let kind = match req.kind {
                    Ui::Files => PickKind::Files,
                    Ui::File => PickKind::File,
                    Ui::Folder => PickKind::Folder,
                    Ui::Save => PickKind::Save,
                };
                let mime_types = req.filter.as_ref().map(|(_, exts)| PickRequest::mime_types_for(exts)).unwrap_or_default();
                h.pick(&PickRequest { kind, mime_types, file_name: req.file_name.clone() }).map_err(|e| e.to_string())
            })
        }),
        open_url: url_host.map(|h| -> lcc_ui_egui::OpenUrlFn { Box::new(move |url: &str| h.open_url(url).map_err(|e| e.to_string())) }),
        // atomic (temp file + sync + rename): a failed write never leaves a truncated file
        write_shared: Some(Arc::new(lcc_engine::export::write_file)),
        write: Some(Box::new(lcc_engine::export::write_file)),
        png: Some(Box::new(|img: &lcc_raster::Rgba8| {
            lcc_codecs::encode_png(&lcc_codecs::EncodeImage::rgba8(img), &lcc_codecs::EncodeMeta::default()).unwrap_or_default()
        })),
        ..Default::default()
    }
}

struct App {
    app: LccApp,
    writer: prefs::Writer,
    host: Option<Arc<Host>>,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.logic(ctx);
        let now = ctx.input(|i| i.time);
        self.writer.tick(&mut self.app, now);
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        // photos opened, shared or dropped from other apps take the desktop's drop path: presets
        // import as presets, photos open the import review
        if let Some(h) = &self.host {
            raw.dropped_files.extend(h.take_opened().into_iter().map(|p| -> egui::DroppedFileHandle { Arc::new(SharedFile(p.into())) }));
        }
        self.app.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.perf.host_frame_ms = frame.info().cpu_usage.map(|s| f64::from(s) * 1000.0);
        self.app.ui(ui);
    }
    fn on_exit(&mut self) {
        if let Err(e) = self.writer.save(&self.app) {
            log::error!("{e}");
        }
        if let Err(e) = self.app.session.close_library() {
            log::error!("saving the library failed: {e}");
        }
        if !self.app.shutdown(EXIT_WAIT) {
            log::warn!("close: background work was still running after {EXIT_WAIT:?}");
        }
    }
}

/// A photo another app opened, shared or dropped in, as the activity copied it.
#[derive(Debug)]
struct SharedFile(PathBuf);

impl egui::DroppedFile for SharedFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| format!("{}: {e}", self.0.display()))
    }
}
