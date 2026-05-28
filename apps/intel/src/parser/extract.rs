//! Stage 2: classify the body of a chatlog line into structured sightings.
//!
//! Real intel reports don't follow the MOTD grammar; we observe shapes
//! like `R-YWID  DrogeOne`, `V7-MID  Ed Stonler nv`, `s1000gt  4DTQ-K*`,
//! `Cheetah  AshiPze  J52-BH`, `4DTQ-K* -`, `S-KSWL 6+`,
//! `Bodyan Pacyan  Dominator1990  Genzel2  LancknehT  Michael Jakson`.
//!
//! The convention that holds (empirically) is **2+ spaces separate fields**
//! while single spaces stay inside multi-word values. Within each field we
//! try to match against known ships (greedy multi-word) and known systems,
//! then sentinels (`nv`, `clr`, `-`, `сдк`, `+N`/`N+`); anything left over
//! is treated as a pilot name.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct Lookups {
    pub systems: HashMap<String, i64>,   // lowercase name → system_id
    pub ships: HashMap<String, i64>,     // lowercase name → type_id
    pub max_ship_words: usize,           // longest multi-word ship name in `ships`
}

impl Lookups {
    pub fn new(systems: HashMap<String, i64>, ships: HashMap<String, i64>) -> Self {
        let max_ship_words = ships
            .keys()
            .map(|s| s.split_whitespace().count())
            .max()
            .unwrap_or(1);
        Self { systems, ships, max_ship_words }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sighting {
    pub ts: DateTime<Utc>,
    pub channel: String,
    pub reporter: String,
    pub system_id: Option<i64>,
    pub pilots: Vec<String>,
    pub ship_type_id: Option<i64>,
    pub fleet_count: Option<u16>,
    pub no_visual: bool,
    pub is_clear: bool,
    pub raw_body: String,
    pub parse_confidence: f32,
}

#[derive(Debug, Clone)]
enum Tok {
    System(i64),
    Ship(i64),
    Clear,
    NoVisual,
    Count(u16),
    Word(String),
}

/// Classify a chatlog body and produce zero or more sightings.
///
/// - If the body contains N>0 system tokens, emit N sightings (one per
///   system) and attach pilots/ship/flags to each.
/// - If the body contains no system token, emit a single low-confidence
///   sighting with `system_id = None`. These are useful for the pilot
///   rap sheet (follow-up "name (Cyclone)" lines) but don't contribute
///   to the safety metric.
pub fn extract(
    ts: DateTime<Utc>,
    channel: &str,
    reporter: &str,
    body: &str,
    lookups: &Lookups,
) -> Vec<Sighting> {
    let fields = split_fields(body);
    let mut tokens: Vec<Tok> = Vec::new();
    for f in &fields {
        tokens.extend(classify_field(f, lookups));
    }

    let systems: Vec<i64> = tokens
        .iter()
        .filter_map(|t| if let Tok::System(s) = t { Some(*s) } else { None })
        .collect();
    let ship = tokens
        .iter()
        .find_map(|t| if let Tok::Ship(s) = t { Some(*s) } else { None });
    let count = tokens
        .iter()
        .find_map(|t| if let Tok::Count(n) = t { Some(*n) } else { None });
    let no_visual = tokens.iter().any(|t| matches!(t, Tok::NoVisual));
    let is_clear = tokens.iter().any(|t| matches!(t, Tok::Clear));
    let pilots: Vec<String> = tokens
        .iter()
        .filter_map(|t| if let Tok::Word(w) = t { Some(w.clone()) } else { None })
        .collect();

    let base_confidence = match (systems.is_empty(), ship.is_some(), pilots.is_empty()) {
        (false, true, false) => 1.0,
        (false, _, _) => 0.8,
        (true, _, _) => 0.3,
    };

    if systems.is_empty() {
        return vec![Sighting {
            ts,
            channel: channel.to_string(),
            reporter: reporter.to_string(),
            system_id: None,
            pilots,
            ship_type_id: ship,
            fleet_count: count,
            no_visual,
            is_clear,
            raw_body: body.to_string(),
            parse_confidence: base_confidence,
        }];
    }

    systems
        .into_iter()
        .map(|sys| Sighting {
            ts,
            channel: channel.to_string(),
            reporter: reporter.to_string(),
            system_id: Some(sys),
            pilots: pilots.clone(),
            ship_type_id: ship,
            fleet_count: count,
            no_visual,
            is_clear,
            raw_body: body.to_string(),
            parse_confidence: base_confidence,
        })
        .collect()
}

/// Split a body on runs of 2+ whitespace characters. Single spaces stay
/// inside the resulting fields.
fn split_fields(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut space_run = 0usize;
    for ch in body.chars() {
        if ch.is_whitespace() {
            space_run += 1;
            continue;
        }
        if space_run >= 2 && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        } else if space_run >= 1 && !current.is_empty() {
            current.push(' ');
        }
        space_run = 0;
        current.push(ch);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Classify a single field. A field may be 1-N words; we try ship matches
/// greedily across the whole field, then sub-word matches, finally leaving
/// any unmatched remainder as a Word (pilot-name candidate).
fn classify_field(field: &str, lookups: &Lookups) -> Vec<Tok> {
    let words: Vec<&str> = field.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        // Greedy multi-word ship match.
        let max_len = lookups.max_ship_words.min(words.len() - i);
        let mut matched = false;
        for n in (1..=max_len).rev() {
            let phrase = words[i..i + n].join(" ").to_lowercase();
            if let Some(&type_id) = lookups.ships.get(&phrase) {
                out.push(Tok::Ship(type_id));
                i += n;
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        let raw = words[i];
        i += 1;
        let core = strip_decorations(raw);
        if core.is_empty() {
            // Token like a bare "*" or "()".
            if is_clear_marker(raw) {
                out.push(Tok::Clear);
            }
            continue;
        }
        if let Some(n) = parse_count(raw) {
            out.push(Tok::Count(n));
            continue;
        }
        if is_clear_marker(&core) {
            out.push(Tok::Clear);
            continue;
        }
        if core.eq_ignore_ascii_case("nv") {
            out.push(Tok::NoVisual);
            continue;
        }
        if let Some(&sys) = lookups.systems.get(&core.to_lowercase()) {
            out.push(Tok::System(sys));
            continue;
        }
        // Try ship match on a single decorated token (e.g. "Cyclone").
        if let Some(&ship) = lookups.ships.get(&core.to_lowercase()) {
            out.push(Tok::Ship(ship));
            continue;
        }
        out.push(Tok::Word(core));
    }

    // Coalesce consecutive Word tokens within the field into a single
    // pilot string. Across fields we already get separation from the
    // outer `split_fields` boundary.
    coalesce_words(out)
}

fn coalesce_words(toks: Vec<Tok>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    let mut buf: Option<String> = None;
    for t in toks {
        match t {
            Tok::Word(w) => {
                if let Some(b) = buf.as_mut() {
                    b.push(' ');
                    b.push_str(&w);
                } else {
                    buf = Some(w);
                }
            }
            other => {
                if let Some(b) = buf.take() {
                    out.push(Tok::Word(b));
                }
                out.push(other);
            }
        }
    }
    if let Some(b) = buf.take() {
        out.push(Tok::Word(b));
    }
    out
}

fn strip_decorations(raw: &str) -> String {
    let mut s = raw.trim_matches(|c: char| c == '(' || c == ')' || c == ',' || c == '.' || c == ':' || c == ';');
    // Trailing `*` (system "adjacent" marker) and trailing `?`.
    while s.ends_with('*') || s.ends_with('?') {
        s = &s[..s.len() - 1];
    }
    // Leading `+` and `-` (handled separately as count / clear).
    s.to_string()
}

fn is_clear_marker(s: &str) -> bool {
    matches!(s, "-" | "сдк" | "СДК")
        || s.eq_ignore_ascii_case("clr")
        || s.eq_ignore_ascii_case("clear")
}

/// `+3`, `3+`, `+nv` etc. We accept either a leading or trailing `+`.
fn parse_count(raw: &str) -> Option<u16> {
    let trimmed = raw.trim_matches('*').trim();
    let body = trimmed
        .strip_prefix('+')
        .or_else(|| trimmed.strip_suffix('+'))?;
    body.parse::<u16>().ok().filter(|&n| n > 0 && n < 1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 8, 11, 17, 30, 0).unwrap()
    }

    fn lookups() -> Lookups {
        let mut systems = HashMap::new();
        for (id, name) in [
            (30001, "R-YWID"),
            (30002, "V7-MID"),
            (30003, "4DTQ-K"),
            (30004, "J52-BH"),
            (30005, "1QH-0K"),
            (30006, "Q1U-IU"),
            (30007, "KMV-CQ"),
            (30008, "9-B1DS"),
            (30009, "XM-4L0"),
            (30010, "S-KSWL"),
        ] {
            systems.insert(name.to_lowercase(), id);
        }
        let mut ships = HashMap::new();
        for (id, name) in [
            (1001, "Cheetah"),
            (1002, "Loki"),
            (1003, "Cyclone"),
            (1004, "Magnate"),
            (1005, "Stabber Fleet Issue"),
            (1006, "Exequror Navy Issue"),
            (1007, "Harpy"),
        ] {
            ships.insert(name.to_lowercase(), id);
        }
        Lookups::new(systems, ships)
    }

    fn extract_one(body: &str) -> Sighting {
        let mut out = extract(now(), "wc.north", "Tester", body, &lookups());
        assert_eq!(out.len(), 1, "expected 1 sighting from {body:?}, got {out:#?}");
        out.remove(0)
    }

    #[test]
    fn system_and_single_word_pilot() {
        let s = extract_one("R-YWID  DrogeOne");
        assert_eq!(s.system_id, Some(30001));
        assert_eq!(s.pilots, vec!["DrogeOne"]);
        assert!(!s.is_clear);
    }

    #[test]
    fn system_and_two_word_pilot_with_nv() {
        let s = extract_one("V7-MID  Ed Stonler nv");
        assert_eq!(s.system_id, Some(30002));
        assert_eq!(s.pilots, vec!["Ed Stonler"]);
        assert!(s.no_visual);
    }

    #[test]
    fn out_of_order_ship_pilot_system() {
        let s = extract_one("Cheetah  AshiPze  J52-BH");
        assert_eq!(s.system_id, Some(30004));
        assert_eq!(s.ship_type_id, Some(1001));
        assert_eq!(s.pilots, vec!["AshiPze"]);
    }

    #[test]
    fn clear_dash_and_clr_and_cdk() {
        let a = extract_one("4DTQ-K* -");
        assert!(a.is_clear);
        let b = extract_one("V7-MID clr");
        assert!(b.is_clear);
        let c = extract_one("XM-4L0* сдк");
        assert!(c.is_clear);
    }

    #[test]
    fn fleet_count() {
        let s = extract_one("Q1U-IU*  Eymur Musana +7");
        assert_eq!(s.fleet_count, Some(7));
        assert_eq!(s.pilots, vec!["Eymur Musana"]);
        assert_eq!(s.system_id, Some(30006));
    }

    #[test]
    fn trailing_plus_count() {
        let s = extract_one("Bodyan Pacyan  Dominator1990  Genzel2  LancknehT  Michael Jakson   S-KSWL 6+");
        assert_eq!(s.system_id, Some(30010));
        assert_eq!(s.fleet_count, Some(6));
        // Five double-space-separated pilots:
        assert_eq!(
            s.pilots,
            vec!["Bodyan Pacyan", "Dominator1990", "Genzel2", "LancknehT", "Michael Jakson"]
        );
    }

    #[test]
    fn multi_word_ship_greedy() {
        let s = extract_one("XM-4L0  jie-rui  Stabber Fleet Issue");
        assert_eq!(s.ship_type_id, Some(1005));
        assert_eq!(s.system_id, Some(30009));
        assert_eq!(s.pilots, vec!["jie-rui"]);
    }

    #[test]
    fn follow_up_with_no_system() {
        let s = extract_one("s1000gt (Cyclone)");
        assert_eq!(s.system_id, None);
        assert_eq!(s.ship_type_id, Some(1003));
        assert_eq!(s.pilots, vec!["s1000gt"]);
        assert!(s.parse_confidence < 0.5);
    }

    #[test]
    fn asterisk_suffix_on_system() {
        let s = extract_one("9-B1DS*  AshiPze");
        assert_eq!(s.system_id, Some(30008));
        assert_eq!(s.pilots, vec!["AshiPze"]);
    }

    #[test]
    fn multiple_systems_emit_multiple_sightings() {
        let out = extract(now(), "wc.north", "T", "R-YWID  V7-MID", &lookups());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].system_id, Some(30001));
        assert_eq!(out[1].system_id, Some(30002));
    }

    #[test]
    fn empty_body_safe() {
        let out = extract(now(), "wc.north", "T", "   ", &lookups());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].system_id, None);
        assert!(out[0].pilots.is_empty());
    }
}
