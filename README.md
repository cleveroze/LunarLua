<div align="center">
  <img src="web/assets/icon.png" width="96" alt="LunarLua" />
  <h1>LunarLua</h1>
  <p><b>Lua / Luau obfuscator &amp; deobfuscator</b> — a compact, glassy Windows desktop app.</p>
</div>

---

LunarLua is a small Tauri app with two workspaces:

| Workspace | What it does |
| --- | --- |
| **Deobfuscate** | Unpacks string-table and VM-packed Lua, decodes escaped literals, folds `string.char`, strips dead weight and re-indents the result. |
| **Obfuscate** | Renames locals, encrypts strings behind a generated decoder, adds optional noise, minifies. |

Everything runs locally in an embedded Lua 5.1 sandbox. No uploads, no accounts, no telemetry.

## Highlights

- **Two-tab glass UI** — black + neon blue, light theme included, smooth spring transitions.
- **Static passes** — escape decoding, literal folding, string-table and decoder resolution, junk removal, formatter.
- **Sandbox trace** — VM-packed scripts are executed inside a stripped Lua state with mocked Roblox APIs; every string the payload materialises is captured.
- **Real obfuscator** — deterministic builds from a seed, minify toggle, generated XOR-shift string decoder.
- **File workflow** — open, paste, drag & drop, save as `.lua` / `.luau` / `.txt`.

## What can actually be recovered

Obfuscation is not one thing:

| Input | Recoverable |
| --- | --- |
| Escaped strings, `string.char`, concatenation | **Fully** — rewritten to plain literals |
| String-table obfuscation (`local t = {...}`, `t[i]`) | **Fully** — table resolved, decoder removed |
| `loadstring` wrappers | **Payload** is captured and shown |
| VM packs (WeAreDevs / IronBrew-style) | Constant pool + observed behaviour — **not** the original statements |

The last row matters: a VM pack compiles the script to a private bytecode. The original variable
names, comments and unexecuted branches are gone. LunarLua restores the strings and the runtime
behaviour instead of pretending otherwise.

## Build

Requirements: Node 18+, Rust 1.77+, MSVC build tools + WebView2 (Windows).

```bash
npm install
npm run build          # produces src-tauri/target/release/lunarlua.exe + NSIS installer
```

Development:

```bash
npm run dev
```

Headless engine use (handy for testing):

```bash
lunarlua.exe --deob input.lua output.lua          # auto mode (sandbox trace for VM packs)
lunarlua.exe --deob input.lua output.lua --static # static passes only
lunarlua.exe --obf  input.lua output.lua          # obfuscate
```

## Shortcuts

| Key | Action |
| --- | --- |
| `Ctrl+O` | Open file |
| `Ctrl+S` | Save output |
| `Ctrl+Enter` | Run the active workspace |
| `Esc` | Close settings |

## Layout

```
web/                front-end (vanilla, no bundler)
  index.html
  styles.css
  app.js
  assets/           logo + generated icon
src-tauri/
  src/main.rs       window, commands, CLI
  src/lua/lexer.rs  Lua tokenizer
  src/lua/format.rs pretty printer
  src/lua/static_deob.rs  static passes
  src/lua/obfuscator.rs   renaming + string encryption
  src/lua/sandbox.rs      Lua 5.1 sandbox + Roblox doubles
  src/lua/mod.rs          detection, reporting
tools/make_icon.py  builds the icon from the logo
```

## License

MIT — see [LICENSE](LICENSE).

## Disclaimer

Use it on code you have the right to modify. Obfuscating someone else's script, or defeating a
product's protection to redistribute its code, can breach that product's terms or the law. VM
deobfuscation is best-effort and intentionally does not claim byte-exact recovery.
