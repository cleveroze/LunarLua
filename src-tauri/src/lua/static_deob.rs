//! Static deobfuscation passes: literal clean-up, string-table resolution,
//! decoder inlining and dead-weight removal.

use super::lexer::{lex, to_literal, Tok, Token};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct PassStats {
    pub strings: usize,
    pub junk: usize,
}

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

fn apply(src: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by(|a, b| b.start.cmp(&a.start));
    let mut out = src.to_string();
    let mut last_start = usize::MAX;
    for e in edits {
        if e.end > last_start {
            continue; // overlapping edit, skip
        }
        out.replace_range(e.start..e.end, &e.text);
        last_start = e.start;
    }
    out
}

fn matching(tokens: &[Token], open_idx: usize, open: &str, close: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(open_idx) {
        if t.is_op(open) {
            depth += 1;
        } else if t.is_op(close) {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn count_name(tokens: &[Token], name: &str) -> usize {
    tokens.iter().filter(|t| t.name() == Some(name)).count()
}

/// `"\72\101\108\108\111"` -> `"Hello"` for literals that use escapes.
pub fn canonicalize_strings(src: &str) -> (String, usize) {
    let tokens = lex(src);
    let mut edits = Vec::new();
    let mut count = 0;
    for t in &tokens {
        if let Tok::Str(value) = &t.kind {
            if t.text.contains('\\') {
                let lit = to_literal(value);
                if lit != t.text {
                    edits.push(Edit { start: t.start, end: t.end, text: lit });
                    count += 1;
                }
            }
        }
    }
    (apply(src, edits), count)
}

/// Fold `string.char(72, 105)` and literal `..` concatenations.
pub fn fold_literals(src: &str) -> (String, usize) {
    let tokens = lex(src);
    let mut edits: Vec<Edit> = Vec::new();
    let mut count = 0usize;

    let mut i = 0usize;
    while i < tokens.len() {
        // string.char(...)
        let is_char = tokens[i].name() == Some("string")
            && tokens.get(i + 1).map_or(false, |t| t.is_op("."))
            && tokens.get(i + 2).and_then(|t| t.name()) == Some("char")
            && tokens.get(i + 3).map_or(false, |t| t.is_op("("));
        if is_char {
            if let Some(close) = matching(&tokens, i + 3, "(", ")") {
                let mut bytes: Vec<u8> = Vec::new();
                let mut ok = true;
                let mut j = i + 4;
                while j < close {
                    match tokens[j].num() {
                        Some(n) => match n.parse::<f64>() {
                            Ok(v) if v >= 0.0 && v <= 255.0 => bytes.push(v as u8),
                            _ => {
                                ok = false;
                                break;
                            }
                        },
                        None => {
                            ok = false;
                            break;
                        }
                    }
                    j += 1;
                    if j < close {
                        if tokens[j].is_op(",") {
                            j += 1;
                        } else if !tokens[j].is_op(")") {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok && !bytes.is_empty() {
                    let s = String::from_utf8_lossy(&bytes).to_string();
                    edits.push(Edit { start: tokens[i].start, end: tokens[close].end, text: to_literal(&s) });
                    count += 1;
                    i = close + 1;
                    continue;
                }
            }
        }

        // literal concat: "a" .. "b" .. 1
        if matches!(tokens[i].kind, Tok::Str(_) | Tok::Num(_)) {
            let mut j = i;
            let mut parts: Vec<String> = Vec::new();
            let mut all_str = true;
            loop {
                match &tokens[j].kind {
                    Tok::Str(s) => parts.push(s.clone()),
                    Tok::Num(n) => {
                        all_str = false;
                        parts.push(n.clone());
                    }
                    _ => break,
                }
                let next = j + 1;
                if tokens.get(next).map_or(false, |t| t.is_op("..")) && tokens.get(next + 1).is_some() {
                    j = next + 1;
                    continue;
                }
                break;
            }
            if j > i + 1 {
                if all_str {
                    let joined = parts.concat();
                    edits.push(Edit { start: tokens[i].start, end: tokens[j].end, text: to_literal(&joined) });
                } else {
                    let joined = parts.concat();
                    edits.push(Edit { start: tokens[i].start, end: tokens[j].end, text: to_literal(&joined) });
                }
                count += 1;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }

    (apply(src, edits), count)
}

/// Resolve `local t = {"a", "b"}` + `t[2]` and decoder functions
/// `local function d(i) return t[i] end` + `d(2)`.
pub fn resolve_string_tables(src: &str) -> (String, usize) {
    let tokens = lex(src);
    let mut tables: HashMap<String, (Vec<String>, usize, usize)> = HashMap::new();

    // find table declarations
    let mut i = 0usize;
    while i + 3 < tokens.len() {
        let is_local = tokens[i].is_kw("local");
        if !is_local {
            i += 1;
            continue;
        }
        let Some(name) = tokens[i + 1].name() else {
            i += 1;
            continue;
        };
        if !tokens[i + 2].is_op("=") || !tokens[i + 3].is_op("{") {
            i += 1;
            continue;
        }
        let Some(close) = matching(&tokens, i + 3, "{", "}") else {
            i += 1;
            continue;
        };
        let mut values: Vec<String> = Vec::new();
        let mut ok = true;
        let mut j = i + 4;
        while j < close {
            match &tokens[j].kind {
                Tok::Str(s) => values.push(s.clone()),
                Tok::Op(o) if o == "," || o == ";" => {}
                _ => {
                    ok = false;
                    break;
                }
            }
            j += 1;
        }
        if ok && !values.is_empty() {
            tables.insert(name.to_string(), (values, tokens[i].start, tokens[close].end));
        }
        i = close + 1;
    }

    if tables.is_empty() {
        return (src.to_string(), 0);
    }

    // decoder functions: function NAME(P) return TABLE[P] end
    let mut decoders: HashMap<String, (String, usize, usize)> = HashMap::new();
    let mut i = 0usize;
    while i + 7 < tokens.len() {
        if !tokens[i].is_kw("function") {
            i += 1;
            continue;
        }
        let Some(fname) = tokens[i + 1].name() else {
            i += 1;
            continue;
        };
        if !tokens[i + 2].is_op("(") {
            i += 1;
            continue;
        }
        let Some(pclose) = matching(&tokens, i + 2, "(", ")") else {
            i += 1;
            continue;
        };
        let Some(param) = tokens.get(i + 3).and_then(|t| t.name()) else {
            i += 1;
            continue;
        };
        if tokens.get(pclose + 1).map_or(true, |t| !t.is_kw("return")) {
            i += 1;
            continue;
        }
        let tname = tokens.get(pclose + 2).and_then(|t| t.name());
        let ok = matches!(
            (tname, tokens.get(pclose + 3), tokens.get(pclose + 4), tokens.get(pclose + 5)),
            (Some(t), Some(o1), Some(p), Some(o2))
                if t != param
                    && o1.is_op("[")
                    && p.name() == Some(param)
                    && o2.is_op("]")
                    && tokens.get(pclose + 6).map_or(false, |e| e.is_kw("end"))
        );
        if ok {
            let t = tname.unwrap().to_string();
            if tables.contains_key(&t) {
                let stmt_start = if i >= 1 && tokens[i - 1].is_kw("local") { tokens[i - 1].start } else { tokens[i].start };
                decoders.insert(fname.to_string(), (t, stmt_start, tokens[pclose + 6].end));
            }
        }
        i = pclose + 1;
    }

    // rewrite accesses
    let mut edits = Vec::new();
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut i = 0usize;
    while i < tokens.len() {
        // NAME [ NUM ]
        let direct = tokens[i].name().and_then(|n| {
            tables.get(n).map(|(vals, _, _)| (n.to_string(), vals.clone()))
        });
        if let Some((tname, vals)) = direct {
            if tokens.get(i + 1).map_or(false, |t| t.is_op("[")) {
                if let Some(idx) = tokens.get(i + 2).and_then(|t| t.num()).and_then(|n| n.parse::<usize>().ok()) {
                    if tokens.get(i + 3).map_or(false, |t| t.is_op("]")) {
                        // make sure it is not an assignment target
                        let assigned = tokens.get(i + 4).map_or(false, |t| t.is_op("="));
                        if !assigned {
                            if let Some(v) = vals.get(idx.wrapping_sub(1)) {
                                edits.push(Edit { start: tokens[i].start, end: tokens[i + 3].end, text: to_literal(v) });
                                *used.entry(tname).or_default() += 1;
                                i += 4;
                                continue;
                            }
                        }
                    }
                }
            }
        }

        // DECODER ( NUM )
        if let Some(fname) = tokens[i].name() {
            if let Some((tname, _ds, _de)) = decoders.get(fname) {
                if tokens.get(i + 1).map_or(false, |t| t.is_op("("))
                    && tokens.get(i + 3).map_or(false, |t| t.is_op(")"))
                {
                    if let Some(idx) = tokens.get(i + 2).and_then(|t| t.num()).and_then(|n| n.parse::<usize>().ok()) {
                        if let Some((vals, _, _)) = tables.get(tname) {
                            if let Some(v) = vals.get(idx.wrapping_sub(1)) {
                                edits.push(Edit { start: tokens[i].start, end: tokens[i + 3].end, text: to_literal(v) });
                                *used.entry(tname.clone()).or_default() += 1;
                                i += 4;
                                continue;
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    let total: usize = used.values().sum();

    let mut covered: Vec<(usize, usize)> = edits.iter().map(|e| (e.start, e.end)).collect();

    // drop decoder functions that ended up with no call sites
    for (fname, (_t, dstart, dend)) in &decoders {
        if count_refs(&tokens, fname, &covered, (*dstart, *dend)) == 0 {
            edits.push(Edit { start: *dstart, end: *dend, text: String::new() });
            covered.push((*dstart, *dend));
        }
    }

    // drop declarations that became unreachable
    for (name, (_vals, start, end)) in &tables {
        if count_refs(&tokens, name, &covered, (*start, *end)) == 0 {
            edits.push(Edit { start: *start, end: *end, text: String::new() });
        }
    }
    (apply(src, edits), total)
}

/// References to `name` outside of already-rewritten regions and outside the
/// declaration that defines it.
fn count_refs(tokens: &[Token], name: &str, covered: &[(usize, usize)], own: (usize, usize)) -> usize {
    tokens
        .iter()
        .filter(|t| t.name() == Some(name))
        .filter(|t| t.start < own.0 || t.end > own.1)
        .filter(|t| !covered.iter().any(|(s, e)| t.start >= *s && t.end <= *e))
        .count()
}

/// Remove `local x = <pure literal>` declarations that are never referenced.
pub fn strip_junk(src: &str) -> (String, usize) {
    let tokens = lex(src);
    let mut edits = Vec::new();
    let mut count = 0usize;
    let mut i = 0usize;
    while i + 2 < tokens.len() {
        if tokens[i].is_kw("local") {
            if let Some(name) = tokens[i + 1].name() {
                if tokens[i + 2].is_op("=") && count_name(&tokens, name) == 1 {
                    // consume a simple literal initializer
                    let mut j = i + 3;
                    let mut pure = true;
                    let mut depth = 0i32;
                    let mut saw = false;
                    while j < tokens.len() {
                        let t = &tokens[j];
                        if t.is_op("(") || t.is_op("{") || t.is_op("[") {
                            depth += 1;
                        } else if t.is_op(")") || t.is_op("}") || t.is_op("]") {
                            depth -= 1;
                        }
                        match &t.kind {
                            Tok::Num(_) | Tok::Str(_) | Tok::Kw("true") | Tok::Kw("false") | Tok::Kw("nil") => saw = true,
                            Tok::Op(o) if o == "-" || o == "," || o == "{" || o == "}" || o == ";" => {}
                            _ => {
                                pure = false;
                                break;
                            }
                        }
                        if depth <= 0 && saw && j + 1 < tokens.len() {
                            let nxt = &tokens[j + 1];
                            let ends = nxt.is_op(";")
                                || nxt.is_kw("local")
                                || nxt.is_kw("if")
                                || nxt.is_kw("for")
                                || nxt.is_kw("while")
                                || nxt.is_kw("return")
                                || nxt.is_kw("end")
                                || nxt.is_kw("function")
                                || matches!(nxt.kind, Tok::Eof);
                            if ends {
                                edits.push(Edit { start: tokens[i].start, end: tokens[j].end, text: String::new() });
                                count += 1;
                                i = j + 1;
                                break;
                            }
                        }
                        j += 1;
                    }
                    if !pure {
                        i += 1;
                    }
                    continue;
                }
            }
        }
        i += 1;
    }
    (apply(src, edits), count)
}

/// Strip comments (used by minify).
pub fn strip_comments(src: &str) -> String {
    let tokens = lex(src);
    let mut out = String::with_capacity(src.len());
    let mut last = 0usize;
    for t in &tokens {
        if let Tok::Comment(_) = t.kind {
            out.push_str(&src[last..t.start]);
            last = t.end;
        }
    }
    out.push_str(&src[last..]);
    out
}
