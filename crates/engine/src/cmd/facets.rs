//! `library.facets`: the distinct values of metadata fields among the photos in view, with how many photos have
//! each (issue #218: the first piece of Lightroom Classic's Metadata filter columns, and of a table view's column
//! filters). Each field is counted with its own constraint left out of the filter, so a field already narrowed to
//! one value still lists the others it could take; the other fields' constraints and the source apply.

use std::collections::HashMap;

use lcc_catalog::{ColorLabel, Filter, Flag, MediaKind, Photo, RatingOp, Sort};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd};
use crate::{Result, Session};

/// The fields `library.facets` counts.
pub const FIELDS: [&str; 11] = ["camera", "lens", "keyword", "person", "label", "flag", "rating", "kind", "year", "iso", "focal"];
/// Values one field returns without a `limit`.
pub const LIMIT_DEFAULT: usize = 200;
/// Most values one field returns.
pub const LIMIT_MAX: usize = 2000;

/// `f` without the constraint of `field` (the ones the filter bar and sidebar set for it).
fn without(field: &str, f: &Filter) -> Filter {
    let mut f = f.clone();
    match field {
        "camera" => {
            f.camera = None;
            f.cameras.clear();
        }
        "lens" => {
            f.lens = None;
            f.lenses.clear();
        }
        "keyword" => {
            f.keyword = None;
            f.keywords.clear();
        }
        "person" => {
            f.person = None;
            f.people.clear();
        }
        "label" => {
            f.label = None;
            f.labels.clear();
        }
        "flag" => f.flag = None,
        "rating" => f.rating = 0,
        "kind" => f.kind = None,
        "year" => {
            f.date = None;
            f.date_from = None;
            f.date_to = None;
        }
        // ISO and focal length are asked only in the search text (`iso:>800`), which stays
        _ => {}
    }
    f
}

/// The fields a Metadata column can narrow the view by (`library.facetPick`): ISO and focal length are counted
/// only, as the filter asks them in the search text alone.
pub const PICKABLE: [&str; 9] = ["camera", "lens", "keyword", "person", "label", "flag", "rating", "kind", "year"];

/// The values of `field` the filter has chosen, spelled as `library.facets` lists them.
pub fn chosen(s: &Session, field: &str) -> Vec<String> {
    let f = &s.filter;
    let list = |one: &Option<String>, many: &[String]| {
        let mut out: Vec<String> = Vec::new();
        for v in one.iter().chain(many.iter()) {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(v)) {
                out.push(v.clone());
            }
        }
        out
    };
    match field {
        "camera" => list(&f.camera, &f.cameras),
        "lens" => list(&f.lens, &f.lenses),
        "keyword" => list(&f.keyword, &f.keywords),
        "person" => list(&f.person, &f.people),
        "label" => {
            let mut v: Vec<ColorLabel> = f.labels.clone();
            v.extend(f.label.filter(|l| !f.labels.contains(l)));
            v.into_iter().map(|l| s.catalog.label_name(l)).collect()
        }
        "flag" => f.flag.map(flag_name).map(str::to_string).into_iter().collect(),
        "rating" if f.rating_op == RatingOp::Exactly && f.rating > 0 => vec![f.rating.to_string()],
        "kind" => f.kind.map(kind_name).map(str::to_string).into_iter().collect(),
        "year" => f.date.iter().filter(|d| d.len() == 4).cloned().collect(),
        _ => Vec::new(),
    }
}

fn flag_name(f: Flag) -> &'static str {
    match f {
        Flag::Pick => "pick",
        Flag::Reject => "reject",
        Flag::None => "none",
    }
}

fn kind_name(k: MediaKind) -> &'static str {
    match k {
        MediaKind::Image => "image",
        MediaKind::Raw => "raw",
        MediaKind::Video => "video",
    }
}

