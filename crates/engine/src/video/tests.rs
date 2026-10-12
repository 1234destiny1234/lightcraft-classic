//! Video commands against a mock backend (synthetic frames, scripted clock).

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dac_catalog::{MediaKind, Op, Photo, PhotoId, Source};
use serde_json::{Value, json};

use super::{FrameImage, VideoBackend, VideoInfo};
use crate::Session;

struct Mock {
    frames: Mutex<Vec<f64>>,
    /// The session's activity stack, to see the task row while a frame decodes.
    activity: Mutex<Option<crate::activity::Activity>>,
    rows: Mutex<Vec<String>>,
}

impl VideoBackend for Mock {
    fn probe(&self, path: &Path) -> Result<VideoInfo, String> {
        if path.to_string_lossy().contains("broken") {
            return Err("not an MP4".into());
        }
        Ok(VideoInfo {
            duration: 10.0,
            fps_num: 25,
            fps_den: 1,
            width: 8,
            height: 6,
            codec: "h264".into(),
            has_audio: path.to_string_lossy().contains("sound"),
        })
    }
    fn can_play_audio(&self, path: &Path) -> bool {
        path.to_string_lossy().contains("sound")
    }
    fn frame_at(&self, _: &Path, t: f64, _: Option<u32>) -> Result<FrameImage, String> {
        self.frames.lock().unwrap().push(t);
        if let Some(a) = self.activity.lock().unwrap().as_ref() {
            self.rows.lock().unwrap().extend(a.list().into_iter().map(|r| r.kind.to_string()));
        }
        let v = (t / 10.0) as f32;
        Ok(FrameImage::filled(8, 6, [v, 0.5, 1.0 - v]))
    }
}

/// A session with clip 900 (and photo 901) active, a mock backend and a clock the test sets (ms).
fn session(dir: &Path) -> (Session, Arc<AtomicU64>) {
    let mut s = Session::with_demo();
    let clip = dir.join("clip.mp4").to_string_lossy().to_string();
    let mut p = Photo::new(PhotoId(900), Source::File { path: clip }, "clip.mp4", "MP4", 8, 6, "");
    p.kind = MediaKind::Video;
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    let ph = Photo::new(PhotoId(901), Source::File { path: "still.jpg".into() }, "still.jpg", "JPEG", 8, 6, "");
    s.catalog.apply(Op::AddPhoto { photo: Box::new(ph) }).unwrap();
    let mock = Arc::new(Mock { frames: Mutex::new(Vec::new()), activity: Mutex::new(Some(s.activity.clone())), rows: Mutex::new(Vec::new()) });
    s.video.backend = Some(mock);
    let ms = Arc::new(AtomicU64::new(0));
    let m = ms.clone();
    s.video.clock = Box::new(move || m.load(Ordering::SeqCst) as f64 / 1000.0);
    select(&mut s, 900);
    (s, ms)
}

