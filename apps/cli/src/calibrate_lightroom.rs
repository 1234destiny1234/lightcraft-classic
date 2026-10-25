//! `<cli> calibrate-lightroom`: fit a camera's colour model and named looks to Lightroom's exports of the
//! contributor's own raws (`lcc_colourfit`), and the developer step that adds a contributed file to the repository.
//!
//! - `preset -o FILE`: the baseline Lightroom preset contributors apply before exporting.
//! - `fit --raws DIR… --exports DIR… [-o OUTDIR] [--contributor NAME] [--dry-run]`: pair, sample, fit; writes the
//!   calibration JSON (the file to send) and one `.dcp` per look for the contributor's own local testing.
//! - `add FILE [--repo DIR] [--contributor NAME]`: validate a contribution and commit-ready write it into
//!   `crates/camdb/data/calibrations/` with its `assets/ATTRIBUTION.md` row.
//! - `list`: the calibrations built into this binary.
//! - `verify --raws DIR… --exports DIR… [--report DIR] [--json] [--size N] [--max N]`: the app's render of edited
//!   photos against their edited exports, ΔE00 per photo and per group of controls used (`lcc_colourfit::verify`).
//!
//! Sources: own design; DNG specification 1.7 chapter 6 (camera profile tags) for the `.dcp` writer.

use std::path::{Path, PathBuf};

use lcc_camdb::calibration::{self, Calibration};
use lcc_colourfit::dataset::{self, CollectOptions, ExportSet};
use lcc_colourfit::fit::{self, FitOptions};
use lcc_raw::ColorData;
use lcc_raw::profile::HsvTable;

/// Where `add` writes calibrations, relative to the repository root.
const DATA_DIR: &str = "crates/camdb/data/calibrations";

pub fn run(args: &[String]) -> Result<(), String> {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("preset") => preset(rest),
        Some("fit") => fit_cmd(rest),
        Some("add") => add(rest),
        Some("list") => list(),
        Some("verify") => verify_cmd(rest),
        _ => Err("calibrate-lightroom needs `preset`, `fit`, `add`, `list` or `verify` (see --help)".into()),
    }
}

fn take<'a>(args: &'a [String], i: &mut usize, flag: &str) -> Result<&'a str, String> {
    *i += 1;
    args.get(*i).map(String::as_str).ok_or_else(|| format!("{flag} needs a value"))
}

fn verify_cmd(args: &[String]) -> Result<(), String> {
    use lcc_colourfit::verify::{self, VerifyOptions};
    let (mut raw_dirs, mut export_dirs) = (Vec::new(), Vec::new());
    let mut report_dir: Option<PathBuf> = None;
    let mut json = false;
    let mut opts = VerifyOptions::default();
    let mut list: Option<bool> = None; // Some(true) = raws, Some(false) = exports
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--raws" => list = Some(true),
            "--exports" => list = Some(false),
            "--report" => report_dir = Some(take(args, &mut i, "--report")?.into()),
            "--json" => json = true,
            "--size" => {
                let v = take(args, &mut i, "--size")?;
                opts.compare.long_edge = v.parse().map_err(|_| format!("--size: `{v}` is not a number of pixels"))?;
            }
            "--max" => {
                let v = take(args, &mut i, "--max")?;
                opts.max_photos = v.parse().map_err(|_| format!("--max: `{v}` is not a number of photos"))?;
            }
            a if a.starts_with('-') => return Err(format!("unknown option `{a}`")),
            a => match list {
                Some(true) => raw_dirs.push(PathBuf::from(a)),
                Some(false) => export_dirs.push(PathBuf::from(a)),
                None => return Err(format!("`{a}`: put folders after --raws or --exports")),
            },
        }
        i += 1;
    }
    if raw_dirs.is_empty() || export_dirs.is_empty() {
        return Err("verify needs --raws DIR… and --exports DIR… (the edited exports, searched with their subfolders)".into());
    }
    let mut raws = Vec::new();
    for d in &raw_dirs {
        raws_in(d, &mut raws, 0);
    }
    raws.sort();
    raws.dedup();
    if raws.is_empty() {
        return Err("no raw files found in the --raws folders".into());
    }
    let exports: Vec<ExportSet> = export_dirs.iter().map(|d| ExportSet { dir: d.clone() }).collect();
    let mut progress = |done: usize, total: usize, name: &str| {
        if done < total {
            eprintln!("calibrate-lightroom verify: {}/{total} {name}", done + 1);
        }
    };
    let report = verify::verify(&raws, &exports, &opts, &mut progress).map_err(|e| e.to_string())?;
    if let Some(dir) = &report_dir {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for p in &report.photos {
            let Some(rep) = &p.report else { continue };
            let rgb = lcc_colourfit::compare::map_rgb8(rep);
            let (Ok(w), Ok(h)) = (u32::try_from(rep.width), u32::try_from(rep.height)) else { continue };
            let img = lcc_codecs::EncodeImage::new(w, h, 3, lcc_codecs::Samples::U8(&rgb));
            let png = lcc_codecs::encode_png(&img, &lcc_codecs::EncodeMeta::default()).map_err(|e| e.to_string())?;
            let stem = Path::new(&p.raw).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.raw.clone());
            let out = dir.join(format!("{stem}.de00.png"));
            std::fs::write(&out, png).map_err(|e| format!("{}: {e}", out.display()))?;
        }
        let out = dir.join("verify.json");
        let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
        eprintln!("calibrate-lightroom verify: wrote {} (and a ΔE00 map per photo)", out.display());
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
    } else {
        print!("{}", verify::text(&report));
    }
    Ok(())
}

