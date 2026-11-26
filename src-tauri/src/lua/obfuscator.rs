//! A real, conservative Lua obfuscator: identifier renaming, string encryption
//! with a generated decoder, optional noise and minification.

use super::format::beautify;
use super::lexer::{lex, to_literal, Tok, Token};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct ObfOptions {
    pub rename: bool,
    pub encrypt: bool,
    pub noise: bool,
    pub minify: bool,
    pub seed: String,
}

impl Default for ObfOptions {
    fn default() -> Self {
        Self { rename: true, encrypt: true, noise: false, minify: false, seed: "lunar".into() }
    }
}

fn hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

const RESERVED: &[&str] = &["self", "_G", "_ENV", "print", "game", "script", "workspace", "require"];

fn is_field_access(tokens: &[Token], i: usize) -> bool {
    i > 0 && (tokens[i - 1].is_op(".") || tokens[i - 1].is_op(":"))
}

fn brace_depths(tokens: &[Token]) -> Vec<i32> {
    let mut depth = 0i32;
    let mut out = Vec::with_capacity(tokens.len());
    for t in tokens {
        if t.is_op("{") {
            depth += 1;
        }
        out.push(depth);
        if t.is_op("}") {
            depth -= 1;
        }
    }
    out
}

fn is_table_key(tokens: &[Token], depths: &[i32], i: usize) -> bool {
    // `{ name = ... }` is a key, `local name = ...` is a declaration
    if i + 1 >= tokens.len() || !tokens[i + 1].is_op("=") {
        return false;
    }
    depths[i] > 0 && i > 0 && (tokens[i - 1].is_op("{") || tokens[i - 1].is_op(",") || tokens[i - 1].is_op(";"))
}

fn collect_locals(tokens: &[Token]) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut i = 0usize;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.is_kw("local") {
            let mut j = i + 1;
            if tokens.get(j).map_or(false, |x| x.is_kw("function")) {
                j += 1;
            }
            while j < tokens.len() {
                if let Some(n) = tokens[j].name() {
                    set.insert(n.to_string());
                    j += 1;
                    if tokens.get(j).map_or(false, |x| x.is_op(",")) {
                        j += 1;
                        continue;
                    }
                }
                break;
            }
        } else if t.is_kw("function") {
            // parameters
            let mut j = i + 1;
            if tokens.get(j).and_then(|x| x.name()).is_some() {
                j += 1; // named function
            }
            if tokens.get(j).map_or(false, |x| x.is_op("(")) {
                j += 1;
                while j < tokens.len() && !tokens[j].is_op(")") {
                    if let Some(n) = tokens[j].name() {
                        set.insert(n.to_string());
                    }
                    j += 1;
                }
            }
        } else if t.is_kw("for") {
            let mut j = i + 1;
            while j < tokens.len() {
                if let Some(n) = tokens[j].name() {
                    set.insert(n.to_string());
                    j += 1;
                    if tokens.get(j).map_or(false, |x| x.is_op(",")) {
                        j += 1;
                        continue;
                    }
                }
                break;
            }
        }
        i += 1;
    }
    set
}

/// Rename local identifiers in place.
fn rename_locals(src: &str, seed: &str) -> (String, usize) {
    let tokens = lex(src);
    let locals = collect_locals(&tokens);
    if locals.is_empty() {
        return (src.to_string(), 0);
    }

    let depths = brace_depths(&tokens);
    let all_names: HashSet<&str> = tokens.iter().filter_map(|t| t.name()).collect();
    let mut mapping: HashMap<String, String> = HashMap::new();
    let mut counter = 0usize;
    let mut salt = 0u64;
    let mut ordered: Vec<&String> = locals.iter().collect();
    ordered.sort();
    for name in ordered {
        if RESERVED.contains(&name.as_str()) {
            continue;
        }
        // skip names that are also used as table fields / methods
        let used_as_field = tokens
            .iter()
            .enumerate()
            .any(|(i, t)| t.name() == Some(name.as_str()) && (is_field_access(&tokens, i) || is_table_key(&tokens, &depths, i)));
        if used_as_field && !name.starts_with("__") {
            continue;
        }
        let mut new_name;
        loop {
            salt = salt.wrapping_add(1);
            let h = hash(&format!("{seed}:{name}:{salt}"));
            new_name = format!("l{:x}", (h ^ (counter as u64)) % 0xffff);
            counter += 1;
            if !all_names.contains(new_name.as_str()) {
                break;
            }
        }
        mapping.insert(name.clone(), new_name);
    }

    if mapping.is_empty() {
        return (src.to_string(), 0);
    }

    let mut out = String::with_capacity(src.len());
    let mut last = 0usize;
    for (i, t) in tokens.iter().enumerate() {
        if let Some(n) = t.name() {
            if !is_field_access(&tokens, i) {
                if let Some(new_name) = mapping.get(n) {
                    out.push_str(&src[last..t.start]);
                    out.push_str(new_name);
                    last = t.end;
                }
            }
        }
    }
    out.push_str(&src[last..]);
    (out, mapping.len())
}

