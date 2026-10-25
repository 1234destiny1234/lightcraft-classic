//! M11.4 end to end: `calibrate-lightroom add` refuses damaged or foreign files with an error exit (never a panic)
//! and leaves the repository untouched; bad arguments are errors too.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_app-cli");

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lcc-cli-lrcal-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("assets")).unwrap();
    std::fs::write(d.join("assets/ATTRIBUTION.md"), "| Path | Asset |\n|---|---|\n").unwrap();
    d
}

#[test]
fn add_refuses_hostile_files_without_panicking() {
    let repo = scratch("add");
    let inputs: [&[u8]; 6] =
        [b"", b"{bad", b"[]", br#"{"version":1,"make":"X","model":"Y","extra":true}"#, br#"{"version":99}"#, &[0xff, 0xfe, 0x00, 0x7b]];
    for (k, bytes) in inputs.iter().enumerate() {
        let file = repo.join(format!("in{k}.json"));
        std::fs::write(&file, bytes).unwrap();
        let o = Command::new(BIN).args(["calibrate-lightroom", "add", file.to_str().unwrap(), "--repo", repo.to_str().unwrap()]).output().unwrap();
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success(), "input {k} accepted");
        assert!(!err.contains("panicked"), "input {k} panicked: {err}");
    }
    assert!(!repo.join("crates/camdb/data/calibrations").exists(), "nothing written for refused files");
    let o = Command::new(BIN).args(["calibrate-lightroom", "fit", "--raws"]).output().unwrap();
    assert!(!o.status.success());
    let _ = std::fs::remove_dir_all(&repo);
}
