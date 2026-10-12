//! End-to-end: synthesise a clip, export it trimmed with a grade, re-decode the result.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use dac_raster::Rgb32f as RgbFrame;
use dac_video_export::{ClipExportOptions, ClipFormat, ExportClipError, export_clip};
use filmcraft_isobmff::{AvcConfig, Brand, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions};

const W: u32 = 64;
const H: u32 = 48;
const FPS: u32 = 25;
const FRAMES: u32 = 50;
const SR: u32 = 48_000;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dac-video-export-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A 2 s, 25 fps, 64×48 H.264 clip of mid grey (Y=126, neutral chroma), optionally with a
/// 440 Hz stereo AAC track.
fn make_clip(path: &Path, audio: bool) {
    let mut cfg = filmcraft_h264enc::EncoderConfig::new(W, H, FPS, 1);
    cfg.format = filmcraft_h264enc::PacketFormat::LengthPrefixed;
    cfg.aud = false;
    cfg.keyint = 10;
    let mut enc = filmcraft_h264enc::Encoder::new(cfg).unwrap();
    let avc = AvcConfig::parse(&enc.avcc()).unwrap();
    let mut mux = Mp4Writer::new(BufWriter::new(File::create(path).unwrap()), WriterOptions::new(Brand::Mp4)).unwrap();
    let mut vt = TrackConfig::new(SampleEntry::avc(avc, W as u16, H as u16), FPS);
    if enc.delay() > 0 {
        vt.media_start = Some(1);
    }
    let vt = mux.add_track(vt).unwrap();
    let mut aac = audio.then(|| filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(SR, 2, 128_000)).unwrap());
    let at = aac.as_ref().map(|a| {
        let mut t = TrackConfig::new(SampleEntry::aac(a.audio_specific_config(), 2, SR), SR);
        t.media_start = Some(i64::from(a.priming_samples()));
        mux.add_track(t).unwrap()
    });
    let y = vec![126u8; (W * H) as usize];
    let c = vec![128u8; (W * H / 4) as usize];
    let write = |mux: &mut Mp4Writer<_>, ps: Vec<filmcraft_h264enc::Packet>| {
        for p in ps {
            mux.write_sample(vt, WriteSample { data: &p.data, duration: 1, composition_offset: (p.pts - p.dts) as i32, is_sync: p.keyframe })
                .unwrap();
        }
    };
    let spf = (SR / FPS) as usize;
    for i in 0..FRAMES {
        let ps = enc
            .encode(&filmcraft_h264enc::YuvFrame { y: &y, u: &c, v: &c, y_stride: W as usize, uv_stride: (W / 2) as usize }, i64::from(i))
            .unwrap();
        write(&mut mux, ps);
        if let (Some(a), Some(t)) = (aac.as_mut(), at) {
            let s: Vec<f32> = (0..spf).map(|k| ((i as usize * spf + k) as f32 * 440.0 * std::f32::consts::TAU / SR as f32).sin() * 0.5).collect();
            for au in a.encode(&[&s, &s]) {
                mux.write_sample(t, WriteSample { data: &au, duration: 1024, composition_offset: 0, is_sync: true }).unwrap();
            }
        }
    }
    let ps = enc.flush();
    write(&mut mux, ps);
    if let (Some(a), Some(t)) = (aac.as_mut(), at) {
        for au in a.flush() {
            mux.write_sample(t, WriteSample { data: &au, duration: 1024, composition_offset: 0, is_sync: true }).unwrap();
        }
    }
    mux.finish().unwrap();
}

/// Mean linear RGB of the decoded frame at `t`.
fn mean_rgb(path: &Path, t: f64) -> [f32; 3] {
    let f = dac_video::FrameReader::open(path).unwrap().frame_at(t, None).unwrap();
    let n = f.image.data.len() as f32;
    let mut m = [0f32; 3];
    for p in &f.image.data {
        for k in 0..3 {
            m[k] += p[k] / n;
        }
    }
    m
}

