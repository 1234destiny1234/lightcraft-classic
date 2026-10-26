//! Example plug-in. Build: `cargo build --release --target wasm32-unknown-unknown`, then install
//! `target/wasm32-unknown-unknown/release/program_tools.wasm` from the Plug-in Manager and tick
//! "Start the program Example Converter".
//!
//! - **Send to Converter** (menu command): writes the selected photos' ids into the run's folder,
//!   one per line, and starts the converter with that list's path. The converter (a stand-in in
//!   the host's tests) writes `result.txt` next to it: `<photo id>\t<path of what it made>` lines.
//! - **`program.end` hook**: reads `result.txt` and records each photo's result in the plug-in's
//!   own metadata namespace (`converted`).

use lcc_plugin_sdk::serde_json::{Value, json};
use lcc_plugin_sdk::{Request, export_plugin, host};

const MANIFEST: &str = r#"{
  "id": "org.example.program-tools",
  "name": "Program Tools (example)",
  "version": "0.1.0",
  "description": "Example plug-in for the SDK: starts a program",
  "author": "the app's authors",
  "requires": ["programs"],
  "permissions": {
    "programs": [
      {"id": "converter", "name": "Example Converter",
       "macBundleIds": ["org.example.converter"], "windowsExe": "converter.exe", "linuxExe": "converter"}
    ]
  },
  "commands": [
    {"id": "send", "label": "Send to Converter", "menu": "Examples"}
  ]
}"#;

const PROGRAM: &str = "converter";

fn handle(req: &Request) -> Result<Value, String> {
    match req.hook() {
        "command" => match req.command() {
            "send" => send(&req.selection()),
            other => Err(format!("unknown command {other}")),
        },
        "program.end" => end(req),
        other => Err(format!("unsupported hook {other}")),
    }
}

/// Starts the converter on `photos`, or says why it can't.
fn send(photos: &[String]) -> Result<Value, String> {
    if photos.is_empty() {
        return Ok(json!({"message": "Select the photos to convert first."}));
    }
    let list = host::program_list()?;
    let entry = list.as_array().and_then(|l| l.iter().find(|p| p.get("program").and_then(Value::as_str) == Some(PROGRAM)));
    let name = entry.and_then(|p| p.get("name")).and_then(Value::as_str).unwrap_or(PROGRAM);
    if entry.and_then(|p| p.get("granted")).and_then(Value::as_bool) != Some(true) {
        return Ok(json!({"message": format!("Allow {name} in the Plug-in Manager first.")}));
    }
    if entry.and_then(|p| p.get("installed")).and_then(Value::as_array).is_none_or(|i| i.is_empty()) {
        return Ok(json!({"message": format!("{name} is not installed.")}));
    }
    let mut job = photos.join("\n");
    job.push('\n');
    let started = host::program_start(PROGRAM, &["{tmpDir}/job.txt"], json!({"job.txt": job}), None)?;
    let run = started.get("run").and_then(Value::as_str).unwrap_or_default();
    host::log(&format!("{run}: sent {} photo(s) to {name}", photos.len()));
    Ok(json!({"message": format!("Sent {} photo(s) to {name}.", photos.len()), "run": run}))
}

/// Records what the converter made, from `result.txt` in the run's folder.
fn end(req: &Request) -> Result<Value, String> {
    if req.status() != "exited" {
        host::log(&format!("{}: {}", req.run(), req.status()));
        return Ok(json!({"converted": []}));
    }
    if let Some(code) = req.code().filter(|c| *c != 0) {
        host::log(&format!("{}: the converter failed (exit code {code})", req.run()));
        return Ok(json!({"converted": []}));
    }
    let result = host::read_text(&format!("{}/result.txt", req.tmp_dir()))?;
    let mut converted = Vec::new();
    for line in result.lines() {
        let Some((photo, path)) = line.split_once('\t') else { continue };
        host::ns_set(photo, "converted", json!(path))?;
        converted.push(photo.to_string());
    }
    host::log(&format!("{}: {} photo(s) converted", req.run(), converted.len()));
    Ok(json!({"converted": converted}))
}

export_plugin!(MANIFEST, handle);