fn select(s: &mut Session, id: u64) {
    s.execute("library.select", &json!({"ids": [id], "active": id})).unwrap();
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("dac-video-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn approx(a: &Value, b: f64) -> bool {
    (a.as_f64().unwrap() - b).abs() < 1e-9
}

#[test]
fn without_a_backend_video_commands_are_unavailable() {
    let (mut s, _) = session(&tmp("none"));
    s.video.backend = None;
    let e = s.execute("video.play", &json!({})).unwrap_err().to_string();
    assert!(e.contains("isn't available in this build"), "{e}");
}

#[test]
fn photos_that_arent_videos_are_refused() {
    let (mut s, _) = session(&tmp("photo"));
    select(&mut s, 901);
    for id in ["video.get", "video.play", "video.seek", "video.setTrim", "video.captureFrame"] {
        let e = s.execute(id, &json!({"time": 1, "in": 0, "out": 1})).unwrap_err().to_string();
        assert!(e.contains("isn't a video"), "{id}: {e}");
    }
}

#[test]
fn unreadable_clip_is_an_error() {
    let d = tmp("broken");
    let (mut s, _) = session(&d.join("broken"));
    let e = s.execute("video.get", &json!({})).unwrap_err().to_string();
    assert!(e.contains("can't read the video"), "{e}");
}

#[test]
fn play_pause_follow_the_clock_and_stop_at_the_end() {
    let (mut s, ms) = session(&tmp("play"));
    let g = run(&mut s, "video.get", json!({}));
    assert_eq!(g["playing"], false);
    assert!(approx(&g["current"], 0.0) && approx(&g["duration"], 10.0) && approx(&g["poster"], 1.0));
    run(&mut s, "video.play", json!({}));
    ms.store(2500, Ordering::SeqCst);
    let g = run(&mut s, "video.get", json!({}));
    assert!(g["playing"].as_bool().unwrap() && approx(&g["current"], 2.5), "{g}");
    let g = run(&mut s, "video.togglePlay", json!({}));
    assert_eq!(g["playing"], false);
    ms.store(4000, Ordering::SeqCst);
    assert!(approx(&run(&mut s, "video.get", json!({}))["current"], 2.5), "paused time doesn't move");
    run(&mut s, "video.togglePlay", json!({}));
    ms.store(60_000, Ordering::SeqCst);
    let g = run(&mut s, "video.get", json!({}));
    assert!(!g["playing"].as_bool().unwrap() && approx(&g["current"], 10.0), "stops at the end: {g}");
    // playing again at the end starts over
    run(&mut s, "video.play", json!({}));
    ms.store(61_000, Ordering::SeqCst);
    assert!(approx(&run(&mut s, "video.get", json!({}))["current"], 1.0));
}

#[test]
fn can_play_audio_needs_the_backend_and_a_sound_track() {
    let (mut s, _) = session(&tmp("mute"));
    assert_eq!(run(&mut s, "video.get", json!({}))["canPlayAudio"], false);
    let (mut s, _) = session(&tmp("sound"));
    let g = run(&mut s, "video.get", json!({}));
    assert_eq!((g["hasAudio"].clone(), g["canPlayAudio"].clone()), (json!(true), json!(true)));
}

#[test]
fn playback_can_follow_an_audio_device_clock() {
    let (mut s, ms) = session(&tmp("audioclock"));
    ms.store(3000, Ordering::SeqCst);
    run(&mut s, "video.play", json!({}));
    let audio = dac_audio_out::AudioClock::new(48_000);
    s.video.follow_audio_clock(audio.clone());
    // the system clock no longer drives the position; the device's frames do, without a jump
    ms.store(9000, Ordering::SeqCst);
    assert!(approx(&run(&mut s, "video.get", json!({}))["current"], 0.0));
    audio.advance(72_000);
    assert!(approx(&run(&mut s, "video.get", json!({}))["current"], 1.5));
    s.video.follow_system_clock();
    let c = run(&mut s, "video.get", json!({}))["current"].as_f64().unwrap();
    assert!((c - 1.5).abs() < 0.05, "{c}");
}

#[test]
fn playback_respects_the_trim_and_loops() {
    let (mut s, ms) = session(&tmp("trim"));
    run(&mut s, "video.setTrim", json!({"in": 2, "out": 4}));
    let g = run(&mut s, "video.get", json!({}));
    assert!(approx(&g["current"], 2.0) && approx(&g["trimIn"], 2.0) && approx(&g["trimOut"], 4.0), "{g}");
    run(&mut s, "video.play", json!({}));
    ms.store(5000, Ordering::SeqCst);
    let g = run(&mut s, "video.get", json!({}));
    assert!(!g["playing"].as_bool().unwrap() && approx(&g["current"], 4.0), "{g}");
    run(&mut s, "video.play", json!({"loop": true}));
    ms.store(5000 + 2500, Ordering::SeqCst);
    let g = run(&mut s, "video.get", json!({}));
    assert!(g["playing"].as_bool().unwrap() && approx(&g["current"], 2.5), "loops: {g}");
    // seeks are clamped into the trim
    run(&mut s, "video.pause", json!({}));
    assert!(approx(&run(&mut s, "video.seek", json!({"time": 9}))["current"], 4.0));
    assert!(approx(&run(&mut s, "video.seek", json!({"time": -5}))["current"], 2.0));
}

#[test]
fn seek_snaps_and_steps_move_whole_frames() {
    let (mut s, _) = session(&tmp("step"));
    let g = run(&mut s, "video.seek", json!({"time": 1.019}));
    assert!(approx(&g["current"], 1.0) && g["frame"] == 25.0, "{g}");
    let g = run(&mut s, "video.stepFrame", json!({"delta": 3}));
    assert!(approx(&g["current"], 28.0 / 25.0) && g["frame"] == 28.0, "{g}");
    let g = run(&mut s, "video.stepFrame", json!({}));
    assert_eq!(g["frame"], 29.0);
    for _ in 0..29 {
        run(&mut s, "video.stepFrame", json!({"delta": -1}));
    }
    assert_eq!(run(&mut s, "video.get", json!({}))["frame"], 0.0);
    // stepping past either end stops at it
    assert!(approx(&run(&mut s, "video.stepFrame", json!({"delta": -5}))["current"], 0.0));
    assert!(approx(&run(&mut s, "video.stepFrame", json!({"delta": 1_000_000}))["current"], 10.0));
    // stepping pauses
    run(&mut s, "video.seek", json!({"time": 1}));
    run(&mut s, "video.play", json!({}));
    assert_eq!(run(&mut s, "video.stepFrame", json!({}))["playing"], false);
}

#[test]
fn hostile_params_are_errors_not_panics() {
    let (mut s, _) = session(&tmp("hostile"));
    for (id, p) in [
        ("video.seek", json!({})),
        ("video.seek", json!({"time": "soon"})),
        ("video.stepFrame", json!({"delta": 1.5})),
        ("video.stepFrame", json!({"delta": i64::MAX})),
        ("video.stepFrame", json!({"delta": i64::MIN})),
        ("video.setTrim", json!({"in": 5, "out": 5})),
        ("video.setTrim", json!({"in": 6, "out": 2})),
        ("video.setTrim", json!({"in": -1, "out": 2})),
        ("video.setTrim", json!({"in": 0, "out": 1e300})),
        ("video.setTrim", json!({"out": 3})),
        ("video.setPoster", json!({"time": 11})),
        ("video.setPoster", json!({})),
        ("video.captureFrame", json!({"time": -0.5})),
        ("video.captureFrame", json!({"time": 1e308})),
    ] {
        assert!(s.execute(id, &p).is_err(), "{id} {p}");
    }
    assert!(s.undo.is_empty(), "no failed call left a history step");
    // a huge seek is clamped, not refused
    assert!(approx(&run(&mut s, "video.seek", json!({"time": 1e308}))["current"], 10.0));
}

#[test]
fn trim_and_poster_undo_and_redo() {
    let (mut s, _) = session(&tmp("undo"));
    run(&mut s, "video.setTrim", json!({"in": 1, "out": 9}));
    run(&mut s, "video.setPoster", json!({"time": 3.01}));
    let g = run(&mut s, "video.get", json!({}));
    assert!(approx(&g["poster"], 3.0) && g["posterSet"] == true && g["trimmed"] == true, "{g}");
    assert_eq!(s.undo.len(), 2);
    s.execute("edit.undo", &json!({})).unwrap();
    let g = run(&mut s, "video.get", json!({}));
    assert!(g["posterSet"] == false && approx(&g["poster"], 1.0) && approx(&g["trimIn"], 1.0), "{g}");
    s.execute("edit.undo", &json!({})).unwrap();
    let g = run(&mut s, "video.get", json!({}));
    assert!(approx(&g["trimIn"], 0.0) && approx(&g["trimOut"], 10.0) && g["trimmed"] == false, "{g}");
    s.execute("edit.redo", &json!({})).unwrap();
    s.execute("edit.redo", &json!({})).unwrap();
    let g = run(&mut s, "video.get", json!({}));
    assert!(approx(&g["trimIn"], 1.0) && approx(&g["trimOut"], 9.0) && approx(&g["poster"], 3.0), "{g}");
    // reset trim is one undoable step; resetting an untrimmed clip adds none
    run(&mut s, "video.resetTrim", json!({}));
    assert_eq!(run(&mut s, "video.get", json!({}))["trimmed"], false);
    let n = s.undo.len();
    run(&mut s, "video.resetTrim", json!({}));
    assert_eq!(s.undo.len(), n);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(approx(&run(&mut s, "video.get", json!({}))["trimIn"], 1.0));
}

#[test]
fn switching_clips_resets_the_player() {
    let (mut s, _) = session(&tmp("switch"));
    run(&mut s, "video.seek", json!({"time": 5}));
    select(&mut s, 901);
    select(&mut s, 900);
    // the same clip keeps its place
    assert!(approx(&run(&mut s, "video.get", json!({}))["current"], 5.0));
    s.video.player.photo = Some(PhotoId(12345));
    assert!(approx(&run(&mut s, "video.get", json!({}))["current"], 0.0));
}

#[test]
fn capture_frame_writes_a_jpeg_imports_and_stacks_it() {
    let d = tmp("capture");
    let (mut s, _) = session(&d);
    let mock = Arc::new(Mock { frames: Mutex::new(Vec::new()), activity: Mutex::new(Some(s.activity.clone())), rows: Mutex::new(Vec::new()) });
    s.video.backend = Some(mock.clone());
    let r = run(&mut s, "video.captureFrame", json!({"time": 2.0}));
    assert_eq!(*mock.rows.lock().unwrap(), ["video.captureFrame"], "the capture shows in the activity stack");
    assert_eq!(*mock.frames.lock().unwrap(), [2.0]);
    let path = r["path"].as_str().unwrap().to_string();
    assert!(path.ends_with("clip-0m02s000.jpg"), "{path}");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "a JPEG");
    let id = PhotoId(r["id"].as_u64().unwrap());
    assert!(s.catalog.photo(id).is_some());
    let st = s.catalog.stack_of(id).expect("stacked");
    assert!(st.photos.contains(&PhotoId(900)) && st.top() == PhotoId(900));
    assert_eq!(s.undo.last().map(|e| e.label.as_str()), Some("Capture Frame"));
    assert!(s.activity.list().is_empty(), "the task row is gone when done");
    // a second capture at the same time never overwrites
    select(&mut s, 900);
    let r2 = run(&mut s, "video.captureFrame", json!({"time": 2.0}));
    assert_ne!(r2["path"], r["path"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn capture_frame_bakes_the_clip_grade() {
    let mean = |path: &str| {
        let img = dac_codecs::decode(&std::fs::read(path).unwrap(), Default::default()).unwrap().to_srgb8();
        img.data.iter().map(|p| p[..3].iter().map(|&c| c as f64).sum::<f64>()).sum::<f64>() / img.data.len() as f64
    };
    let d = tmp("capture-grade");
    let (mut s, _) = session(&d);
    let plain = run(&mut s, "video.captureFrame", json!({"time": 2.0}));
    let plain_mean = mean(plain["path"].as_str().unwrap());
    let mut dev = (*s.catalog.photo(PhotoId(900)).unwrap().develop).clone();
    dev.light.exposure = 1.0;
    s.catalog.apply(Op::SetDevelop { id: PhotoId(900), settings: std::sync::Arc::new(dev), label: "Exposure".into(), edited: None }).unwrap();
    select(&mut s, 900);
    let graded = run(&mut s, "video.captureFrame", json!({"time": 2.0}));
    let graded_mean = mean(graded["path"].as_str().unwrap());
    assert!(graded_mean > plain_mean + 10.0, "{graded_mean} vs {plain_mean}");
    let new = s.catalog.photo(PhotoId(graded["id"].as_u64().unwrap())).unwrap();
    assert!(!new.is_edited(), "the grade is baked, the still starts at defaults");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn commands_are_listed_with_docs() {
    for id in [
        "video.get",
        "video.play",
        "video.pause",
        "video.togglePlay",
        "video.seek",
        "video.stepFrame",
        "video.setTrim",
        "video.resetTrim",
        "video.setPoster",
        "video.captureFrame",
    ] {
        let c = crate::cmd::find_command(id).unwrap_or_else(|| panic!("{id}"));
        assert!(!c.params.is_empty() && !c.label.is_empty());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_native_backend_reports_a_garbage_clip_as_an_error() {
    let d = tmp("native");
    let (mut s, _) = session(&d);
    std::fs::write(d.join("clip.mp4"), b"not really a movie").unwrap();
    s.video = super::VideoState::default();
    assert!(s.video.backend.is_some(), "native builds decode with dac-video");
    let e = s.execute("video.get", &json!({})).unwrap_err().to_string();
    assert!(e.contains("can't read the video"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}
