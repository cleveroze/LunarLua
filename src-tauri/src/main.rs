#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod lua;

use lua::{Options, Report};
use serde::Serialize;
use std::fs;
use tauri::{WebviewUrl, WebviewWindowBuilder};

#[derive(Serialize)]
struct PickedFile {
    path: String,
    contents: String,
}

fn read_to_string(path: &std::path::Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[tauri::command]
fn open_file() -> Result<Option<PickedFile>, String> {
    let picked = rfd::FileDialog::new()
        .add_filter("Lua source", &["lua", "luau", "txt"])
        .set_title("Open Lua file")
        .pick_file();
    match picked {
        Some(p) => {
            let contents = read_to_string(&p)?;
            Ok(Some(PickedFile { path: p.display().to_string(), contents }))
        }
        None => Ok(None),
    }
}

#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    read_to_string(std::path::Path::new(&path))
}

#[tauri::command]
fn save_file(contents: String, suggested: String) -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new()
        .set_file_name(&suggested)
        .add_filter("Lua source", &["lua", "luau", "txt"])
        .set_title("Save Lua file")
        .save_file();
    match picked {
        Some(p) => {
            fs::write(&p, contents.as_bytes()).map_err(|e| e.to_string())?;
            Ok(Some(p.display().to_string()))
        }
        None => Ok(None),
    }
}

#[tauri::command]
fn read_clipboard() -> Result<String, String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.get_text().map_err(|e| e.to_string())
}

#[tauri::command]
fn write_clipboard(text: String) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
fn deobfuscate(code: String, options: Options) -> Result<Report, String> {
    Ok(lua::deobfuscate(&code, &options))
}

#[tauri::command]
fn obfuscate(code: String, options: Options) -> Result<Report, String> {
    Ok(lua::obfuscate_source(&code, &options))
}

/// WebView2 needs a writable profile folder. Prefer a portable one next to the
/// executable (works for per-user installs and portable use), fall back to the
/// default under %LOCALAPPDATA%.
fn data_dir() -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("LUNARLUA_DATA_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let _ = fs::create_dir_all(&dir);
        return Some(dir);
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.join(".lunarlua-webview");
    if fs::create_dir_all(&dir).is_ok() {
        Some(dir)
    } else {
        None
    }
}

fn build_window(app: &mut tauri::App) -> tauri::Result<()> {
    let portable = data_dir();

    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("LunarLua")
        .inner_size(1180.0, 764.0)
        .min_inner_size(980.0, 660.0)
        .center()
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .visible(true);

    if let Some(dir) = portable {
        let _ = std::fs::create_dir_all(&dir);
        builder = builder.data_directory(dir);
    }

    builder.build()?;
    Ok(())
}

fn cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--deob" {
        let src = fs::read_to_string(&args[2]).ok()?;
        let mut opts = Options {
            decode: true,
            trace: true,
            junk: true,
            format: true,
            mode: "auto".into(),
            ..Default::default()
        };
        if args.len() >= 5 && args[4] == "--static" {
            opts.trace = false;
            opts.mode = "static".into();
        }
        let rep = lua::deobfuscate(&src, &opts);
        let dest = args.get(3).cloned().unwrap_or_else(|| "out.lua".into());
        let _ = fs::write(&dest, rep.output.as_bytes());
        println!(
            "detected={} strings={} junk={} ms={} -> {}",
            rep.detected, rep.stats.strings, rep.stats.junk, rep.ms, dest
        );
        for n in &rep.notes {
            println!("note: {n}");
        }
        return Some(0);
    }
    if args.len() >= 3 && args[1] == "--run" {
        let src = fs::read_to_string(&args[2]).ok()?;
        let t = lua::sandbox::trace(&src, 3000, 400);
        println!("--- prints ---");
        for p in &t.prints {
            println!("{p}");
        }
        if let Some(e) = &t.error {
            println!("error: {e}");
        }
        println!("--- strings: {} ---", t.strings.len());
        return Some(0);
    }
    if args.len() >= 3 && args[1] == "--obf" {
        let src = fs::read_to_string(&args[2]).ok()?;
        let opts = Options {
            rename: true,
            encrypt: true,
            noise: true,
            minify: false,
            seed: "lunar".into(),
            ..Default::default()
        };
        let rep = lua::obfuscate_source(&src, &opts);
        let dest = args.get(3).cloned().unwrap_or_else(|| "out.lua".into());
        let _ = fs::write(&dest, rep.output.as_bytes());
        println!("obfuscated -> {} ({} bytes)", dest, rep.output.len());
        return Some(0);
    }
    None
}

fn main() {
    if let Some(code) = cli() {
        std::process::exit(code);
    }

    let built = tauri::Builder::default()
        .setup(|app| {
            build_window(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_file,
            read_file,
            save_file,
            read_clipboard,
            write_clipboard,
            deobfuscate,
            obfuscate
        ])
        .build(tauri::generate_context!());

    match built {
        Ok(app) => app.run(|_, _| {}),
        Err(err) => {
            let _ = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("LunarLua could not start")
                .set_description(format!(
                    "The interface could not be created.\n\n{}\n\n\
                     If this mentions WebView2, install the Microsoft Edge WebView2 runtime, \
                     or set LUNARLUA_DATA_DIR to a writable folder and try again.",
                    err
                ))
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
        }
    }
}
