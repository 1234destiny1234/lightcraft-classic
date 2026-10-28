//! The shared AI queue in the session (#491): its row in the activity stack, cancelling from the row, the device it
//! reports, and `ml.status`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lcc_ml::{Config, Ctx, Job, JobError, Outcome, Preference};
use serde_json::json;

use crate::Session;
use crate::ml::{Output, ROW_KIND, ROW_LABEL};

fn wait_for(mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// A session whose queue sees a GPU called "Test GPU" (no driver touched).
fn session() -> Session {
    let mut s = Session::new();
    s.ml_queue_with(Config::default(), Box::new(|| Ok("Test GPU".into())));
    s
}

/// A job that runs until `open` is set or it is cancelled.
fn held(key: &str, open: Arc<AtomicBool>, started: Arc<AtomicBool>) -> Job<Output> {
    Job::new(key, move |ctx: &Ctx| -> Result<Output, JobError> {
        started.store(true, Ordering::SeqCst);
        while !open.load(Ordering::SeqCst) {
            ctx.check()?;
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(Box::new(1u32))
    })
    .name("IMG_0001.CR3")
}

fn rows(s: &Session) -> Vec<crate::activity::TaskInfo> {
    s.activity.list().into_iter().filter(|t| t.kind == ROW_KIND).collect()
}

#[test]
fn a_busy_queue_has_one_cancellable_row_that_goes_when_done() {
    let mut s = session();
    let (open, started) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    s.ml_submit(held("test:1", open.clone(), started.clone())).unwrap();
    s.ml_submit(Job::new("test:2", |_: &Ctx| Ok(Box::new(2u32) as Output))).unwrap();
    wait_for(|| started.load(Ordering::SeqCst));
    let r = rows(&s);
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].label, ROW_LABEL);
    assert!(r[0].cancellable);
    let st = s.execute("ml.status", &json!({})).unwrap();
    assert_eq!(st["running"], "test:1");
    assert_eq!(st["pending"], 1);
    assert_eq!(st["device"]["kind"], "gpu");

    open.store(true, Ordering::SeqCst);
    let mut done = Vec::new();
    wait_for(|| {
        done.extend(s.ml_queue().drain());
        done.len() == 2 && rows(&s).is_empty()
    });
    let two = done.iter().find(|f| f.key == "test:2").unwrap();
    match &two.outcome {
        Outcome::Done(v) => assert_eq!(v.downcast_ref::<u32>(), Some(&2)),
        o => panic!("{o:?}"),
    }
    // the batch left its summary
    let summary = s.activity.finished().into_iter().find(|b| b.kind == ROW_KIND).unwrap();
    assert_eq!((summary.done, summary.failed, summary.cancelled), (2, 0, false));
}

#[test]
fn the_rows_cancel_stops_the_batch() {
    let mut s = session();
    let (open, started) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    s.ml_submit(held("test:1", open, started.clone())).unwrap();
    for i in 0..3 {
        s.ml_submit(Job::new(format!("test:w{i}"), |_: &Ctx| Ok(Box::new(0u32) as Output))).unwrap();
    }
    wait_for(|| started.load(Ordering::SeqCst));
    let id = rows(&s)[0].id;
    s.activity.cancel(id).unwrap();
    let mut done = Vec::new();
    wait_for(|| {
        done.extend(s.ml_queue().drain());
        done.len() == 4 && rows(&s).is_empty()
    });
    assert!(done.iter().all(|f| matches!(f.outcome, Outcome::Cancelled)));
    let summary = s.activity.finished().into_iter().find(|b| b.kind == ROW_KIND).unwrap();
    assert!(summary.cancelled);
    // the queue takes new work afterwards
    s.ml_submit(Job::new("test:after", |_: &Ctx| Ok(Box::new(0u32) as Output))).unwrap();
    wait_for(|| s.ml_queue().drain().iter().any(|f| f.key == "test:after" && matches!(f.outcome, Outcome::Done(_))));
}

#[test]
fn a_job_failing_on_the_gpu_finishes_on_the_cpu_and_status_says_why() {
    let mut s = session();
    s.ml_submit(Job::new(
        "test:gpu",
        |ctx: &Ctx| {
            if ctx.device().is_gpu() { Err(JobError::Device("device lost".into())) } else { Ok(Box::new(()) as Output) }
        },
    ))
    .unwrap();
    wait_for(|| s.ml_queue().drain().iter().any(|f| matches!(f.outcome, Outcome::Done(_))));
    let st = s.execute("ml.status", &json!({})).unwrap();
    assert_eq!(st["device"]["kind"], "cpu");
    assert!(st["device"]["whyCpu"].as_str().unwrap().contains("device lost"));
}

#[test]
fn ml_status_before_any_job_starts_nothing() {
    let mut s = Session::new();
    let st = s.execute("ml.status", &json!({})).unwrap();
    assert!(st["device"].is_null());
    assert_eq!(st["pending"], 0);
    assert_eq!(st["threaded"], false);
}

#[test]
fn ml_status_sets_the_preference_and_refuses_junk() {
    let mut s = session();
    let st = s.execute("ml.status", &json!({"preference": "cpu"})).unwrap();
    assert_eq!(st["preference"], "cpu");
    s.ml_submit(Job::new("test:k", |ctx: &Ctx| Ok(Box::new(ctx.device().kind) as Output))).unwrap();
    wait_for(|| {
        s.ml_queue().drain().iter().any(|f| match &f.outcome {
            Outcome::Done(v) => v.downcast_ref::<lcc_ml::Kind>() == Some(&lcc_ml::Kind::Cpu),
            _ => false,
        })
    });
    for bad in [json!({"preference": "tpu"}), json!({"preference": 3})] {
        assert!(s.execute("ml.status", &bad).is_err());
    }
    assert_eq!(s.execute("ml.status", &json!({})).unwrap()["preference"], Preference::Cpu.as_str());
}