fn preset(args: &[String]) -> Result<(), String> {
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => out = Some(take(args, &mut i, "-o")?.to_string()),
            a => return Err(format!("unknown argument `{a}`")),
        }
        i += 1;
    }
    let out = out.ok_or("preset needs -o FILE.xmp")?;
    let xmp = lcc_colourfit::preset::baseline_xmp();
    if xmp.is_empty() {
        return Err("the baseline preset is not available in this build yet".into());
    }
    std::fs::write(&out, xmp).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("calibrate-lightroom: wrote {out}; import it in Lightroom's Develop module (Presets ▸ Import Presets)");
    Ok(())
}

/// Files below `dir` that look like raws (by content, else by extension), skipping hidden folders.
fn raws_in(path: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    const EXT: &[&str] =
        &["arw", "cr2", "cr3", "crw", "dng", "nef", "nrw", "raf", "orf", "rw2", "rwl", "pef", "srw", "x3f", "3fr", "iiq", "erf", "mrw", "kdc", "dcr"];
    if path.is_file() {
        let by_ext = path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXT.iter().any(|x| e.eq_ignore_ascii_case(x)));
        let by_content = || {
            use std::io::Read;
            let mut head = vec![0u8; 64 << 10];
            let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
            head.get(..n).and_then(lcc_codecs::sniff).is_some_and(|f| f.is_raw())
        };
        if by_ext || by_content() {
            out.push(path.to_path_buf());
        }
        return;
    }
    if depth > 16 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(['.', '@']) {
            continue;
        }
        raws_in(&entry.path(), out, depth + 1);
    }
}