/// Mean linear RGB of the first ProRes frame (`dac-video` does not decode ProRes).
fn prores_mean_rgb(path: &Path) -> [f32; 3] {
    let src = File::open(path).unwrap();
    let file = filmcraft_isobmff::open(&src).unwrap();
    let ti = file.track_of_kind(filmcraft_isobmff::TrackKind::Video).unwrap();
    let f = filmcraft_prores::decode_frame(&file.read_sample(&src, ti, 0).unwrap()).unwrap();
    let mean = |v: &[u16]| v.iter().map(|x| f32::from(*x)).sum::<f32>() / v.len() as f32;
    let (y, cb, cr) = ((mean(&f.y) - 64.0) / 876.0, (mean(&f.cb) - 512.0) / 896.0, (mean(&f.cr) - 512.0) / 896.0);
    filmcraft_color::ycbcr_to_rgb(y, cb, cr, filmcraft_color::Matrix::Bt709).map(filmcraft_color::srgb_to_linear)
}

/// Peak of the first `secs` of the file's AAC track.
fn audio_peak(path: &Path, secs: f64) -> f32 {
    let src = File::open(path).unwrap();
    let file = filmcraft_isobmff::open(&src).unwrap();
    let ti = file.track_of_kind(filmcraft_isobmff::TrackKind::Audio).unwrap();
    let t = &file.tracks[ti];
    let Some(filmcraft_isobmff::CodecConfig::Aac(c)) = t.codec() else { panic!("not AAC") };
    let mut dec = filmcraft_aac::Decoder::new(&c.asc).unwrap();
    let mut peak = 0f32;
    for i in 0..t.samples.len() {
        if t.samples[i].pts as f64 / f64::from(t.timescale) > secs {
            break;
        }
        for ch in dec.decode(&file.read_sample(&src, ti, i).unwrap()).unwrap() {
            peak = ch.iter().fold(peak, |m, v| m.max(v.abs()));
        }
    }
    peak
}

fn opts(out: PathBuf, format: ClipFormat) -> ClipExportOptions {
    ClipExportOptions { format, quality: 80, max_side: None, out }
}

/// Halve red, keep green, double blue.
fn shift(f: &mut RgbFrame) {
    for p in &mut f.data {
        p[0] *= 0.5;
        p[2] *= 2.0;
    }
}

