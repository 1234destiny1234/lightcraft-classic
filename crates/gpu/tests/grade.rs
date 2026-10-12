//! Video grading on the GPU matches the CPU grade (within 8-bit rounding), and the timing probe.

use std::sync::Arc;
use std::time::Instant;

use dac_develop::DevelopSettings;
use dac_pipeline::{Quality, grade};

fn settings() -> DevelopSettings {
    let mut s = DevelopSettings::default();
    s.light.exposure = 0.3;
    s.light.contrast = 15.0;
    s.light.highlights = -25.0;
    s.light.shadows = 20.0;
    s.color.vibrance = 10.0;
    s.wb.temp = 7000.0;
    s
}

#[test]
fn gpu_grade_matches_cpu() {
    let img = Arc::new(dac_scenes::demo_library()[0].render(480, 270));
    let s = settings();
    let (gpu, on_gpu) = dac_gpu::grade_frame(&img, &s, Quality::Full);
    let cpu = grade::grade_frame(&img, &s, Quality::Full);
    assert_eq!((gpu.width, gpu.height), (cpu.width, cpu.height));
    if !on_gpu {
        assert_eq!(gpu, cpu);
        return;
    }
    let mut sum = 0u64;
    for (a, b) in gpu.data.iter().zip(cpu.data.iter()) {
        for k in 0..3 {
            sum += a[k].abs_diff(b[k]) as u64;
        }
    }
    let mean = sum as f64 / (3 * gpu.data.len()) as f64;
    assert!(mean < 1.5, "mean abs diff {mean}");
}

/// Prints CPU/GPU grade times for 1080p and 4K, draft and full: `cargo test -p dac-gpu --release
/// --test grade -- --ignored --nocapture`.
#[test]
#[ignore]
fn grade_timings() {
    let s = settings();
    for (w, h) in [(1920, 1080), (3840, 2160)] {
        let img = Arc::new(dac_scenes::demo_library()[0].render(w, h));
        for q in [Quality::Draft, Quality::Full] {
            let time = |f: &dyn Fn()| {
                f();
                let n = 10;
                let t = Instant::now();
                for _ in 0..n {
                    f();
                }
                t.elapsed().as_secs_f64() * 1e3 / n as f64
            };
            let cpu = time(&|| {
                grade::grade_frame(&img, &s, q);
            });
            let on = dac_gpu::grade_frame(&img, &s, q).1;
            let gpu = time(&|| {
                dac_gpu::grade_frame(&img, &s, q);
            });
            println!("{w}x{h} {q:?}: cpu {cpu:.1} ms ({:.0} fps), gpu {gpu:.1} ms ({:.0} fps, on_gpu={})", 1e3 / cpu, 1e3 / gpu, on);
        }
    }
}