fn fit_cmd(args: &[String]) -> Result<(), String> {
    let (mut raw_dirs, mut export_dirs) = (Vec::new(), Vec::new());
    let mut out = PathBuf::from(".");
    let mut contributor = None;
    let mut dry_run = false;
    // `--raws A B --exports C D`: plain arguments go to the list the last flag opened
    let mut list: Option<bool> = None; // Some(true) = raws, Some(false) = exports
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--raws" => list = Some(true),
            "--exports" => list = Some(false),
            "-o" | "--out" => out = take(args, &mut i, "-o")?.into(),
            "--contributor" => contributor = Some(take(args, &mut i, "--contributor")?.to_string()),
            "--dry-run" => dry_run = true,
            a if a.starts_with('-') => return Err(format!("unknown option `{a}`")),
            a => match list {
                Some(true) => raw_dirs.push(PathBuf::from(a)),
                Some(false) => export_dirs.push(PathBuf::from(a)),
                None => return Err(format!("`{a}`: put folders after --raws or --exports")),
            },
        }
        i += 1;
    }
    if raw_dirs.is_empty() || export_dirs.is_empty() {
        return Err("fit needs --raws DIR… and --exports DIR… (folders of Lightroom exports, searched with their subfolders)".into());
    }
    let mut raws = Vec::new();
    for d in &raw_dirs {
        raws_in(d, &mut raws, 0);
    }
    raws.sort();
    raws.dedup();
    if raws.is_empty() {
        return Err("no raw files found in the --raws folders".into());
    }
    let exports: Vec<ExportSet> = export_dirs.iter().map(|d| ExportSet { dir: d.clone() }).collect();
    eprintln!("calibrate-lightroom: {} raw file(s), {} export folder(s); pairing and sampling…", raws.len(), exports.len());
    let (photos, report) = dataset::collect(&raws, &exports, &CollectOptions::default()).map_err(|e| e.to_string())?;
    eprintln!("calibrate-lightroom: {} pair(s) usable", report.paired);
    for (file, why) in &report.skipped {
        eprintln!("  skipped {file}: {why}");
    }
    let ccts: Vec<f64> = photos.iter().filter_map(|p| p.white.lightroom_temp_tint.map(|(t, _)| t)).filter(|t| t.is_finite()).collect();
    if let (Some(lo), Some(hi)) = (ccts.iter().copied().reduce(f64::min), ccts.iter().copied().reduce(f64::max)) {
        eprintln!("calibrate-lightroom: white balance {lo:.0} K – {hi:.0} K over {} export(s)", ccts.len());
        if hi - lo < 1500.0 {
            eprintln!("  narrow white-balance range: add tungsten-lit (indoor) and daylight photos for a better fit");
        }
    }
    let mut looks: Vec<&str> = photos.iter().map(|p| p.look.as_str()).collect();
    looks.sort_unstable();
    looks.dedup();
    eprintln!("calibrate-lightroom: looks: {}", looks.join(", "));
    if dry_run {
        eprintln!("calibrate-lightroom: dry run, nothing fitted or written");
        return Ok(());
    }
    eprintln!("calibrate-lightroom: fitting (this can take a few minutes)…");
    let mut cal = fit::fit(&photos, &FitOptions { contributor, rounds: 0 }).map_err(|e| e.to_string())?;
    cal.tool_version = env!("CARGO_PKG_VERSION").to_string();
    cal.validate().map_err(|e| format!("the fit produced an invalid calibration: {e}"))?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let json_path = out.join(cal.file_name());
    std::fs::write(&json_path, cal.to_json().map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", json_path.display()))?;
    let mut dcps = Vec::new();
    for look in &cal.looks {
        let dcp = cal.look_dcp(look);
        let bytes = write_dcp(&dcp.model, &dcp.name, &dcp.copyright, &dcp.color)?;
        let stem = json_path.file_stem().and_then(|s| s.to_str()).unwrap_or("calibration");
        // the id without its `cal:` prefix: `:` is not allowed in Windows file names
        let id = calibration::look_id(&look.name);
        let slug = id.strip_prefix(calibration::LOOK_PREFIX).unwrap_or(&id);
        let path = out.join(format!("{stem}.{slug}.dcp"));
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        dcps.push(path);
    }
    print_contents(&cal);
    println!();
    println!("Send this file: {}", json_path.display());
    println!("  It holds only the numbers listed above: no images, file names, dates, serial numbers, locations or lenses.");
    if lcc_brand::CALIBRATION_DISCORD.is_empty() {
        println!("  Open an issue on {} and attach it (licence {}).", lcc_brand::REPOSITORY, calibration::LICENCE);
    } else {
        let thread =
            if lcc_brand::CALIBRATION_THREAD.is_empty() { String::new() } else { format!(" in the {} thread", lcc_brand::CALIBRATION_THREAD) };
        println!("  Send it on Discord{thread}: {} (licence {}).", lcc_brand::CALIBRATION_DISCORD, calibration::LICENCE);
    }
    if !dcps.is_empty() {
        println!("For your own testing only (do not send): Camera ▸ Import DCP… in the app, or `camera.importDcp`:");
        for p in &dcps {
            println!("  {}", p.display());
        }
    }
    Ok(())
}

