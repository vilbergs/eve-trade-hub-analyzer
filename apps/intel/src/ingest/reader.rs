//! Read a UTF-16 LE chatlog file into numbered UTF-8 lines.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use chrono::{NaiveDateTime, TimeZone, Utc};
use encoding_rs_io::DecodeReaderBytesBuilder;

use eve_core::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct DecodedFile {
    pub session_started: Option<chrono::DateTime<Utc>>,
    pub lines: Vec<NumberedLine>,
}

#[derive(Debug, Clone)]
pub struct NumberedLine {
    pub line_no: i64,   // 1-indexed across the whole file (header lines included)
    pub text: String,
}

pub fn read_chatlog(path: &Path) -> AppResult<DecodedFile> {
    let file = File::open(path).map_err(AppError::Io)?;
    decode_reader(file)
}

fn decode_reader<R: Read>(reader: R) -> AppResult<DecodedFile> {
    // `DecodeReaderBytesBuilder` defaults to BOM-sniffing, which handles
    // the UTF-16 LE BOM EVE writes and falls back to UTF-8 if missing.
    let decoded = DecodeReaderBytesBuilder::new()
        .bom_sniffing(true)
        .build(reader);
    let mut buf = BufReader::new(decoded);
    let mut text = String::new();
    buf.read_to_string(&mut text).map_err(AppError::Io)?;

    let mut session_started: Option<chrono::DateTime<Utc>> = None;
    let mut lines = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line_no = (i + 1) as i64;
        if session_started.is_none() {
            if let Some(rest) = raw.trim().strip_prefix("Session started:") {
                let ts = NaiveDateTime::parse_from_str(rest.trim(), "%Y.%m.%d %H:%M:%S").ok();
                if let Some(naive) = ts {
                    session_started = Some(Utc.from_utc_datetime(&naive));
                }
            }
        }
        lines.push(NumberedLine {
            line_no,
            text: raw.to_string(),
        });
    }

    Ok(DecodedFile {
        session_started,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn decodes_utf16_le_with_bom() {
        // Build a tiny UTF-16 LE buffer with a session header + one intel line.
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(&[0xFF, 0xFE]); // BOM
        let text = "          Session started: 2025.08.11 17:23:55\n[ 2025.08.11 17:23:58 ] EVE System > MOTD\n[ 2025.08.11 17:27:23 ] rody > 1QH-0K  Richard\n";
        for c in text.encode_utf16() {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
        let decoded = decode_reader(Cursor::new(bytes)).unwrap();
        assert!(decoded.session_started.is_some());
        assert_eq!(decoded.session_started.unwrap().to_rfc3339(), "2025-08-11T17:23:55+00:00");
        assert!(decoded.lines.iter().any(|l| l.text.contains("1QH-0K")));
    }
}
