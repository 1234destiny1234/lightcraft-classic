//! Photo ▸ Enhance through the command layer: a Bayer DNG on disk → Raw Details / Super Resolution DNG next to it,
//! imported with the source's settings and stacked with it; hostile files give errors.

use lcc_color::Mat3;
use lcc_geom::Orientation;
use lcc_meta::Metadata;
use lcc_raw::{BlackLevel, Cfa, ColorData, DngWriteOptions, OpcodeLists, RawData, RawFormat, RawImage, Rect};
use serde_json::json;

use crate::Session;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lc-enhance-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A small RGGB DNG of edges and stripes.
fn bayer_dng(w: usize, h: usize) -> Vec<u8> {
    let cfa = Cfa::bayer("RGGB").unwrap();
    let data = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let lum = if (x / 3) % 2 == 0 { 0.7 } else { 0.2 } * if y > h / 2 { 1.0 } else { 0.6 };
            let tint = [0.9, 1.0, 0.7];
            (lum * tint[cfa.color_at(x, y) as usize] * 4000.0 + 64.0) as u16
        })
        .collect();
    let raw = RawImage {
        format: RawFormat::Dng,
        width: w,
        height: h,
        cpp: 1,
        data: RawData::U16(data),
        cfa: Some(cfa),
        bits: 12,
        black: BlackLevel::uniform(64.0),
        white: vec![4095.0],
        active_area: Rect::new(0, 0, w, h),
        crop: Rect::new(0, 0, w, h),
        orientation: Orientation::Normal,
        color: ColorData {
            illuminant: [21, 0],
            color_matrix: [Some(Mat3::IDENTITY), None],
            as_shot_neutral: Some([1.0, 1.0, 1.0]),
            ..Default::default()
        },
        wb_multipliers: None,
        linearized: false,
        pixel_aspect: 1.0,
        opcodes: OpcodeLists::default(),
        metadata: Metadata { make: Some("Test".into()), model: Some("Bayer".into()), ..Default::default() },
        lens_metadata: Default::default(),
    };
    lcc_raw::write_dng(&raw, &DngWriteOptions::default()).unwrap()
}

fn import(s: &mut Session, path: &std::path::Path) -> u64 {
    let r = s.execute("library.import", &json!({"paths": [path.to_string_lossy()]})).unwrap();
    r["imported"][0].as_u64().unwrap()
}

#[test]
fn raw_details_and_super_resolution_make_enhanced_dngs() {
    let dir = temp_dir("cmd");
    let src = dir.join("IMG_1.dng");
    std::fs::write(&src, bayer_dng(64, 48)).unwrap();
    let mut s = Session::new().with_fs();
    let id = import(&mut s, &src);
    s.execute("develop.set", &json!({"id": id, "values": {"light.exposure": 0.5}})).ok();

    let rd = s.execute("enhance.rawDetails", &json!({"ids": [id], "wait": true})).unwrap();
    let made = &rd["made"][0];
    let path = made["path"].as_str().unwrap();
    assert!(path.ends_with("IMG_1-Enhanced-RD.dng"), "{path}");
    let back = lcc_raw::decode(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!((back.width, back.height, back.cpp), (64, 48, 3), "a demosaiced linear DNG of the same size");
    let rd_id = lcc_catalog::PhotoId(made["id"].as_u64().unwrap());
    assert!(s.catalog.photo(rd_id).is_some(), "imported");

    let sr = s.execute("enhance.superResolution", &json!({"ids": [id], "wait": true})).unwrap();
    let path = sr["made"][0]["path"].as_str().unwrap();
    assert!(path.ends_with("IMG_1-Enhanced-SR.dng"), "{path}");
    let back = lcc_raw::decode(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!((back.width, back.height), (128, 96), "twice each side");

    // in the background: queued, then imported by the pump, with a row in the activity stack meanwhile
    let q = s.execute("enhance.rawDetails", &json!({"ids": [id]})).unwrap();
    assert_eq!(q["queued"], 1);
    assert!(s.activity.list().iter().any(|t| t.kind == "enhance"), "a row in the activity stack");
    let mut done = false;
    for _ in 0..600 {
        let v = s.execute("enhance.pump", &json!({})).unwrap();
        if v["made"].as_array().is_some_and(|m| !m.is_empty()) {
            done = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(done, "the background run finished");
    assert!(!s.activity.list().iter().any(|t| t.kind == "enhance"), "the row goes when it is done");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hostile_and_unsuitable_photos_give_errors() {
    let dir = temp_dir("bad");
    let mut s = Session::new().with_fs();
    // a PNG is not a raw: Raw Details refuses it, Super Resolution takes it
    let jpg = dir.join("a.png");
    let img = lcc_raster::Rgba8::from_fn(24, 16, |x, y| [(x * 10) as u8, (y * 15) as u8, 90, 255]);
    std::fs::write(&jpg, lcc_codecs::encode_png(&lcc_codecs::EncodeImage::rgba8(&img), &Default::default()).unwrap()).unwrap();
    let id = import(&mut s, &jpg);
    assert!(s.execute("enhance.rawDetails", &json!({"ids": [id], "wait": true})).is_err());
    let sr = s.execute("enhance.superResolution", &json!({"ids": [id], "wait": true})).unwrap();
    assert_eq!(sr["made"][0]["size"], json!([48, 32]));
    // a raw whose file was damaged after import
    let raw = dir.join("b.dng");
    std::fs::write(&raw, bayer_dng(32, 24)).unwrap();
    let id = import(&mut s, &raw);
    let mut bytes = std::fs::read(&raw).unwrap();
    bytes.truncate(bytes.len() / 3);
    for b in bytes.iter_mut().skip(200).step_by(7) {
        *b ^= 0x5a;
    }
    std::fs::write(&raw, &bytes).unwrap();
    for cmd in ["enhance.rawDetails", "enhance.superResolution"] {
        assert!(s.execute(cmd, &json!({"ids": [id], "wait": true})).is_err(), "{cmd}");
    }
    std::fs::remove_file(&raw).unwrap();
    assert!(s.execute("enhance.rawDetails", &json!({"ids": [id], "wait": true})).is_err(), "a missing file");
    assert!(s.execute("enhance.rawDetails", &json!({"ids": [999_999], "wait": true})).is_err(), "no such photo");
    let _ = std::fs::remove_dir_all(&dir);
}