/// Everything a calibration file holds, in words.
fn print_contents(c: &Calibration) {
    println!("{} {} — calibration matched to Lightroom", c.make, c.model);
    if !c.aliases.is_empty() {
        println!("  also sold as: {}", c.aliases.join(", "));
    }
    println!("  contributor: {}", c.contributor.as_deref().unwrap_or("(none; pass --contributor NAME for credit)"));
    println!("  tool version {}, licence {}", c.tool_version, c.licence);
    println!(
        "  camera colour model: illuminants {:?}, two colour and two forward matrices (3×3){}",
        c.camera.illuminants,
        if c.camera.hue_sat_map.is_some() { ", two hue/saturation maps" } else { "" }
    );
    for l in &c.looks {
        println!(
            "  look \"{}\": exposure offset {:+.2} EV, {}-point tone curve{}",
            l.name,
            l.baseline_exposure_offset,
            l.tone_curve.len(),
            if l.look_table.is_some() { ", look table" } else { "" }
        );
    }
    let e = &c.evidence;
    println!("  evidence: {} photos, {} colour samples, hue coverage {:.0} %", e.photos, e.samples, e.hue_coverage * 100.0);
    if !e.cct.is_empty() {
        let bins: Vec<String> = e.cct.iter().map(|b| format!("≥{} K: {}", b.from_k, b.photos)).collect();
        println!("  white-balance ranges (photos): {}", bins.join(", "));
    }
    for l in &e.looks {
        println!(
            "  held-out ΔE00 \"{}\" ({} photos, {} held out): median {:.2} → {:.2}, p95 {:.2} → {:.2}",
            l.name, l.photos, l.held_out, l.before.median, l.after.median, l.before.p95, l.after.p95
        );
    }
}

fn list() -> Result<(), String> {
    let all = calibration::embedded();
    if all.is_empty() {
        println!("no calibrations are built in yet");
        return Ok(());
    }
    for c in all {
        let looks: Vec<String> =
            c.looks.iter().map(|l| format!("{} ({}{})", l.name, calibration::LOOK_PREFIX, calibration::look_id(&l.name))).collect();
        println!("{} {}: {} photos; looks: {}", c.make, c.model, c.evidence.photos, looks.join(", "));
        for l in &c.evidence.looks {
            println!("  ΔE00 {}: median {:.2} → {:.2}, p95 {:.2} → {:.2}", l.name, l.before.median, l.after.median, l.before.p95, l.after.p95);
        }
    }
    Ok(())
}

