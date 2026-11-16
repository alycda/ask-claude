use serde::Deserialize;
use serde_json::Value;

/// One event of the stream. Every line of every `sse/*.sse` fixture
/// deserializes, and nothing is swallowed: `kind()` is the JSON `type`.
/// (A placeholder shape: the port may make this an enum, as long as
/// `parse` and `kind` keep their meaning.)
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub rest: Value,
}

impl Event {
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

/// The JSON of every `data: ` line of a raw SSE body, one per line, the
/// way the bash loop strips them.
pub fn data_lines(raw: &str) -> Vec<&str> {
    raw.lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .collect()
}

/// One `data:` payload into an [`Event`].
pub fn parse(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str(line)
}

/// A raw body off the wire or a fixture, as the text jq reads: valid UTF-8
/// comes back as it is, and invalid UTF-8 inside a JSON string is replaced
/// the way jq replaces it while it parses the line, so each `data:` line
/// parses to jq's string. jq undoes a string's escapes first and replaces
/// on the bytes that leaves (`found_string` in jq's `src/jv_parse.c`, then
/// `jvp_utf8_next` in `src/jv_unicode.c`), a rule that is not
/// `String::from_utf8_lossy`'s; see [`jq_lossy`]. Bytes outside a string
/// are never valid JSON there, so they get `from_utf8_lossy` and the line
/// fails to parse, as it does in jq.
pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes
            .split_inclusive(|&b| b == b'\n')
            .map(decode_line)
            .collect(),
    }
}

/// One line of [`decode`]: each JSON string in it through [`json_string`].
fn decode_line(line: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(line) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.iter().position(|&b| b == b'"') {
        out.push_str(&String::from_utf8_lossy(&rest[..open]));
        let body = &rest[open + 1..];
        let mut close = None;
        let mut i = 0;
        while i < body.len() {
            match body[i] {
                b'\\' => i += 2,
                b'"' => {
                    close = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        let Some(close) = close else {
            // an unterminated string: jq refuses the line, and so will serde
            out.push('"');
            out.push_str(&String::from_utf8_lossy(body));
            return out;
        };
        out.push_str(&json_string(&body[..close]));
        rest = &body[close + 1..];
    }
    out.push_str(&String::from_utf8_lossy(rest));
    out
}

/// The contents of one JSON string, quotes excluded, as the JSON string
/// literal that parses to what jq makes of them. A raw control character or
/// a bad escape makes jq refuse the line; those stay as they are, so serde
/// refuses it too.
fn json_string(raw: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(raw) {
        return format!("\"{text}\"");
    }
    match unescape(raw).filter(|_| raw.iter().all(|&b| b >= 0x20)) {
        Some(bytes) => Value::String(jq_lossy(&bytes)).to_string(),
        None => format!("\"{}\"", String::from_utf8_lossy(raw)),
    }
}

/// A JSON string's escapes undone into bytes, jq 1.8's way: a lone `\u`
/// low surrogate is U+FFFD, and a high one with no low one after it is an
/// escape jq refuses, like any other; None on those.
fn unescape(raw: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while let Some(&c) = raw.get(i) {
        i += 1;
        if c != b'\\' {
            out.push(c);
            continue;
        }
        let e = *raw.get(i)?;
        i += 1;
        let byte = match e {
            b'"' | b'\\' | b'/' => e,
            b'b' => 0x08,
            b'f' => 0x0c,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'u' => {
                let mut cp = hex4(raw.get(i..i + 4)?)?;
                i += 4;
                if (0xD800..=0xDBFF).contains(&cp) {
                    let lo = raw
                        .get(i..i + 6)
                        .filter(|r| r.starts_with(b"\\u"))
                        .and_then(|r| hex4(&r[2..]))
                        .filter(|lo| (0xDC00..=0xDFFF).contains(lo))?;
                    i += 6;
                    cp = 0x10000 + (((cp - 0xD800) << 10) | (lo - 0xDC00));
                }
                let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                out.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
                continue;
            }
            _ => return None,
        };
        out.push(byte);
    }
    Some(out)
}

/// Four hex digits, or None.
pub(crate) fn hex4(digits: &[u8]) -> Option<u32> {
    if digits.len() != 4 || !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
}

/// Bytes into text the way jq's `jvp_utf8_next` reads them, one U+FFFD per
/// bad sequence. A lead byte (C2..DF, E0..EF, F0..F4) reads as many bytes
/// as it promises and stops at the first that is not a continuation; what
/// it read is one U+FFFD when the sequence breaks off, is overlong, is a
/// surrogate or lies past U+10FFFF, where `from_utf8_lossy` gives one per
/// byte for the last three. A lead byte the input ends before its sequence
/// could is one U+FFFD for all that is left. Any other byte that is not
/// ASCII is a U+FFFD of its own.
fn jq_lossy(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&first) = bytes.get(i) {
        let len = match first {
            0x00..=0x7F => {
                out.push(char::from(first));
                i += 1;
                continue;
            }
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => {
                out.push('\u{FFFD}');
                i += 1;
                continue;
            }
        };
        let Some(seq) = bytes.get(i..i + len) else {
            out.push('\u{FFFD}');
            break;
        };
        let mut cp = Some(u32::from(first) & (0x7F >> len));
        let mut read = len;
        for (k, &b) in seq.iter().enumerate().skip(1) {
            if b & 0xC0 != 0x80 {
                cp = None;
                read = k;
                break;
            }
            cp = cp.map(|c| (c << 6) | u32::from(b & 0x3F));
        }
        let least = [0, 0, 0x80, 0x800, 0x10000][len];
        let ch = cp.filter(|&c| c >= least).and_then(char::from_u32);
        out.push(ch.unwrap_or('\u{FFFD}'));
        i += read;
    }
    out
}
