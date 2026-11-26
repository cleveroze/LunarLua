//! LunarLua engine: detection, orchestration and reporting.

pub mod format;
pub mod lexer;
pub mod obfuscator;
pub mod sandbox;
pub mod static_deob;

use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Plain,
    Minified,
    StringTable,
    Loadstring,
    VmPack,
}

impl Family {
    pub fn label(&self) -> &'static str {
        match self {
            Family::Plain => "Plain Lua",
            Family::Minified => "Minified Lua",
            Family::StringTable => "String table",
            Family::Loadstring => "loadstring wrapper",
            Family::VmPack => "WeAreDevs VM pack",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Options {
    pub decode: bool,
    pub trace: bool,
    pub junk: bool,
    pub format: bool,
    pub rename: bool,
    pub encrypt: bool,
    pub noise: bool,
    pub minify: bool,
    pub seed: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Stats {
    pub strings: usize,
    pub junk: usize,
    pub lines: usize,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub output: String,
    pub detected: String,
    pub ms: u64,
    pub stats: Stats,
    pub notes: Vec<String>,
}

pub fn detect(src: &str) -> Family {
    let lower = src.to_ascii_lowercase();
    let vm_markers = [
        "return(function(...)",
        "return (function(...)",
        "local z={",
        "getfenv and getfenv()",
        "unpack or table",
    ];
    let vm_hits = vm_markers.iter().filter(|m| lower.contains(*m)).count();
    let modular = src.matches('%').count();
    if (vm_hits >= 2 && modular > 12) || (lower.contains("wearedevs.net/obfuscator") && vm_hits >= 1) {
        return Family::VmPack;
    }
    if lower.contains("loadstring(") || lower.contains("loadstring (") {
        return Family::Loadstring;
    }
    let tokens = lexer::lex(src);
    let strings = tokens.iter().filter(|t| matches!(t.kind, lexer::Tok::Str(_))).count();
    let has_table = src.contains("={") || src.contains("= {");
    if strings >= 6 && has_table {
        return Family::StringTable;
    }
    let lines = src.lines().count().max(1);
    let avg = src.len() / lines;
    if avg > 220 {
        return Family::Minified;
    }
    Family::Plain
}

fn header(family: Family, note: &str) -> String {
    format!(
        "--[[\n  LunarLua 1.0 - deobfuscation report\n  detected : {}\n  {}\n]]\n\n",
        family.label(),
        note
    )
}

fn vm_report(family: Family, trace: &sandbox::Trace, elapsed_ms: u64, _src_len: usize) -> Report {
    let mut out = String::new();
    out.push_str(&format!(
        "--[[\n  LunarLua 1.0 - deobfuscation report\n  detected : {}\n  recovered: {} string constants, {} traced operations\n  status   : {}\n\n  This build was compiled to a custom Lua VM, so the original source cannot be\n  reconstructed statement by statement. The constant pool and the observed\n  behaviour are restored below.\n]]\n\n",
        family.label(),
        trace.strings.len(),
        trace.prints.len(),
        match &trace.error {
            None => "clean run".to_string(),
            Some(e) => format!("stopped early ({})", truncate(e, 120)),
        }
    ));

    if !trace.strings.is_empty() {
        out.push_str("-- recovered string constants\nlocal strings = {\n");
        for (i, s) in trace.strings.iter().enumerate() {
            out.push_str(&format!("  [{}] = {},\n", i + 1, lexer::to_literal(s)));
        }
        out.push_str("}\n\n");
    }

    if !trace.prints.is_empty() {
        out.push_str("-- observed behaviour\n");
        for p in trace.prints.iter().take(400) {
            out.push_str(&format!("-- print({})\n", p));
        }
        out.push('\n');
    }

    if trace.strings.is_empty() && trace.prints.is_empty() {
        out.push_str("-- nothing was recovered: the payload refused to run in the sandbox.\n");
        out.push_str("-- try the Static mode instead.\n");
    }

    let notes = vec![
        "VM pack: source is compiled — constants and behaviour only".to_string(),
    ];

    Report {
        output: out.clone(),
        detected: family.label().to_string(),
        ms: elapsed_ms,
        stats: Stats {
            strings: trace.strings.len(),
            junk: 0,
            lines: out.lines().count(),
            bytes: out.len(),
        },
        notes,
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}…")
    }
}

pub fn deobfuscate(code: &str, opts: &Options) -> Report {
    let started = Instant::now();
    let family = detect(code);

    // forced static mode
    if opts.mode == "static" {
        return static_report(code, family, opts, started);
    }

    let is_vm = family == Family::VmPack || opts.mode == "wearedevs";
    if is_vm {
        if opts.trace {
            let trace = sandbox::trace(code, 1500, 900);
            let mut rep = vm_report(Family::VmPack, &trace, started.elapsed().as_millis() as u64, code.len());
            if let Some(e) = &trace.error {
                rep.notes.push(format!("sandbox: {}", truncate(e, 90)));
            }
            if trace.strings.is_empty() && opts.decode {
                let (fallback, _) = static_clean(code, opts);
                if fallback.trim().len() > 40 {
                    rep.notes.push("sandbox recovered nothing - showing static clean-up".into());
                    return static_report_from(code, Family::VmPack, opts, started, fallback);
                }
            }
            return rep;
        }
        let (cleaned, _) = static_clean(code, opts);
        return static_report_from(code, Family::VmPack, opts, started, cleaned);
    }

    static_report(code, family, opts, started)
}

fn static_report(code: &str, family: Family, opts: &Options, started: Instant) -> Report {
    let (out, stats) = static_clean(code, opts);
    static_report_from_with(code, family, opts, started, out, stats)
}

fn static_report_from(code: &str, family: Family, opts: &Options, started: Instant, out: String) -> Report {
    static_report_from_with(code, family, opts, started, out, Default::default())
}

fn static_report_from_with(
    code: &str,
    family: Family,
    opts: &Options,
    started: Instant,
    body: String,
    stats: Stats,
) -> Report {
    let mut notes: Vec<String> = Vec::new();
    let note = match family {
        Family::VmPack => "VM pack - best-effort static clean-up",
        Family::StringTable => "string table resolved",
        Family::Loadstring => "loadstring layer kept, payload decoded where possible",
        _ => "literals cleaned and re-indented",
    };
    let head = header(family, note);
    let output = format!("{head}{body}");

    if family == Family::VmPack {
        notes.push("VM pack: enable Sandbox trace for constants and behaviour".into());
    }
    if stats.strings == 0 && family == Family::StringTable {
        notes.push("no string table matched - structure may be non-standard".into());
    }

    let mut clean = output.trim_end().to_string();
    clean.push('\n');
    let bytes = clean.len();
    let lines = clean.lines().count();
    let _ = code.len();
    let _ = opts;

    Report {
        output: clean,
        detected: family.label().to_string(),
        ms: started.elapsed().as_millis() as u64,
        stats: Stats { lines, bytes, ..stats },
        notes,
    }
}

/// Runs every static pass; returns cleaned source and pass statistics.
pub fn static_clean(code: &str, opts: &Options) -> (String, Stats) {
    let mut stats = Stats::default();
    let mut text = code.to_string();

    if opts.decode {
        let (t, n) = static_deob::canonicalize_strings(&text);
        text = t;
        stats.strings += n;
        let (t, n) = static_deob::fold_literals(&text);
        text = t;
        stats.strings += n;
        let (t, n) = static_deob::resolve_string_tables(&text);
        text = t;
        stats.strings += n;
    }
    if opts.junk {
        let (t, n) = static_deob::strip_junk(&text);
        text = t;
        stats.junk = n;
    }
    if opts.format {
        text = format::beautify(&text);
    }
    (text, stats)
}

pub fn obfuscate_source(code: &str, opts: &Options) -> Report {
    let started = Instant::now();
    let o = obfuscator::ObfOptions {
        rename: opts.rename,
        encrypt: opts.encrypt,
        noise: opts.noise,
        minify: opts.minify,
        seed: if opts.seed.is_empty() { "lunar".into() } else { opts.seed.clone() },
    };
    let (out, renamed, strings) = obfuscator::obfuscate(code, &o);
    let mut text = out.trim_end().to_string();
    text.push('\n');
    let mut notes = Vec::new();
    if renamed > 0 {
        notes.push(format!("{renamed} identifiers renamed"));
    }
    if strings > 0 {
        notes.push(format!("{strings} strings encrypted"));
    }
    let bytes = text.len();
    let lines = text.lines().count();
    Report {
        output: text,
        detected: "Obfuscated".into(),
        ms: started.elapsed().as_millis() as u64,
        stats: Stats { strings, junk: 0, lines, bytes },
        notes,
    }
}
