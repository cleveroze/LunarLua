//! Token based pretty printer for Lua / Luau.

use super::lexer::{lex, Tok, Token};

#[derive(Clone, Copy, PartialEq)]
enum Sep {
    None,
    Space,
    Newline,
}

pub fn beautify(src: &str) -> String {
    let toks = lex(src);
    let tokens: Vec<&Token> = toks.iter().filter(|t| !matches!(t.kind, Tok::Comment(_) | Tok::Eof)).collect();
    let mut out = String::with_capacity(src.len() + src.len() / 3);
    let mut indent: usize = 0;
    let mut paren_depth: usize = 0;
    let mut func_paren: Vec<usize> = Vec::new();
    let mut line_start = true;
    let mut prev: Option<&Token> = None;

    for (idx, tok) in tokens.iter().enumerate() {
        let next = tokens.get(idx + 1).copied();
        let mut sep = separator(prev, tok);

        let closes = tok.is_kw("end") || tok.is_kw("until") || tok.is_kw("else") || tok.is_kw("elseif");
        if closes {
            sep = Sep::Newline;
            indent = indent.saturating_sub(1);
        }

        if is_statement_start(tok) && !is_continuation(prev) && !line_start {
            sep = Sep::Newline;
        }

        // a new statement directly after a finished value: `f() g()`
        if !closes
            && sep != Sep::Newline
            && paren_depth == 0
            && !line_start
            && prev.map_or(false, ends_value)
            && tok.name().is_some()
            && next.map_or(false, |n| n.is_op("(") || n.is_op("{") || matches!(n.kind, Tok::Str(_)))
        {
            sep = Sep::Newline;
        }

        match sep {
            Sep::Newline => {
                if !line_start {
                    out.push('\n');
                }
                line_start = true;
            }
            Sep::Space => {
                if !line_start {
                    out.push(' ');
                }
            }
            Sep::None => {}
        }
        if line_start {
            for _ in 0..indent {
                out.push_str("  ");
            }
            line_start = false;
        }

        out.push_str(&tok.text);

        if tok.is_op("(") {
            paren_depth += 1;
        } else if tok.is_op(")") {
            paren_depth = paren_depth.saturating_sub(1);
            if let Some(open) = func_paren.last().copied() {
                if open == paren_depth {
                    func_paren.pop();
                    out.push('\n');
                    line_start = true;
                    indent += 1;
                }
            }
        } else if tok.is_kw("function") {
            func_paren.push(paren_depth);
        }

        if tok.is_kw("then") || tok.is_kw("do") || tok.is_kw("else") || tok.is_kw("repeat") {
            out.push('\n');
            line_start = true;
            indent += 1;
        } else if tok.is_op(";") {
            out.push('\n');
            line_start = true;
        } else if tok.is_kw("end") || tok.is_kw("until") {
            out.push('\n');
            line_start = true;
        }

        prev = Some(tok);
    }

    let mut text = out.trim_end().to_string();
    text.push('\n');
    text
}

fn is_statement_start(t: &Token) -> bool {
    t.is_kw("local")
        || t.is_kw("if")
        || t.is_kw("for")
        || t.is_kw("while")
        || t.is_kw("return")
        || t.is_kw("break")
        || t.is_kw("do")
        || t.is_kw("repeat")
        || t.is_kw("function")
}

fn is_continuation(prev: Option<&Token>) -> bool {
    match prev {
        None => true,
        Some(p) => {
            matches!(&p.kind, Tok::Op(o) if matches!(o.as_str(), "=" | "," | "(" | "{" | "[" | ".." | "+" | "-" | "*" | "/" | "%" | "^" | "and" | "or" | "not" | "==" | "~=" | "<" | ">" | "<=" | ">=" | ";" | ":" | "::"))
                || p.is_kw("then")
                || p.is_kw("do")
                || p.is_kw("else")
                || p.is_kw("repeat")
                || p.is_kw("in")
                || p.is_kw("local")
        }
    }
}

fn ends_value(t: &Token) -> bool {
    match &t.kind {
        Tok::Name(_) | Tok::Num(_) | Tok::Str(_) => true,
        Tok::Kw(k) => matches!(*k, "true" | "false" | "nil" | "end"),
        Tok::Op(o) => matches!(o.as_str(), ")" | "]" | "}"),
        _ => false,
    }
}

fn is_binary_op(t: &Token) -> bool {
    match &t.kind {
        Tok::Op(o) => matches!(
            o.as_str(),
            "=" | "+" | "-" | "*" | "/" | "//" | "%" | "^" | ".." | "==" | "~=" | "<" | ">" | "<=" | ">=" | "&" | "|" | "~" | "<<" | ">>"
        ),
        Tok::Kw(k) => matches!(*k, "and" | "or"),
        _ => false,
    }
}

fn separator(prev: Option<&Token>, cur: &Token) -> Sep {
    let Some(p) = prev else { return Sep::None };

    if cur.is_op(")") || cur.is_op("]") || cur.is_op(",") || cur.is_op(";") || cur.is_op(".") || cur.is_op(":") {
        return Sep::None;
    }
    if p.is_op("(") || p.is_op("[") || p.is_op(".") || p.is_op(":") || p.is_op("#") || p.is_op("...") {
        return Sep::None;
    }
    if cur.is_op("(") || cur.is_op("[") {
        return if ends_value(p) { Sep::None } else { Sep::Space };
    }
    if cur.is_op("..") || p.is_op("..") {
        return Sep::Space;
    }
    if is_binary_op(cur) || is_binary_op(p) {
        return Sep::Space;
    }
    if cur.is_op("{") {
        return if p.is_op("(") || p.is_op("=") || p.is_op(",") || p.is_kw("return") { Sep::Space } else { Sep::Space };
    }
    if p.is_op(",") {
        return Sep::Space;
    }
    if cur.is_op("}") {
        return Sep::None;
    }
    Sep::Space
}