/// `library.facetPick`: a click on a Metadata column's value. Fields that take several values (camera, lens,
/// keyword, person, label) toggle it among the chosen ones, any of which matches; the others hold one value, which
/// a second click clears. No value clears the field.
fn pick(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.facetPick";
    let Some(field) = p.get("field").and_then(Value::as_str).filter(|f| PICKABLE.contains(f)) else {
        return Err(bad(C, format!("field is one of {}", PICKABLE.join(", "))));
    };
    let value = match p.get("value") {
        None | Some(Value::Null) => None,
        Some(Value::String(v)) if !v.trim().is_empty() => Some(v.trim().to_string()),
        Some(_) => return Err(bad(C, "value is a non-empty string (or null to clear the field)")),
    };
    let before = chosen(s, field);
    let on = value.as_ref().is_some_and(|v| before.iter().any(|b| b.eq_ignore_ascii_case(v)));
    // the chosen values once the click is done
    let next: Vec<String> = match &value {
        None => Vec::new(),
        Some(v) if on => before.iter().filter(|b| !b.eq_ignore_ascii_case(v)).cloned().collect(),
        Some(v) => before.iter().cloned().chain([v.clone()]).collect(),
    };
    // a single-valued field keeps only the last value clicked; clicking it again clears it
    let single = if on { None } else { value.clone() };
    match field {
        "camera" => (s.filter.camera, s.filter.cameras) = (None, next),
        "lens" => (s.filter.lens, s.filter.lenses) = (None, next),
        "keyword" => (s.filter.keyword, s.filter.keywords) = (None, next),
        "person" => (s.filter.person, s.filter.people) = (None, next),
        "label" => {
            let mut labels = Vec::new();
            for name in &next {
                let Some(l) = ColorLabel::ALL.into_iter().find(|l| s.catalog.label_name(*l).eq_ignore_ascii_case(name)) else {
                    return Err(bad(C, format!("no colour label is named `{name}`")));
                };
                labels.push(l);
            }
            labels.sort();
            labels.dedup();
            (s.filter.label, s.filter.labels) = (None, labels);
        }
        "flag" => {
            s.filter.flag = match single.as_deref() {
                None => None,
                Some("pick") => Some(Flag::Pick),
                Some("reject") => Some(Flag::Reject),
                Some("none") => Some(Flag::None),
                Some(other) => return Err(bad(C, format!("flag `{other}` is pick, reject or none"))),
            }
        }
        "rating" => match single {
            None => (s.filter.rating, s.filter.rating_op) = (0, RatingOp::AtLeast),
            Some(r) => {
                // (the filter has no "exactly unrated": a rating of 0 asks nothing)
                let r: u8 = r.parse().ok().filter(|r| (1..=5).contains(r)).ok_or_else(|| bad(C, format!("rating `{r}` is 1 to 5")))?;
                (s.filter.rating, s.filter.rating_op) = (r, RatingOp::Exactly);
            }
        },
        "kind" => {
            s.filter.kind = match single.as_deref() {
                None => None,
                Some("image") => Some(MediaKind::Image),
                Some("raw") => Some(MediaKind::Raw),
                Some("video") => Some(MediaKind::Video),
                Some(other) => return Err(bad(C, format!("kind `{other}` is image, raw or video"))),
            }
        }
        // "year"
        _ => {
            if single.as_ref().is_some_and(|y| y.len() != 4 || !y.chars().all(|c| c.is_ascii_digit())) {
                return Err(bad(C, "year is four digits"));
            }
            s.filter.date = single;
            (s.filter.date_from, s.filter.date_to) = (None, None);
        }
    }
    Ok(json!({"field": field, "chosen": chosen(s, field), "count": s.visible().len()}))
}

