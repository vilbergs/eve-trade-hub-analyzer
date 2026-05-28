//! Stage 1: turn a decoded chatlog line into a structured `RawLine`.
//!
//! Chatlog body shape:
//!
//! ```text
//! [ 2025.08.11 17:23:58 ] Author Name > body of message
//! ```
//!
//! Lines before the first `[ ... ]` (header block) are header noise and
//! return `None`. The EVE-System author + MOTD line is also skipped.

use chrono::{NaiveDateTime, TimeZone, Utc};

#[derive(Debug, Clone)]
pub struct RawLine {
    pub ts: chrono::DateTime<Utc>,
    pub author: String,
    pub body: String,
}

/// Parse a single decoded line. Returns `None` for headers, blank lines,
/// or `EVE System >` messages.
pub fn parse_line(line: &str) -> Option<RawLine> {
    // Strip BOM if present (only on first line, but cheap to do every time).
    let line = line.trim_start_matches('\u{feff}').trim_end_matches(['\r']);
    let line = line.trim_start();
    if !line.starts_with('[') {
        return None;
    }
    let rest = &line[1..];
    let close = rest.find(']')?;
    let ts_raw = rest[..close].trim();
    // EVE writes the dot-separated date form, in UTC game time.
    let ts = NaiveDateTime::parse_from_str(ts_raw, "%Y.%m.%d %H:%M:%S").ok()?;
    let ts = Utc.from_utc_datetime(&ts);

    let after = rest[close + 1..].trim_start();
    let arrow = after.find('>')?;
    let author = after[..arrow].trim().to_string();
    let body = after[arrow + 1..].trim().to_string();

    if author.eq_ignore_ascii_case("EVE System") {
        return None;
    }
    if body.is_empty() {
        return None;
    }

    Some(RawLine { ts, author, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_intel() {
        let r = parse_line("[ 2025.08.11 17:27:23 ] rody haringman > 1QH-0K  Richard FonCrossberg").unwrap();
        assert_eq!(r.author, "rody haringman");
        assert_eq!(r.body, "1QH-0K  Richard FonCrossberg");
        assert_eq!(r.ts.to_rfc3339(), "2025-08-11T17:27:23+00:00");
    }

    #[test]
    fn skips_header_and_system_messages() {
        assert!(parse_line("---------------------").is_none());
        assert!(parse_line("          Channel ID:      ...").is_none());
        assert!(parse_line("[ 2025.08.11 18:25:13 ] EVE System > Connection to chat server lost").is_none());
    }

    #[test]
    fn strips_bom() {
        let r = parse_line("\u{feff}[ 2025.08.11 17:27:23 ] x > y").unwrap();
        assert_eq!(r.author, "x");
        assert_eq!(r.body, "y");
    }
}