/// Encrypt every short string literal behind a generated decoder.
fn encrypt_strings(src: &str, seed: &str) -> (String, usize) {
    let tokens = lex(src);
    let key: u8 = (hash(seed) % 53 + 11) as u8;
    let tag = (hash(&format!("{seed}/tag")) % 0xffff) as u16;
    let tbl = format!("__t{tag:x}");
    let dec = format!("__d{tag:x}");

    let mut pool: Vec<String> = Vec::new();
    let mut out = String::with_capacity(src.len());
    let mut last = 0usize;
    let mut count = 0usize;

    for t in &tokens {
        if let Tok::Str(value) = &t.kind {
            // keep the generated prelude untouched (it has no string literals anyway)
            if value.is_empty() && t.text.is_empty() {
                continue;
            }
            let encoded: Vec<u8> = value.bytes().map(|b| ((b as u16 + key as u16) % 256) as u8).collect();
            pool.push(bytes_literal(&encoded));
            out.push_str(&src[last..t.start]);
            out.push_str(&format!("{dec}({})", pool.len()));
            last = t.end;
            count += 1;
        }
    }
    if count == 0 {
        return (src.to_string(), 0);
    }
    out.push_str(&src[last..]);

    let mut prelude = String::new();
    prelude.push_str(&format!("local {tbl}={{"));
    for (i, s) in pool.iter().enumerate() {
        if i > 0 {
            prelude.push(',');
        }
        prelude.push_str(s);
    }
    prelude.push_str("}\n");
    prelude.push_str(&format!(
        "local {dec}=function(i)local s={tbl}[i]local o={{}}for k=1,#s do o[k]=string.char((string.byte(s,k)-{key})%256)end return table.concat(o)end\n"
    ));

    (format!("{prelude}{out}"), count)
}

/// Prepend opaque but harmless statements.
fn add_noise(src: &str, seed: &str) -> String {
    let h = hash(seed) as u32;
    let a = (h % 900 + 100) as u32;
    let b = (h / 7 % 900 + 100) as u32;
    let mut junk = String::new();
    junk.push_str(&format!("local __n1=0x{a:x}\n"));
    junk.push_str(&format!("local __n2={b}\n"));
    junk.push_str("if __n1>999999 then __n1=__n1*3+__n2 end\n");
    junk.push_str("do local __n3=__n1-__n1 __n2=__n2+__n3 end\n");
    format!("{junk}{src}")
}

/// Exact byte literal: everything outside printable ASCII becomes `\ddd`,
/// so the runtime string keeps the original bytes.
fn bytes_literal(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{:03}", b)),
        }
    }
    out.push('"');
    out
}

fn word_like(t: &Token) -> bool {
    matches!(t.kind, Tok::Name(_) | Tok::Num(_) | Tok::Kw(_))
}

pub fn minify(src: &str) -> String {
    let tokens = lex(src);
    let mut out = String::with_capacity(src.len());
    let mut prev: Option<&Token> = None;
    for t in &tokens {
        if matches!(t.kind, Tok::Eof | Tok::Comment(_)) {
            continue;
        }
        if let Some(p) = prev {
            let need = (word_like(p) && word_like(t))
                || (p.text.ends_with('-') && t.text.starts_with('-'))
                || (matches!(p.kind, Tok::Num(_)) && t.is_op(".."));
            if need {
                out.push(' ');
            }
        }
        out.push_str(&t.text);
        prev = Some(t);
    }
    out
}

pub fn obfuscate(src: &str, opts: &ObfOptions) -> (String, usize, usize) {
    let mut text = src.to_string();
    let mut renamed = 0usize;
    let mut strings = 0usize;

    if opts.encrypt {
        let (t, n) = encrypt_strings(&text, &opts.seed);
        text = t;
        strings = n;
    }
    if opts.rename {
        let (t, n) = rename_locals(&text, &opts.seed);
        text = t;
        renamed = n;
    }
    if opts.noise {
        text = add_noise(&text, &opts.seed);
    }
    let output = if opts.minify { minify(&text) } else { beautify(&text) };
    (output, renamed, strings)
}