fn add(args: &[String]) -> Result<(), String> {
    let mut file = None;
    let mut repo = PathBuf::from(".");
    let mut contributor = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo" => repo = take(args, &mut i, "--repo")?.into(),
            "--contributor" => contributor = Some(take(args, &mut i, "--contributor")?.to_string()),
            a if a.starts_with('-') => return Err(format!("unknown option `{a}`")),
            a if file.is_none() => file = Some(a.to_string()),
            a => return Err(format!("unexpected argument `{a}`")),
        }
        i += 1;
    }
    let file = file.ok_or("add needs the contributed calibration FILE")?;
    let meta = std::fs::metadata(&file).map_err(|e| format!("{file}: {e}"))?;
    if meta.len() > calibration::MAX_FILE as u64 {
        return Err(format!("{file}: {} bytes is too large for a calibration", meta.len()));
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?;
    let mut cal = Calibration::parse(&text).map_err(|e| format!("{file}: {e}"))?;
    if let Some(c) = contributor {
        cal.contributor = Some(c);
        cal.validate().map_err(|e| format!("{file}: {e}"))?;
    }
    let attribution = repo.join("assets/ATTRIBUTION.md");
    let table =
        std::fs::read_to_string(&attribution).map_err(|e| format!("{}: {e} (pass --repo DIR, the repository root)", attribution.display()))?;
    let dir = repo.join(DATA_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = cal.file_name();
    let dest = dir.join(&name);
    std::fs::write(&dest, cal.to_json().map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", dest.display()))?;
    let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).date_naive().format("%Y-%m-%d").to_string();
    let updated = with_attribution_row(&table, &format!("{DATA_DIR}/{name}"), &cal, &today);
    std::fs::write(&attribution, updated).map_err(|e| format!("{}: {e}", attribution.display()))?;
    println!("added {}", dest.display());
    println!("updated {}", attribution.display());
    print_contents(&cal);
    Ok(())
}

/// The ATTRIBUTION table with `path`'s row added after the last calibration or camera-profile row, or replacing
/// the row it already has.
fn with_attribution_row(table: &str, path: &str, c: &Calibration, today: &str) -> String {
    let looks: Vec<&str> = c.looks.iter().map(|l| l.name.as_str()).collect();
    let by = c.contributor.as_deref().map(|n| format!(" by {}", n.replace('|', "/"))).unwrap_or_default();
    let row = format!(
        "| `{path}` | {make} {model} camera calibration matched to Lightroom (colour model + looks: {looks}), built into the app | {who} | original work: black-box fit by `<cli> calibrate-lightroom` to {n} photos and their Lightroom exports{by}; aggregate statistics only, no image content; no Adobe profile data | {lic} | {today} | none |",
        make = c.make.replace('|', "/"),
        model = c.model.replace('|', "/"),
        looks = looks.join(", ").replace('|', "/"),
        who = c.contributor.as_deref().map(|n| n.replace('|', "/")).unwrap_or_else(|| "contributors".into()),
        n = c.evidence.photos,
        lic = c.licence,
    );
    let key = format!("| `{path}` |");
    let mut lines: Vec<String> = table.lines().map(str::to_string).collect();
    if let Some(l) = lines.iter_mut().find(|l| l.starts_with(&key)) {
        *l = row;
    } else {
        let anchor = lines
            .iter()
            .rposition(|l| l.starts_with(&format!("| `{DATA_DIR}/")))
            .or_else(|| lines.iter().rposition(|l| l.starts_with("| `assets/camera-profiles/")))
            .or_else(|| lines.iter().rposition(|l| l.starts_with("|---")));
        match anchor {
            Some(at) => lines.insert(at + 1, row),
            None => lines.push(row),
        }
    }
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

/// A little-endian DNG camera profile (`.dcp`) of `color`'s profile tags, readable by `lcc_raw::dcp::parse`.
pub fn write_dcp(model: &str, name: &str, copyright: &str, color: &ColorData) -> Result<Vec<u8>, String> {
    enum V {
        Ascii(String),
        Short(u16),
        Long(Vec<u32>),
        SRational(Vec<f64>),
        Float(Vec<f32>),
    }
    let mut tags: Vec<(u16, V)> = vec![(50708, V::Ascii(model.into()))];
    let mat = |m: &lcc_raw::Mat3| V::SRational(m.0.iter().flatten().copied().collect());
    for (k, tag) in [(0usize, 50721u16), (1, 50722)] {
        if let Some(Some(m)) = color.color_matrix.get(k) {
            tags.push((tag, mat(m)));
        }
    }
    for (k, tag) in [(0usize, 50778u16), (1, 50779)] {
        if let Some(&code) = color.illuminant.get(k)
            && code != 0
        {
            tags.push((tag, V::Short(code)));
        }
    }
    if !name.is_empty() {
        tags.push((50936, V::Ascii(name.into())));
    }
    let table_data = |t: &HsvTable| V::Float(t.data.iter().flatten().copied().collect());
    let dims = |t: &HsvTable| -> Result<V, String> {
        let d = [t.hue_divisions, t.sat_divisions, t.val_divisions].map(u32::try_from);
        match d {
            [Ok(h), Ok(s), Ok(v)] => Ok(V::Long(vec![h, s, v])),
            _ => Err("table too large for a camera profile".into()),
        }
    };
    let p = &color.profile;
    if let [Some(a), Some(b)] = &p.hue_sat_map {
        if (a.hue_divisions, a.sat_divisions, a.val_divisions) == (b.hue_divisions, b.sat_divisions, b.val_divisions) {
            tags.push((50937, dims(a)?));
            tags.push((50938, table_data(a)));
            tags.push((50939, table_data(b)));
            tags.push((51107, V::Long(vec![u32::from(a.srgb_value)])));
        }
    } else if let Some(a) = p.hue_sat_map.iter().flatten().next() {
        tags.push((50937, dims(a)?));
        tags.push((50938, table_data(a)));
        tags.push((51107, V::Long(vec![u32::from(a.srgb_value)])));
    }
    if let Some(curve) = &p.tone_curve {
        tags.push((50940, V::Float(curve.points.iter().flatten().copied().collect())));
    }
    tags.push((50941, V::Long(vec![3]))); // ProfileEmbedPolicy: no restrictions
    if !copyright.is_empty() {
        tags.push((50942, V::Ascii(copyright.into())));
    }
    for (k, tag) in [(0usize, 50964u16), (1, 50965)] {
        if let Some(Some(m)) = color.forward_matrix.get(k) {
            tags.push((tag, mat(m)));
        }
    }
    if let Some(t) = &p.look_table {
        tags.push((50981, dims(t)?));
        tags.push((50982, table_data(t)));
        tags.push((51108, V::Long(vec![u32::from(t.srgb_value)])));
    }
    if color.baseline_exposure != 0.0 && color.baseline_exposure.is_finite() {
        tags.push((51109, V::SRational(vec![color.baseline_exposure])));
    }
    tags.sort_by_key(|(t, _)| *t);

    let to_u32 = |n: usize| u32::try_from(n).map_err(|_| "camera profile too large".to_string());
    let encode = |v: &V| -> (u16, usize, Vec<u8>) {
        match v {
            V::Ascii(s) => {
                let mut b: Vec<u8> = s.bytes().filter(|&c| c != 0).collect();
                b.push(0);
                (2, b.len(), b)
            }
            V::Short(x) => (3, 1, x.to_le_bytes().to_vec()),
            V::Long(xs) => (4, xs.len(), xs.iter().flat_map(|x| x.to_le_bytes()).collect()),
            V::SRational(xs) => (
                10,
                xs.len(),
                xs.iter()
                    .flat_map(|x| {
                        let n = (x * 10000.0).round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
                        n.to_le_bytes().into_iter().chain(10000i32.to_le_bytes())
                    })
                    .collect(),
            ),
            V::Float(xs) => (11, xs.len(), xs.iter().flat_map(|x| x.to_le_bytes()).collect()),
        }
    };
    let n = tags.len();
    let ifd_len = 2 + 12 * n + 4;
    let mut head = b"IIRC".to_vec();
    head.extend_from_slice(&8u32.to_le_bytes());
    head.extend_from_slice(&u16::try_from(n).map_err(|_| "too many tags".to_string())?.to_le_bytes());
    let mut data = Vec::new();
    let data_at = 8 + ifd_len;
    for (tag, v) in &tags {
        let (ty, count, bytes) = encode(v);
        head.extend_from_slice(&tag.to_le_bytes());
        head.extend_from_slice(&ty.to_le_bytes());
        head.extend_from_slice(&to_u32(count)?.to_le_bytes());
        if bytes.len() <= 4 {
            let mut inline = [0u8; 4];
            for (d, s) in inline.iter_mut().zip(&bytes) {
                *d = *s;
            }
            head.extend_from_slice(&inline);
        } else {
            if data.len() % 2 == 1 {
                data.push(0); // TIFF values start on a word boundary
            }
            head.extend_from_slice(&to_u32(data_at + data.len())?.to_le_bytes());
            data.extend_from_slice(&bytes);
        }
    }
    head.extend_from_slice(&0u32.to_le_bytes());
    head.extend_from_slice(&data);
    Ok(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lcc_raw::Mat3;
    use lcc_raw::profile::ToneCurve;

    #[test]
    fn written_profiles_parse_back() {
        let mut color = ColorData { illuminant: [17, 21], ..ColorData::default() };
        let fm = Mat3([[0.70, 0.18, 0.08], [0.28, 0.80, -0.08], [0.02, -0.15, 0.95]]);
        let cm = Mat3([[0.9, -0.3, -0.05], [-0.5, 1.4, 0.1], [-0.1, 0.3, 0.6]]);
        color.color_matrix = [Some(cm), Some(cm)];
        color.forward_matrix = [Some(fm), Some(fm)];
        color.baseline_exposure = 0.35;
        let table = HsvTable { hue_divisions: 6, sat_divisions: 2, val_divisions: 1, data: vec![[2.0, 1.1, 0.9]; 12], srgb_value: false };
        color.profile.hue_sat_map = [Some(table.clone()), Some(table.clone())];
        color.profile.look_table = Some(table);
        color.profile.tone_curve = Some(ToneCurve { points: vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]] });
        let bytes = write_dcp("SONY ILCE-7M4", "Standard (matched)", "A Contributor", &color).unwrap();
        let d = lcc_raw::dcp::parse(&bytes).unwrap();
        assert_eq!(d.model, "SONY ILCE-7M4");
        assert_eq!(d.name, "Standard (matched)");
        assert_eq!(d.copyright, "A Contributor");
        assert_eq!(d.color.illuminant, [17, 21]);
        let got = d.color.forward_matrix[1].unwrap();
        assert!((got.0[1][1] - 0.80).abs() < 1e-3);
        assert!((d.color.baseline_exposure - 0.35).abs() < 1e-3);
        assert!(d.color.profile.hue_sat_map[1].is_some());
        assert_eq!(d.color.profile.look_table.as_ref().map(|t| t.data.len()), Some(12));
        assert_eq!(d.color.profile.tone_curve.as_ref().map(|c| c.points.len()), Some(3));
    }

    #[test]
    fn attribution_row_is_added_once_and_replaced() {
        let table = "| Path | x |\n|---|---|\n| `assets/camera-profiles/A.json` | a |\n| `other` | b |\n";
        let cal = sample();
        let once = with_attribution_row(table, "crates/camdb/data/calibrations/SONY_ILCE-7M4.json", &cal, "2026-10-25");
        let twice = with_attribution_row(&once, "crates/camdb/data/calibrations/SONY_ILCE-7M4.json", &cal, "2026-10-26");
        assert_eq!(twice.matches("SONY_ILCE-7M4.json").count(), 1);
        assert!(twice.contains("2026-10-26"));
        let lines: Vec<&str> = twice.lines().collect();
        assert!(lines[3].contains("SONY_ILCE-7M4.json"), "row goes after the camera-profile rows");
    }

    fn sample() -> Calibration {
        let fm = Mat3([[0.70, 0.18, 0.08], [0.28, 0.80, -0.08], [0.02, -0.15, 0.95]]);
        let cm = Mat3([[0.9, -0.3, -0.05], [-0.5, 1.4, 0.1], [-0.1, 0.3, 0.6]]);
        Calibration {
            version: calibration::VERSION,
            make: "SONY".into(),
            model: "ILCE-7M4".into(),
            aliases: vec![],
            method: calibration::METHOD.into(),
            tool_version: "0".into(),
            contributor: Some("A Contributor".into()),
            licence: calibration::LICENCE.into(),
            camera: calibration::CameraModel { illuminants: [17, 21], color_matrix: [cm, cm], forward_matrix: [fm, fm], hue_sat_map: None },
            looks: vec![],
            evidence: calibration::Evidence { photos: 24, ..Default::default() },
        }
    }
}
