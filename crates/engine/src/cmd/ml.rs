//! The shared AI backend (#491): `ml.status`. The queue itself is `crate::ml`.

use super::{CommandSpec, cmd};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        query "ml.status",
        "AI Backend Status",
        [],
        None,
        "{preference?: auto | cpu} → {device: {kind: gpu | cpu, name, whyCpu} | null (until the first job), preference, pending, running (the running job's key), done, failed, cancelled, threaded} — the shared queue the AI features (search embeddings, scores, culling, orientation) run their models through, on a worker thread with one `mlIndex` row in the activity stack while busy (its ✕ cancels the batch). `preference: cpu` keeps the jobs on the CPU; `auto` (the default, or `{ENV_PREFIX}_ML_DEVICE`) takes the GPU when GPU rendering may use one, and a job that fails on the GPU runs again on the CPU",
        super::always,
        |s, p| s.ml_status(p)
    )]
}
