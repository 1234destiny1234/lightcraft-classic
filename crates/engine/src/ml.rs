//! The session's side of the shared AI backend (`lcc_ml`, issue #491): its one job queue, the GPU probe it uses (the
//! same switches as GPU rendering: `{ENV_PREFIX}_GPU`, `{ENV_PREFIX}_GPU_BACKEND`, `app.gpu`) and its batch row in the
//! activity stack ("Indexing photos · 37 of 200", kind `mlIndex`; its ✕ cancels the batch).
//!
//! `{ENV_PREFIX}_ML_DEVICE=cpu` keeps the AI jobs on the CPU (`auto`, the default, takes the GPU when there is one);
//! `ml.status {preference}` changes it for the session.
//!
//! No feature submits jobs yet: the embeddings (#487), scores (#488), culling (#489) and orientation (#490) jobs will,
//! each draining its own results by key prefix. See `docs/ml-backend.md`.

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use lcc_ml::{Config, Device, Job, Preference, Probe, Queue, Row, Rows, Status, SubmitError};
use serde_json::{Value, json};

use crate::Session;
use crate::activity::{Activity, Cancel, TaskGuard};

/// What a job hands back; its owner (known by the job's key) downcasts it.
pub(crate) type Output = Box<dyn Any + Send>;

/// The activity row's kind and label.
pub(crate) const ROW_KIND: &str = "mlIndex";
pub(crate) const ROW_LABEL: &str = "Indexing photos";

#[derive(Default)]
pub(crate) struct State {
    /// Made on the first job (no thread, no GPU probe before).
    queue: Option<Queue<Output>>,
    /// Set by `ml.status {preference}` before the queue exists.
    preference: Option<Preference>,
}

/// The preference from `{ENV_PREFIX}_ML_DEVICE` (`auto` when unset or not understood).
fn env_preference() -> Preference {
    lcc_brand::env("ML_DEVICE").and_then(|v| Preference::parse(&v.to_lowercase())).unwrap_or_default()
}

/// The GPU the AI jobs may use, or why not: the rendering GPU's switches apply.
fn gpu_probe() -> Result<String, String> {
    if let Some(why) = lcc_gpu::env_off_reason() {
        return Err(why);
    }
    if !lcc_gpu::enabled() {
        return Err("disabled by the GPU rendering preference (app.gpu)".into());
    }
    lcc_gpu::adapter_name().ok_or_else(|| lcc_gpu::unavailable_reason().unwrap_or_else(|| "no usable GPU".into()))
}

struct ActivityRows(Activity);

struct ActivityRow(TaskGuard);

impl Rows for ActivityRows {
    fn begin(&self, cancel: Arc<AtomicBool>) -> Box<dyn Row> {
        Box::new(ActivityRow(self.0.start_batch(ROW_KIND, ROW_LABEL, Cancel::Flag(cancel))))
    }
}

impl Row for ActivityRow {
    fn progress(&self, done: u64, total: u64) {
        self.0.progress(done, total);
    }
    fn detail(&self, name: &str) {
        self.0.detail(name);
    }
    fn item_done(&self) {
        self.0.item_done();
    }
    fn item_failed(&self, why: &str) {
        self.0.item_failed(why);
    }
}

fn device_json(d: &Device) -> Value {
    json!({"kind": d.kind.as_str(), "name": d.name, "whyCpu": d.why_cpu})
}

pub(crate) fn status_json(st: &Status) -> Value {
    json!({
        "device": st.device.as_ref().map(device_json),
        "preference": st.preference.as_str(),
        "pending": st.pending,
        "running": st.running,
        "done": st.done,
        "failed": st.failed,
        "cancelled": st.cancelled,
        "threaded": st.threaded,
    })
}

impl Session {
    /// The queue, made on first use with the engine's GPU probe and activity row.
    pub(crate) fn ml_queue(&mut self) -> &Queue<Output> {
        let preference = self.ml.preference.unwrap_or_else(env_preference);
        self.ml_queue_with(Config { preference, ..Config::default() }, Box::new(gpu_probe))
    }

    /// The queue, made with `probe` and `config` if it doesn't exist yet (tests pick the GPU this way).
    pub(crate) fn ml_queue_with(&mut self, config: Config, probe: Probe) -> &Queue<Output> {
        let activity = self.activity.clone();
        self.ml.queue.get_or_insert_with(|| Queue::new(config, probe, Some(Arc::new(ActivityRows(activity)) as Arc<dyn Rows>)))
    }

    /// Queue an AI job.
    #[allow(dead_code)] // the first callers are the features of #487-#490
    pub(crate) fn ml_submit(&mut self, job: Job<Output>) -> Result<(), SubmitError> {
        self.ml_queue().submit(job)
    }

    /// `ml.status {preference?}`.
    pub(crate) fn ml_status(&mut self, p: &Value) -> crate::Result<Value> {
        if let Some(v) = p.get("preference") {
            let pref = v
                .as_str()
                .and_then(Preference::parse)
                .ok_or_else(|| crate::cmd::bad("ml.status", "preference is `auto` (the GPU when there is one) or `cpu`"))?;
            self.ml.preference = Some(pref);
            if let Some(q) = &self.ml.queue {
                q.set_preference(pref);
            }
        }
        Ok(match &self.ml.queue {
            Some(q) => status_json(&q.status()),
            None => status_json(&Status {
                device: None,
                preference: self.ml.preference.unwrap_or_else(env_preference),
                pending: 0,
                running: None,
                done: 0,
                failed: 0,
                cancelled: 0,
                threaded: false,
            }),
        })
    }
}
