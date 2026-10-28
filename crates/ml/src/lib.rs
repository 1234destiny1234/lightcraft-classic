//! The shared backend the AI features run their models through (issue #491): semantic search embeddings (#487),
//! aesthetic scores (#488), culling suggestions (#489) and orientation detection (#490).
//!
//! - [`device`]: which device the jobs run on — the GPU when a usable one exists and the preference allows it, else
//!   the CPU, with the reason kept.
//! - [`queue`]: one [`Queue`] with a worker thread of its own, so a model never runs on the UI thread. Jobs are taken
//!   in priority order (someone waiting first, import-time work after), can be cancelled one by one or all at once,
//!   and fall back to the CPU when the GPU fails. A batch row ([`Rows`]) shows the work in the activity stack.
//!
//! The crate knows nothing about models, photos or the activity stack: a job is a closure, and the engine
//! (`lcc_engine::ml`) supplies the GPU probe and the row. It is pure `std` (no `unsafe`, no C), and never panics:
//! a job that panics ends as [`Outcome::Failed`], and the worker keeps going.
//!
//! See `docs/ml-backend.md`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod device;
pub mod queue;

pub use device::{Device, Kind, Preference, Probe, select};
pub use queue::{Config, Ctx, Finished, Job, JobError, Outcome, Priority, Queue, Row, Rows, Status, SubmitError, Work};

#[cfg(test)]
mod tests;
