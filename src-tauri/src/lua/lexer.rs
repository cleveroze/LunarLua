//! A small, forgiving Lua 5.1 / Luau lexer.
//!
//! It is deliberately lossless: every token keeps its byte span so passes can
//! rewrite slices of the original source without re-printing the whole file.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Name(String),
    Num(String),
    Str(String),
    Kw(&'static str),
    Op(String),
    Comment(String),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: Tok,
    /// raw source slice
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
}

impl Token {
    pub fn is_op(&self, op: &str) -> bool {
        matches!(&self.kind, Tok::Op(o) if o == op)
    }
    pub fn is_kw(&self, kw: &str) -> bool {
        matches!(&self.kind, Tok::Kw(k) if *k == kw)
    }
    pub fn name(&self) -> Option<&str> {
        match &self.kind {
            Tok::Name(n) => Some(n),
            _ => None,
        }
    }
    pub fn num(&self) -> Option<&str> {
        match &self.kind {
            Tok::Num(n) => Some(n),
            _ => None,
        }
    }
    pub fn string(&self) -> Option<&str> {
        match &self.kind {
            Tok::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

fn keyword(s: &str) -> Option<&'static str> {
    KEYWORDS.iter().copied().find(|k| *k == s)
}

pub fn lex(src: &str) -> Vec<Token> {
    let b = src.as_bytes();
    let n = b.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut out = Vec::with_capacity(n / 4 + 8);

    while i < n {
        let c = b[i];
        // whitespace
        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        let start = i;
        let start_line = line;

        // comments
        if c == b'-' && i + 1 < n && b[i + 1] == b'-' {
            if let Some((content, end, newlines)) = long_bracket(b, i + 2) {
                line += newlines;
                out.push(Token {
                    kind: Tok::Comment(content),
                    text: src[start..end].to_string(),
                    start,
                    end,
                    line: start_line,
                });
                i = end;
                continue;
            }
            let mut j = i;
            while j < n && b[j] != b'\n' {
                j += 1;
            }
            out.push(Token {
                kind: Tok::Comment(src[i..j].to_string()),
                text: src[i..j].to_string(),
                start,
                end: j,
                line: start_line,
            });
            i = j;
            continue;
        }

        // long strings
        if c == b'[' || (c == b'=' && false) {
            if let Some((content, end, newlines)) = long_bracket(b, i) {
                line += newlines;
                out.push(Token {
                    kind: Tok::Str(content),
                    text: src[start..end].to_string(),
                    start,
                    end,
                    line: start_line,
                });
                i = end;
                continue;
            }
        }

        // quoted strings
        if c == b'"' || c == b'\'' {
            let quote = c;
            let mut j = i + 1;
            let mut value = String::new();
            while j < n {
                let d = b[j];
                if d == b'\\' && j + 1 < n {
                    let (chunk, adv) = read_escape(&b[j..]);
                    value.push_str(&chunk);
                    j += adv;
                    continue;
                }
                if d == quote {
                    j += 1;
                    break;
                }
                if d == b'\n' {
                    line += 1;
                }
                // keep utf-8 sequences intact
                let ch_len = utf8_len(b, j);
                value.push_str(&src[j..(j + ch_len).min(n)]);
                j += ch_len;
            }
            out.push(Token {
                kind: Tok::Str(value),
                text: src[start..j].to_string(),
                start,
                end: j,
                line: start_line,
            });
            i = j;
            continue;
        }

        // numbers
        if c.is_ascii_digit() || (c == b'.' && i + 1 < n && b[i + 1].is_ascii_digit()) {
            let mut j = i;
            if c == b'0' && i + 1 < n && (b[i + 1] == b'x' || b[i + 1] == b'X') {
                j += 2;
                while j < n && (b[j].is_ascii_hexdigit() || b[j] == b'.' || b[j] == b'p' || b[j] == b'P') {
                    j += 1;
                }
            } else {
                while j < n && (b[j].is_ascii_digit() || b[j] == b'.') {
                    j += 1;
                }
                if j < n && (b[j] == b'e' || b[j] == b'E') {
                    j += 1;
                    if j < n && (b[j] == b'+' || b[j] == b'-') {
                        j += 1;
                    }
                    while j < n && b[j].is_ascii_digit() {
                        j += 1;
                    }
                }
            }
            out.push(Token {
                kind: Tok::Num(src[i..j].to_string()),
                text: src[i..j].to_string(),
                start,
                end: j,
                line: start_line,
            });
            i = j;
            continue;
        }

        // names
        if c == b'_' || c.is_ascii_alphabetic() {
            let mut j = i + 1;
            while j < n && (b[j] == b'_' || b[j].is_ascii_alphanumeric()) {
                j += 1;
            }
            let word = &src[i..j];
            let kind = match keyword(word) {
                Some(k) => Tok::Kw(k),
                None => Tok::Name(word.to_string()),
            };
            out.push(Token {
                kind,
                text: word.to_string(),
                start,
                end: j,
                line: start_line,
            });
            i = j;
            continue;
        }

        // operators
        let three: &[&str] = &["...", "//=", "..="];
        let two: &[&str] = &["==", "~=", "<=", ">=", "..", "::", "//", "+=", "-=", "*=", "/=", "%=", "^=", "->"];
        let mut matched = None;
        for op in three {
            if src[i..].starts_with(op) {
                matched = Some(*op);
                break;
            }
        }
        if matched.is_none() {
            for op in two {
                if src[i..].starts_with(op) {
                    matched = Some(*op);
                    break;
                }
            }
        }
        let oplen = matched.map(|s| s.len()).unwrap_or(1);
        let op = &src[i..i + oplen];
        out.push(Token {
            kind: Tok::Op(op.to_string()),
            text: op.to_string(),
            start,
            end: i + oplen,
            line: start_line,
        });
        i += oplen;
    }

    out.push(Token {
        kind: Tok::Eof,
        text: String::new(),
        start: n,
        end: n,
        line,
    });
    out
}

fn utf8_len(b: &[u8], i: usize) -> usize {
    let c = b[i];
    if c < 0x80 {
        1
    } else if c >> 5 == 0b110 {
        2
    } else if c >> 4 == 0b1110 {
        3
    } else if c >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

/// Parses a long bracket string comment/body starting at `at`.
/// Returns (content, end_index, newlines).
fn long_bracket(b: &[u8], at: usize) -> Option<(String, usize, usize)> {
    if at >= b.len() || b[at] != b'[' {
        return None;
    }
    let mut eq = 0usize;
    let mut j = at + 1;
    while j < b.len() && b[j] == b'=' {
        eq += 1;
        j += 1;
    }
    if j >= b.len() || b[j] != b'[' {
        return None;
    }
    let body_start = j + 1;
    let mut k = body_start;
    let mut newlines = 0usize;
    while k < b.len() {
        if b[k] == b'\n' {
            newlines += 1;
        }
        if b[k] == b']' {
            let mut m = k + 1;
            let mut e = 0usize;
            while m < b.len() && b[m] == b'=' {
                e += 1;
                m += 1;
            }
            if e == eq && m < b.len() && b[m] == b']' {
                let content = String::from_utf8_lossy(&b[body_start..k]).to_string();
                return Some((content, m + 1, newlines));
            }
        }
        k += 1;
    }
    None
}

/// Decodes one backslash escape starting at `b[0] == '\\'`.
/// Returns the decoded text and how many bytes were consumed.
fn read_escape(b: &[u8]) -> (String, usize) {
    debug_assert_eq!(b[0], b'\\');
    if b.len() < 2 {
        return ("\\".to_string(), 1);
    }
    match b[1] {
        b'a' => ("\u{7}".to_string(), 2),
        b'b' => ("\u{8}".to_string(), 2),
        b'f' => ("\u{c}".to_string(), 2),
        b'n' => ("\n".to_string(), 2),
        b'r' => ("\r".to_string(), 2),
        b't' => ("\t".to_string(), 2),
        b'v' => ("\u{b}".to_string(), 2),
        b'\\' => ("\\".to_string(), 2),
        b'"' => ("\"".to_string(), 2),
        b'\'' => ("'".to_string(), 2),
        b'\n' => ("\n".to_string(), 2),
        b'x' => {
            let mut v: u32 = 0;
            let mut k = 2;
            let mut digits = 0;
            while k < b.len() && digits < 2 && (b[k] as char).is_ascii_hexdigit() {
                v = v * 16 + (b[k] as char).to_digit(16).unwrap();
                k += 1;
                digits += 1;
            }
            (char::from_u32(v).unwrap_or('\u{fffd}').to_string(), k)
        }
        b'z' => {
            let mut k = 2;
            while k < b.len() && (b[k] as char).is_ascii_whitespace() {
                k += 1;
            }
            (String::new(), k)
        }
        b'u' => {
            if b.len() > 3 && b[2] == b'{' {
                let mut v: u32 = 0;
                let mut k = 3;
                while k < b.len() && b[k] != b'}' {
                    if let Some(d) = (b[k] as char).to_digit(16) {
                        v = v * 16 + d;
                    }
                    k += 1;
                }
                (char::from_u32(v).unwrap_or('\u{fffd}').to_string(), (k + 1).min(b.len()))
            } else {
                ("u".to_string(), 2)
            }
        }
        d if d.is_ascii_digit() => {
            let mut v: u32 = 0;
            let mut k = 1;
            let mut digits = 0;
            while k < b.len() && digits < 3 && b[k].is_ascii_digit() {
                v = v * 10 + (b[k] - b'0') as u32;
                k += 1;
                digits += 1;
            }
            let ch = char::from_u32(v % 256).unwrap_or('\u{fffd}');
            (ch.to_string(), k)
        }
        _other => {
            let len = utf8_len(b, 1);
            (String::from_utf8_lossy(&b[1..(1 + len).min(b.len())]).to_string(), 1 + len)
        }
    }
}

/// Renders a byte value the way a Lua literal would, preferring printable ASCII.
pub fn escape_byte(v: u8, quote: char) -> String {
    match v {
        b'\\' => "\\\\".to_string(),
        b'\n' => "\\n".to_string(),
        b'\r' => "\\r".to_string(),
        b'\t' => "\\t".to_string(),
        0x20..=0x7e => {
            let c = v as char;
            if c == quote {
                format!("\\{c}")
            } else {
                c.to_string()
            }
        }
        _ => format!("\\{v}"),
    }
}

/// Escapes a decoded string into a double-quoted Lua literal.
pub fn to_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                out.push_str(&format!("\\{:03}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