fn check_export(format: ClipFormat, audio: bool) {
    let dir = scratch(&format!("{format:?}-{audio}"));
    let src = dir.join("in.mp4");
    make_clip(&src, audio);
    let before = mean_rgb(&src, 1.0);
    let out = dir.join(format!("out.{}", format.extension()));
    let last = std::sync::Mutex::new(0.0);
    let rep =
        export_clip(&src, (0.4, Some(1.4)), &shift, &opts(out.clone(), format), &AtomicBool::new(false), &|p| *last.lock().unwrap() = p).unwrap();
    assert_eq!(rep.frames, 25);
    assert_eq!(rep.path, out);
    assert!((*last.lock().unwrap() - 1.0f64).abs() < 1e-9);
    assert!(!dir.join(format!("out.{}.partial", format.extension())).exists());

    let info = dac_video::probe(&out).unwrap();
    assert!((info.duration - 1.0).abs() <= 1.0 / f64::from(FPS) + 1e-6, "duration {}", info.duration);
    assert_eq!(info.has_audio, audio);
    if audio {
        let p = audio_peak(&out, 0.5);
        assert!(p > 0.2, "audio is silent: {p}");
    }
    let after = if format == ClipFormat::ProResMov { prores_mean_rgb(&out) } else { mean_rgb(&out, 0.5) };
    assert!((after[0] - before[0] * 0.5).abs() < 0.03, "red {before:?} -> {after:?}");
    assert!((after[1] - before[1]).abs() < 0.03, "green {before:?} -> {after:?}");
    assert!((after[2] - before[2] * 2.0).abs() < 0.05, "blue {before:?} -> {after:?}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn h264_export_honours_trim_and_grade_with_audio() {
    check_export(ClipFormat::H264Mp4, true);
}

#[test]
fn h264_export_without_audio() {
    check_export(ClipFormat::H264Mp4, false);
}

#[test]
fn prores_export_honours_trim_and_grade() {
    check_export(ClipFormat::ProResMov, true);
}

#[test]
fn max_side_downscales() {
    let dir = scratch("scale");
    let src = dir.join("in.mp4");
    make_clip(&src, false);
    let mut o = opts(dir.join("out.mp4"), ClipFormat::H264Mp4);
    o.max_side = Some(32);
    let rep = export_clip(&src, (0.0, None), &|_: &mut RgbFrame| {}, &o, &AtomicBool::new(false), &|_| {}).unwrap();
    assert_eq!((rep.width, rep.height, rep.frames), (32, 24, 50));
    let v = dac_video::probe(&dir.join("out.mp4")).unwrap();
    assert_eq!((v.width, v.height), (32, 24));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cancel_stops_early_and_leaves_no_file() {
    let dir = scratch("cancel");
    let src = dir.join("in.mp4");
    make_clip(&src, true);
    let out = dir.join("out.mp4");
    let cancel = AtomicBool::new(false);
    let graded = std::sync::atomic::AtomicUsize::new(0);
    let grade = |_: &mut RgbFrame| {
        if graded.fetch_add(1, Ordering::Relaxed) == 4 {
            cancel.store(true, Ordering::Relaxed);
        }
    };
    let r = export_clip(&src, (0.0, None), &grade, &opts(out.clone(), ClipFormat::H264Mp4), &cancel, &|_| {});
    assert!(matches!(r, Err(ExportClipError::Cancelled)), "{r:?}");
    assert_eq!(graded.load(Ordering::Relaxed), 5);
    assert!(!out.exists());
    assert!(!dir.join("out.mp4.partial").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn hostile_inputs_error() {
    let dir = scratch("hostile");
    let out = dir.join("out.mp4");
    let none = AtomicBool::new(false);
    let id = |_: &mut RgbFrame| {};
    let o = opts(out.clone(), ClipFormat::H264Mp4);
    // missing file
    assert!(export_clip(&dir.join("missing.mp4"), (0.0, None), &id, &o, &none, &|_| {}).is_err());
    // garbage
    let junk = dir.join("junk.mp4");
    std::fs::write(&junk, b"\0\0\0\x18ftypisom\0\0\0\0isomgarbage-garbage").unwrap();
    assert!(export_clip(&junk, (0.0, None), &id, &o, &none, &|_| {}).is_err());
    // truncated real clip
    let src = dir.join("in.mp4");
    make_clip(&src, true);
    let bytes = std::fs::read(&src).unwrap();
    for cut in [16, bytes.len() / 3, bytes.len() / 2, bytes.len() - 7] {
        let t = dir.join("trunc.mp4");
        std::fs::write(&t, &bytes[..cut]).unwrap();
        let _ = export_clip(&t, (0.0, None), &id, &o, &none, &|_| {});
    }
    // flipped bytes
    let mut seed = 0x2545_f491u32;
    for _ in 0..20 {
        let mut b = bytes.clone();
        for _ in 0..16 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let i = seed as usize % b.len();
            b[i] ^= (seed >> 8) as u8;
        }
        let t = dir.join("flip.mp4");
        std::fs::write(&t, &b).unwrap();
        let _ = export_clip(&t, (0.2, Some(0.6)), &id, &o, &none, &|_| {});
    }
    // a flipped file may still export: start the remaining checks without an output
    std::fs::remove_file(&out).ok();
    // bad trims
    for trim in [(1.5, Some(0.5)), (f64::NAN, None), (-1.0, None), (5.0, None), (0.0, Some(f64::INFINITY))] {
        assert!(matches!(export_clip(&src, trim, &id, &o, &none, &|_| {}), Err(ExportClipError::BadTrim(_))), "{trim:?}");
    }
    // a grade that changes the frame size
    let bad = |f: &mut RgbFrame| f.data.truncate(3);
    assert!(export_clip(&src, (0.0, Some(0.2)), &bad, &o, &none, &|_| {}).is_err());
    // a panicking grade becomes an error
    let boom = |_: &mut RgbFrame| panic!("boom");
    assert!(matches!(export_clip(&src, (0.0, Some(0.2)), &boom, &o, &none, &|_| {}), Err(ExportClipError::Internal)));
    assert!(!out.exists() && !dir.join("out.mp4.partial").exists());
    std::fs::remove_dir_all(&dir).ok();
}
