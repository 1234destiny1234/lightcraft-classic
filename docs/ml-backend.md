# The shared AI backend

The AI features planned in issue #453 — semantic search (#487), aesthetic scores (#488), culling suggestions (#489)
and orientation detection (#490) — run their models through one backend (issue #491), so a big import is worked
through once, in the background, on the best device Chromaforge can use, without the window waiting.

The plan this follows (runtime choice, models, storage, milestones) is the investigation posted on issue #491. This
page describes what exists.

## What exists

**`crates/ml` (`lcc-ml`, L3)** knows nothing about models, photos or the UI. A job is a closure.

- **Device** (`device.rs`): `Preference::Auto` takes the GPU when the probe finds one, else the CPU, with the reason
  kept (`Device::why_cpu`); `Preference::Cpu` never asks for the GPU. The probe runs on the worker thread before the
  first job (it may create a device), and a probe that panics means "no GPU".
- **Queue** (`queue.rs`): one worker thread (`ml-queue`) takes jobs in priority order (`Interactive` before
  `Background`), one at a time. Each job has a key (`"embed:1234"`): a job submitted again while it still waits
  replaces the waiting one, and the job's owner routes results by key prefix when it calls `Queue::drain`.
  - **Cancel:** `cancel(key)` drops a waiting job or tells the running one to stop; `cancel_all()` does it for
    everything. A running job checks `Ctx::cancelled()` / `Ctx::check()?` between steps (between batches or layers).
  - **CPU fallback:** a job that returns `JobError::Device` on the GPU runs again on the CPU, and the queue stays on
    the CPU (with the reason) until the preference is set again.
  - **Never crash:** a job that panics ends as `Outcome::Failed` and the worker goes on; full queues refuse more
    (`SubmitError::Full`); results nobody drains are capped (the oldest go, with a warning).
  - **No threads** (the browser build): the worker can't start, and `drain` runs one job in place each call.
  - Dropping the queue tells the running job to stop and doesn't wait for the thread.
- **Activity row** (`Rows` / `Row`): while the queue has work it holds one batch row; the row's ✕ sets the batch's
  cancel flag (the running job stops, everything waiting ends as cancelled), and the row goes when the queue is empty.

**The engine** (`crates/engine/src/ml.rs`) gives the session one queue, made on its first job:

- its GPU probe follows the rendering GPU's switches (`<PREFIX>_GPU`, `<PREFIX>_GPU_BACKEND`, `app.gpu`) and reports
  the adapter rendering uses;
- `<PREFIX>_ML_DEVICE=cpu` keeps the AI jobs on the CPU (`auto` is the default);
- its row in the activity stack is a batch: kind `mlIndex`, label "Indexing photos" ("Indexing photos · 37 of 200 ·
  2 failed"), detail the photo's file name;
- `ml.status {preference?: auto | cpu}` reports the device (and why it is the CPU), the jobs waiting, the running
  job's key and the counts, and sets the preference for the session.

No feature submits jobs yet, so nothing user-visible has changed.

## What's next

In the order of the plan on issue #491:

1. **M0 spike:** measure CLIP ViT-B/32 image throughput on candle (CPU, and Metal on macOS), the decode vs encode
   split on raw previews, and an orientation head on a frozen embedding. Post the numbers on the issue.
2. **Models in `lcc-ml`:** a model manifest and known-model table (the `lcc_faces::manifest` shape: size, SHA-256,
   licence, commercial flag), download through `lcc-fetch` with the licence dialog, an `Encoder` trait (image and text
   embeddings) with a candle CPU implementation (Metal on macOS) that reports device failures as
   `JobError::Device`, and hostile-input tests for weights and shapes.
3. **Indexing in the engine:** a pump that diffs the library against a per-model embedding cache beside the library
   (not in the catalog), feeds preview-size pictures in batches, paces itself like the face scan (`Pace`,
   `memory::work_gate`) and starts after an import; a backfill command.
4. **Features (#487–#490)** on the cached embeddings: search, scores, culling suggestions, orientation suggestions.
   Suggestions are proposals the user accepts, never silent writes.
5. **GPU on Linux and Windows** (wgpu), only if the measurements show the CPU is too slow; checked against the CPU
   result before it is trusted, as the denoise network's GPU path is.

## Testing

`crates/ml/src/tests.rs` covers the queue (priority, replacement, cancel one / all / from the row, CPU fallback,
panicking jobs, limits, the row's lifetime) with a fake probe; `crates/engine/src/tests_ml.rs` covers the session's
row in the activity stack and `ml.status`. Nothing touches a GPU driver: tests pass a probe that names a fake GPU.
