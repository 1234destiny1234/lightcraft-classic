//! Pixel-filter plug-ins end to end through engine commands: install, list, add at both stages,
//! render, undo, missing plug-in, analyze without `pc_analyze`.

use serde_json::{Value, json};

use crate::Session;

fn invert_module(id: &str) -> Vec<u8> {
    let manifest = format!(
        r#"{{"id":"{id}","name":"Invert","version":"1","kind":"filter","params":{{"amount":{{"type":"number","min":0,"max":1,"default":1}}}}}}"#
    );
    let src = format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const 16) "{m}")
  (func (export "pc_abi_version") (result i32) (i32.const 1))
  (func (export "pc_manifest") (result i64) (i64.or (i64.shl (i64.const {len}) (i64.const 32)) (i64.const 16)))
  (func (export "pc_alloc") (param $n i32) (result i32) (local $old i32)
    (local.set $old (memory.grow (i32.shr_u (i32.add (local.get $n) (i32.const 65535)) (i32.const 16))))
    (if (result i32) (i32.eq (local.get $old) (i32.const -1)) (then (i32.const 0)) (else (i32.mul (local.get $old) (i32.const 65536)))))
  (func (export "pc_filter") (param $buf i32) (param $len i32) (param i32 i32 i32 i32 i32 i32) (result i32) (local $i i32) (local $p i32)
    (block $d (loop $l
      (br_if $d (i32.ge_u (local.get $i) (local.get $len)))
      (local.set $p (i32.add (local.get $buf) (local.get $i)))
      (f32.store (local.get $p) (f32.sub (f32.const 1) (f32.load (local.get $p))))
      (local.set $i (i32.add (local.get $i) (i32.const 4)))
      (br $l)))
    (i32.const 0)))"#,
        m = manifest.replace('"', "\\\""),
        len = manifest.len()
    );
    wat::parse_str(&src).unwrap()
}

fn install(s: &mut Session, id: &str) -> Value {
    let dir = std::env::temp_dir().join(format!("lcc-filter-test-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("f.wasm");
    std::fs::write(&path, invert_module(id)).unwrap();
    let r = s.execute("plugin.install", &json!({"path": path})).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    r
}

fn render(s: &mut Session) -> lcc_pipeline::Rendered {
    let id = s.active().unwrap();
    let src = s.source_now(id, crate::media::SourceLevel::Thumb).unwrap();
    let info = s.source_info(id);
    let d = s.develop_of(id).unwrap();
    lcc_pipeline::render(&src, &info, &d, &lcc_pipeline::RenderRequest::fit(64, 64))
}

fn mean(i: &lcc_raster::Rgba8) -> f64 {
    i.data.iter().map(|p| (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0).sum::<f64>() / i.data.len() as f64
}

#[test]
fn install_list_add_render_undo() {
    let mut s = Session::with_demo();
    let info = install(&mut s, "test.engine.invert");
    assert_eq!(info["kind"], "filter");
    let listed = s.execute("plugin.list", &json!({})).unwrap();
    let f = listed["plugins"].as_array().unwrap().iter().find(|p| p["id"] == "test.engine.invert").unwrap().clone();
    assert_eq!(f["kind"], "filter");
    assert!(f["params"]["amount"].is_object());
    assert_eq!(f["analyze"], false);
    assert!(f["moduleHash"].as_str().unwrap().len() == 64);

    let before = render(&mut s);
    let r = s.execute("develop.pluginFilter.add", &json!({"plugin": "test.engine.invert", "stage": "display"})).unwrap();
    let uid = r["uid"].as_u64().unwrap();
    let stack = s.execute("develop.pluginFilter.list", &json!({})).unwrap();
    assert_eq!(stack["filters"][0]["params"]["amount"], 1.0, "defaults filled from the manifest");
    let after = render(&mut s);
    assert!(after.warnings.is_empty(), "{:?}", after.warnings);
    assert!((mean(&before.image) + mean(&after.image) - 255.0).abs() < 2.0, "display invert");
    // analysis is unsupported for this module
    assert!(s.execute("develop.pluginFilter.analyze", &json!({"uid": uid})).is_err());
    // a wrong value type is refused by the manifest
    assert!(s.execute("develop.pluginFilter.add", &json!({"plugin": "test.engine.invert", "params": {"amount": "x"}})).is_err());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(render(&mut s).image.data, before.image.data, "undo removes the effect");

    // Source stage: the develop pipeline acts on the inverted source
    s.execute("develop.pluginFilter.add", &json!({"plugin": "test.engine.invert", "stage": "source"})).unwrap();
    let src = render(&mut s);
    assert!(src.warnings.is_empty());
    assert_ne!(src.image.data, before.image.data);

    // uninstalled: renders unfiltered with a warning
    s.execute("plugin.uninstall", &json!({"id": "test.engine.invert"})).unwrap();
    let gone = render(&mut s);
    assert!(!gone.warnings.is_empty());
    assert_eq!(gone.image.data, before.image.data);
}