/// The values `field` takes for photo `p` (several for keywords and people; none when it has no value).
fn values(s: &Session, field: &str, p: &Photo) -> Vec<String> {
    let text = |t: &str| if t.trim().is_empty() { Vec::new() } else { vec![t.trim().to_string()] };
    match field {
        "camera" => text(&p.meta.camera),
        "lens" => text(&p.meta.lens),
        "keyword" => p.meta.keywords.iter().filter(|k| !k.trim().is_empty()).cloned().collect(),
        "person" => p.people().into_iter().map(str::to_string).collect(),
        "label" => p.label.map(|l| s.catalog.label_name(l)).into_iter().collect(),
        "flag" => vec![flag_name(p.flag).to_string()],
        "rating" => vec![p.rating.to_string()],
        "kind" => vec![kind_name(p.kind).to_string()],
        "year" => {
            p.captured.as_deref().and_then(|c| c.get(..4)).filter(|y| y.chars().all(|c| c.is_ascii_digit())).map(str::to_string).into_iter().collect()
        }
        "iso" => p.meta.iso.map(|i| i.to_string()).into_iter().collect(),
        "focal" => p.meta.focal_mm.filter(|f| f.is_finite() && *f > 0.0).map(|f| format!("{}", (f * 10.0).round() / 10.0)).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Numeric fields sort by value; the rest by count, most first, then by name.
fn numeric(field: &str) -> bool {
    matches!(field, "rating" | "year" | "iso" | "focal")
}

fn facets(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "library.facets";
    let fields: Vec<String> = match p.get("fields") {
        None | Some(Value::Null) => FIELDS.iter().map(|f| f.to_string()).collect(),
        Some(Value::Array(a)) => {
            let mut out = Vec::new();
            for v in a {
                let Some(name) = v.as_str().filter(|n| FIELDS.contains(n)) else {
                    return Err(bad(C, format!("fields are among {}", FIELDS.join(", "))));
                };
                if !out.iter().any(|o| o == name) {
                    out.push(name.to_string());
                }
            }
            out
        }
        Some(_) => return Err(bad(C, "fields must be a list of field names")),
    };
    let limit = match p.get("limit") {
        None | Some(Value::Null) => LIMIT_DEFAULT,
        Some(v) => {
            v.as_u64().map(|n| usize::try_from(n).unwrap_or(usize::MAX).min(LIMIT_MAX)).ok_or_else(|| bad(C, "limit must be a whole number"))?
        }
    };
    let mut out = serde_json::Map::new();
    for field in &fields {
        let f = s.view_filter(s.source, &without(field, &s.filter));
        let ids = s.catalog.query(&f, &Sort::default());
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut none = 0usize;
        for photo in ids.iter().filter_map(|id| s.catalog.photo(*id)) {
            let mut vals = values(s, field, photo);
            vals.sort();
            vals.dedup();
            if vals.is_empty() {
                none += 1;
            }
            for v in vals {
                *counts.entry(v).or_default() += 1;
            }
        }
        let distinct = counts.len();
        let mut rows: Vec<(String, usize)> = counts.into_iter().collect();
        if numeric(field) {
            rows.sort_by(|a, b| {
                a.0.parse::<f64>().unwrap_or(f64::MAX).total_cmp(&b.0.parse::<f64>().unwrap_or(f64::MAX)).then_with(|| a.0.cmp(&b.0))
            });
        } else {
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        }
        let shown: Vec<Value> = rows.into_iter().take(limit).map(|(value, count)| json!({"value": value, "count": count})).collect();
        out.insert(field.clone(), json!({"photos": ids.len(), "none": none, "distinct": distinct, "values": shown, "chosen": chosen(s, field)}));
    }
    Ok(Value::Object(out))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "library.facetPick",
            "Pick Metadata Value",
            [],
            None,
            "{field: camera | lens | keyword | person | label | flag | rating | kind | year, value?: a value as library.facets lists it (null: clear the field)} → {field, chosen, count} — a Metadata filter column's click: camera, lens, keyword, person and label toggle the value among the chosen ones (any matches); flag, rating (exactly), kind and year take the one value, and a second click clears it",
            always,
            pick
        ),
        cmd!(
        query "library.facets",
        "Metadata Facets",
        [],
        None,
        "{fields?: [camera | lens | keyword | person | label | flag | rating | kind | year | iso | focal] (default: all), limit? (values per field, default 200, at most 2000)} → {<field>: {photos, none, distinct, values: [{value, count}], chosen: [the values the filter picked]}} — the values each field takes among the photos in view (the source, and the filter without that field's own constraint), with how many photos have each; `none`: photos without one. Counts by most photos first; rating, year, ISO and focal length by value",
            always,
            facets
        ),
    ]
}
