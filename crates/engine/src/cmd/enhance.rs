//! Photo ▸ Enhance: `enhance.rawDetails`, `enhance.superResolution` and the per-frame `enhance.pump`. The work is
//! `crate::enhance`.

use serde_json::Value;

use super::{CommandSpec, cmd};
use crate::enhance::EnhanceKind;
use crate::{Result, Session};

fn can_enhance(s: &Session) -> std::result::Result<(), String> {
    if cfg!(target_arch = "wasm32") && s.media.file_bytes.is_none() {
        return Err("Enhance needs access to the photo files".into());
    }
    // the targets are checked when run (an explicit `ids` parameter may name them)
    if s.catalog.photos().next().is_none() { Err("the library has no photos".into()) } else { Ok(()) }
}

fn pump(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(s.enhance_pump())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "enhance.rawDetails",
            "Raw Details",
            ["Photo", "Enhance"],
            None,
            "{ids?, stack?: true, wait?: false} → {queued} or with `wait` {made: [{id, source, path, size, bytes}], failed: [{id, error}]} — demosaic each raw again with the slower edge-aware method (finer detail, less false colour and zipper) into a new linear DNG `<name>-Enhanced-RD.dng` next to it, imported with the source's develop settings and stacked with it. Runs in the background with a row in the activity stack; `wait: true` returns when done (scripts)",
            can_enhance,
            |s, p| s.enhance_command(EnhanceKind::RawDetails, p)
        ),
        cmd!(
            "enhance.superResolution",
            "Super Resolution",
            ["Photo", "Enhance"],
            None,
            "{ids?, stack?: true, wait?: false} → as `enhance.rawDetails` — Raw Details, then twice the width and height (four times the pixels) by iterative back-projection, into `<name>-Enhanced-SR.dng`. Also takes JPEG, TIFF and other photos. At most 400 MP out",
            can_enhance,
            |s, p| s.enhance_command(EnhanceKind::SuperResolution, p)
        ),
        cmd!(query "enhance.pump", "Enhance Pump", [], None, "{} → {active, running, made, failed} — called every frame by the app: imports the photos Enhance has finished", super::always, pump),
    ]
}
