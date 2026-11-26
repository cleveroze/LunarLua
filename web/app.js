(function () {
"use strict";

/* LunarLua — front-end controller */

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));

const TAURI = window.__TAURI__ || null;
const isTauri = !!TAURI;
const invoke = (cmd, args) => (TAURI ? TAURI.core.invoke(cmd, args) : Promise.reject(new Error("Not running inside Tauri")));

// localStorage can throw on custom protocols - never let it kill the boot
const store = {
  get(key, fallback) {
    try {
      const v = localStorage.getItem(key);
      return v == null ? fallback : v;
    } catch {
      return fallback;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch {}
  },
};

const state = {
  tab: "deob",
  theme: store.get("ll.theme", "dark"),
  busy: false,
  last: { deob: "", obf: "" },
};

/* ------------------------------------------------------------------ theme */
function applyTheme(theme, announce) {
  state.theme = theme;
  document.documentElement.dataset.theme = theme;
  store.set("ll.theme", theme);
  const label = $("#themeLabel");
  const icon = $("#themeIcon use");
  if (label) label.textContent = theme === "dark" ? "Dark" : "Light";
  if (icon) icon.setAttribute("href", theme === "dark" ? "#i-moon" : "#i-sun");
  const setThemeSwitch = $("#setTheme");
  if (setThemeSwitch) setThemeSwitch.classList.toggle("is-on", theme === "light");
  if (announce) toast(`${theme === "dark" ? "Dark" : "Light"} theme`, "info");
}

/* ------------------------------------------------------------------ toasts */
function toast(text, kind = "info", ttl = 2600) {
  const host = $("#toasts");
  const el = document.createElement("div");
  el.className = `toast ${kind}`;
  const icon = kind === "ok" ? "#i-check" : kind === "err" ? "#i-alert" : "#i-bolt";
  el.innerHTML = `<svg class="i"><use href="${icon}"/></svg><span></span>`;
  el.querySelector("span").textContent = text;
  host.appendChild(el);
  setTimeout(() => {
    el.classList.add("out");
    setTimeout(() => el.remove(), 320);
  }, ttl);
}

/* ------------------------------------------------------------------ tabs */
function setTab(tab) {
  state.tab = tab;
  $$(".nav-item").forEach((b) => b.classList.toggle("is-active", b.dataset.tab === tab));
  $$(".view").forEach((v) => v.classList.toggle("is-active", v.dataset.view === tab));
  positionGlider();
}

function positionGlider() {
  const nav = $("#nav");
  const glider = $("#navGlider");
  const active = $(".nav-item.is-active", nav);
  if (!active || !glider) return;
  glider.style.transform = `translateY(${active.offsetTop}px)`;
  glider.style.height = `${active.offsetHeight}px`;
}

/* ------------------------------------------------------------------ editors */
function gutterFor(value) {
  const lines = Math.max(1, value.split("\n").length);
  let html = "";
  for (let i = 1; i <= lines; i++) html += `<div>${i}</div>`;
  return html;
}

function refreshGutter(textarea, gutter) {
  gutter.innerHTML = gutterFor(textarea.value);
}

function stats(value) {
  const lines = value ? value.split("\n").length : 0;
  const bytes = new Blob([value]).size;
  return { lines, bytes };
}

function human(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

function setInput(which, text) {
  const ta = which === "deob" ? $("#deobIn") : $("#obfIn");
  ta.value = text;
  const meta = which === "deob" ? $("#deobInMeta") : $("#obfInMeta");
  const g = which === "deob" ? $("#deobGutterIn") : $("#obfGutterIn");
  refreshGutter(ta, g);
  const s = stats(text);
  meta.textContent = text ? `${s.lines} lines · ${human(s.bytes)}` : "empty";
  resetOutput(which);
}

function resetOutput(which) {
  const pre = which === "deob" ? $("#deobOut") : $("#obfOut");
  const meta = which === "deob" ? $("#deobOutMeta") : $("#obfOutMeta");
  const g = which === "deob" ? $("#deobGutterOut") : $("#obfGutterOut");
  pre.innerHTML = `<span class="empty-state">${which === "deob" ? "Recovered source will appear here." : "Protected source will appear here."}</span>`;
  meta.textContent = "—";
  g.innerHTML = "";
  state.last[which] = "";
}

function renderOutput(which, text) {
  const pre = which === "deob" ? $("#deobOut") : $("#obfOut");
  const meta = which === "deob" ? $("#deobOutMeta") : $("#obfOutMeta");
  const g = which === "deob" ? $("#deobGutterOut") : $("#obfGutterOut");
  state.last[which] = text;
  pre.innerHTML = highlightLua(text);
  g.innerHTML = gutterFor(text);
  const s = stats(text);
  meta.textContent = `${s.lines} lines · ${human(s.bytes)}`;
}

/* ------------------------------------------------------------------ lua highlight */
const KEYWORDS = new Set([
  "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in",
  "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
  "continue", "export", "type", "typeof", "self",
]);

const TOKEN_RE = new RegExp(
  [
    "--\\[\\[[\\s\\S]*?\\]\\]",
    "--[^\\n]*",
    "\\[\\[[\\s\\S]*?\\]\\]",
    '"(?:\\\\.|[^"\\\\\\n])*"',
    "'(?:\\\\.|[^'\\\\\\n])*'",
    "\\b0[xX][0-9a-fA-F]+\\b",
    "\\b\\d+\\.?\\d*(?:[eE][+-]?\\d+)?\\b",
    "[A-Za-z_][A-Za-z0-9_]*",
    "\\.\\.\\.|::|[+\\-*/%^#=<>~&|.,;:(){}\\[\\]]",
    "\\s+",
  ].join("|"),
  "g",
);

const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

function highlightLua(src) {
  if (!src) return "";
  if (src.length > 400_000) return esc(src);
  const out = [];
  let m;
  let last = 0;
  TOKEN_RE.lastIndex = 0;
  while ((m = TOKEN_RE.exec(src))) {
    if (m.index > last) out.push(esc(src.slice(last, m.index)));
    const t = m[0];
    last = m.index + t.length;
    if (t.startsWith("--")) out.push(`<span class="tk-com">${esc(t)}</span>`);
    else if (t.startsWith('"') || t.startsWith("'") || t.startsWith("[[")) out.push(`<span class="tk-str">${esc(t)}</span>`);
    else if (/^\d/.test(t) || /^0[xX]/.test(t)) out.push(`<span class="tk-num">${esc(t)}</span>`);
    else if (/^[A-Za-z_]/.test(t)) {
      if (KEYWORDS.has(t)) out.push(`<span class="${t === "true" || t === "false" || t === "nil" ? "tk-bool" : "tk-kw"}">${t}</span>`);
      else if (src[last] === "(") out.push(`<span class="tk-fn">${t}</span>`);
      else out.push(t);
    } else out.push(`<span class="tk-op">${esc(t)}</span>`);
  }
  if (last < src.length) out.push(esc(src.slice(last)));
  return out.join("");
}

/* ------------------------------------------------------------------ engine calls */
function busy(which, on) {
  const pulse = which === "deob" ? $("#deobPulse") : $("#obfPulse");
  const progress = which === "deob" ? $("#deobProgress") : $("#obfProgress");
  const btn = which === "deob" ? $("#deobRun") : $("#obfRun");
  const chip = $("#engineChip");
  pulse.className = `status-pulse ${on ? "busy" : ""}`;
  progress.classList.toggle("on", on);
  btn.disabled = on;
  chip.classList.toggle("busy", on);
  $("#engineChipText").textContent = on ? "working" : "engine ready";
}

function setStatus(which, text, kind, extra) {
  const pulse = which === "deob" ? $("#deobPulse") : $("#obfPulse");
  const label = which === "deob" ? $("#deobStatus") : $("#obfStatus");
  const det = which === "deob" ? $("#deobDetected") : $("#obfDetected");
  pulse.className = `status-pulse ${kind || ""}`;
  label.textContent = text;
  if (extra !== undefined) det.textContent = extra;
}

async function run(which) {
  if (state.busy) return;
  const input = which === "deob" ? $("#deobIn").value : $("#obfIn").value;
  if (!input.trim()) {
    toast("Paste some Lua first", "info");
    return;
  }
  state.busy = true;
  busy(which, true);
  setStatus(which, "Working…", "busy", "analyzing");
  const started = performance.now();

  const options = {
    decode: isOn("setDecode"),
    trace: isOn("setTrace"),
    junk: isOn("setJunk"),
    format: isOn("setFormat"),
    rename: isOn("setRename"),
    encrypt: isOn("setEncrypt"),
    noise: isOn("setNoise"),
    minify: which === "obf" ? isOn("obfMinify") : false,
    seed: ($("#setSeed") || {}).value || "lunar",
    mode: ($(".seg-item.is-active") || {}).dataset?.mode || "auto",
  };

  try {
    const cmd = which === "deob" ? "deobfuscate" : "obfuscate";
    const res = await invoke(cmd, { code: input, options });
    const ms = Math.round(res.ms ?? performance.now() - started);
    renderOutput(which, res.output || "");
    animateOut(which);
    setStatus(which, `Done in ${ms} ms`, "ok", res.detected || "Lua / Luau");
    const bits = [];
    if (res.stats?.strings) bits.push(`${res.stats.strings} strings`);
    if (res.stats?.junk) bits.push(`${res.stats.junk} junk removed`);
    toast(bits.length ? `Done — ${bits.join(" · ")}` : `Done in ${ms} ms`, "ok");
    (res.notes || []).forEach((n) => toast(n, "info", 4200));
  } catch (err) {
    const msg = typeof err === "string" ? err : err?.message || String(err);
    setStatus(which, "Failed", "err", msg.slice(0, 90));
    toast(msg.slice(0, 220), "err", 5200);
  } finally {
    state.busy = false;
    busy(which, false);
    const ms = Math.round(performance.now() - started);
    if ($("#meterValue")) {
      $("#meterValue").textContent = `${ms} ms`;
      $("#meterLabel").textContent = "last run";
      $("#meterFill").style.width = `${Math.min(100, Math.max(8, ms / 6))}%`;
    }
  }
}

function animateOut(which) {
  const pre = which === "deob" ? $("#deobOut") : $("#obfOut");
  pre.animate(
    [
      { opacity: 0, transform: "translateY(6px)" },
      { opacity: 1, transform: "none" },
    ],
    { duration: 380, easing: "cubic-bezier(.22,1,.36,1)" },
  );
}

/* ------------------------------------------------------------------ files */
async function openFile(which) {
  try {
    const res = await invoke("open_file");
    if (!res) return;
    const base = (res.path || "").split(/[\\/]/).pop();
    setInput(which, res.contents || "");
    toast(`Opened ${base}`, "ok");
  } catch (err) {
    toast(String(err?.message || err), "err");
  }
}

async function saveFile(which) {
  const text = state.last[which] || (which === "deob" ? $("#deobOut").textContent : $("#obfOut").textContent);
  if (!text || !text.trim()) {
    toast("Nothing to save yet", "info");
    return;
  }
  try {
    const name = which === "deob" ? "deobfuscated.lua" : "obfuscated.lua";
    const res = await invoke("save_file", { contents: text, suggested: name });
    if (res) toast(`Saved ${res.split(/[\\/]/).pop()}`, "ok");
  } catch (err) {
    toast(String(err?.message || err), "err");
  }
}

async function pasteInto(which) {
  try {
    const text = await invoke("read_clipboard");
    if (!text || !text.trim()) {
      toast("Clipboard is empty", "info");
      return;
    }
    setInput(which, text);
    toast("Pasted from clipboard", "ok");
  } catch (err) {
    toast("Clipboard unavailable", "err");
  }
}

async function copyText(text, label) {
  if (!text) return toast("Nothing to copy", "info");
  try {
    await navigator.clipboard.writeText(text);
    toast(`${label} copied`, "ok");
  } catch {
    try {
      await invoke("write_clipboard", { text });
      toast(`${label} copied`, "ok");
    } catch (e) {
      toast("Copy failed", "err");
    }
  }
}

/* ------------------------------------------------------------------ ui plumbing */
const isOn = (id) => {
  const el = document.getElementById(id);
  return !!el && el.classList.contains("is-on");
};

function wireSwitch(el, onChange) {
  el.addEventListener("click", () => {
    el.classList.toggle("is-on");
    onChange?.(el.classList.contains("is-on"));
  });
}

function openDrawer(open) {
  $("#drawer").classList.toggle("on", open);
  $("#scrim").classList.toggle("on", open);
  $("#drawer").setAttribute("aria-hidden", open ? "false" : "true");
}

function initWindow() {
  let win = null;
  try {
    win = TAURI.window.getCurrentWindow();
  } catch {
    return;
  }
  if (!win) return;

  const action = (fn) => async (event) => {
    event?.preventDefault?.();
    try {
      await fn();
    } catch (err) {
      toast("Window action blocked", "err");
    }
  };

  $("#btnMin").addEventListener("click", action(() => win.minimize()));
  $("#btnMax").addEventListener("click", action(() => win.toggleMaximize()));
  $("#btnClose").addEventListener("click", action(() => win.close()));

  const bar = $("#titlebar");
  bar.addEventListener("mousedown", (e) => {
    if (e.button !== 0 || e.target.closest(".win-btn")) return;
    win.startDragging().catch(() => {});
  });
  bar.addEventListener("dblclick", (e) => {
    if (e.target.closest(".win-btn")) return;
    win.toggleMaximize().catch(() => {});
  });
}

function finishBoot() {
  const boot = document.getElementById("boot");
  if (!boot || boot.classList.contains("done")) return;
  boot.classList.add("done");
  setTimeout(() => boot.remove(), 700);
}

async function loadSample() {
  try {
    const res = await fetch("assets/sample.lua");
    const text = await res.text();
    setInput("deob", text);
    setStatus("deob", "Sample loaded", "ok", "WeAreDevs-style");
    toast("Obfuscated sample loaded", "info");
  } catch {
    toast("Sample not found", "err");
  }
}

/* ------------------------------------------------------------------ boot */
document.addEventListener("DOMContentLoaded", () => {
  applyTheme(state.theme, false);
  initWindow();
  positionGlider();
  window.addEventListener("resize", positionGlider);

  $$(".nav-item").forEach((b) => b.addEventListener("click", () => setTab(b.dataset.tab)));
  $$(".seg-item").forEach((b) => b.addEventListener("click", () => {
    $$(".seg-item").forEach((x) => x.classList.toggle("is-active", x === b));
  }));

  // inputs
  for (const which of ["deob", "obf"]) {
    const ta = $(`#${which}In`);
    const g = $(`#${which}GutterIn`);
    refreshGutter(ta, g);
    ta.addEventListener("input", () => {
      refreshGutter(ta, g);
      const meta = $(`#${which}InMeta`);
      const s = stats(ta.value);
      meta.textContent = ta.value ? `${s.lines} lines · ${human(s.bytes)}` : "empty";
    });
    ta.addEventListener("scroll", () => {
      g.scrollTop = ta.scrollTop;
    });
    ta.addEventListener("keydown", (e) => {
      if (e.key === "Tab") {
        e.preventDefault();
        const start = ta.selectionStart;
        ta.setRangeText("  ", start, ta.selectionEnd, "end");
        refreshGutter(ta, g);
      }
    });
  }

  // toolbar
  $("#deobOpen").onclick = () => openFile("deob");
  $("#obfOpen").onclick = () => openFile("obf");
  $("#deobPaste").onclick = () => pasteInto("deob");
  $("#obfPaste").onclick = () => pasteInto("obf");
  $("#deobSample").onclick = loadSample;
  $("#deobClear").onclick = () => {
    setInput("deob", "");
    setStatus("deob", "Ready", "", "Lua / Luau");
  };
  $("#deobCopyIn").onclick = () => copyText($("#deobIn").value, "Input");
  $("#obfCopyIn").onclick = () => copyText($("#obfIn").value, "Input");
  $("#deobCopyOut").onclick = () => copyText(state.last.deob, "Output");
  $("#obfCopyOut").onclick = () => copyText(state.last.obf, "Output");
  $("#deobSaveOut").onclick = () => saveFile("deob");
  $("#obfSaveOut").onclick = () => saveFile("obf");
  $("#deobRun").onclick = () => run("deob");
  $("#obfRun").onclick = () => run("obf");

  // switches
  wireSwitch($("#obfMinify"));
  wireSwitch($("#obfStrings"));
  wireSwitch($("#setDecode"));
  wireSwitch($("#setTrace"));
  wireSwitch($("#setJunk"));
  wireSwitch($("#setFormat"));
  wireSwitch($("#setRename"));
  wireSwitch($("#setEncrypt"));
  wireSwitch($("#setNoise"));
  wireSwitch($("#setTheme"), (on) => applyTheme(on ? "light" : "dark", true));

  // theme
  $("#themeToggle").onclick = () => applyTheme(state.theme === "dark" ? "light" : "dark", true);

  // settings drawer
  $("#btnSettings").onclick = () => openDrawer(true);
  $("#drawerClose").onclick = () => openDrawer(false);
  $("#scrim").onclick = () => openDrawer(false);
  $("#obfOptions").onclick = () => openDrawer(true);

  // output scroll sync
  for (const which of ["deob", "obf"]) {
    const scroller = $(`#${which}Out`).parentElement;
    const g = $(`#${which}GutterOut`);
    scroller.addEventListener("scroll", () => {
      g.scrollTop = scroller.scrollTop;
    });
  }

  // shortcuts
  document.addEventListener("keydown", (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "o") {
      e.preventDefault();
      openFile(state.tab);
    } else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      saveFile(state.tab);
    } else if (mod && e.key === "Enter") {
      e.preventDefault();
      run(state.tab);
    } else if (e.key === "Escape") {
      openDrawer(false);
    }
  });

  // drag & drop from the OS
  if (isTauri) {
    try {
      const w = TAURI.window.getCurrentWindow();
      w.onDragDropEvent?.(async (event) => {
        if (event.payload?.type === "drop" && event.payload.paths?.length) {
          const path = event.payload.paths[0];
          try {
            const res = await invoke("read_file", { path });
            setInput(state.tab, res);
            toast(`Opened ${path.split(/[\\/]/).pop()}`, "ok");
          } catch (err) {
            toast("Could not read that file", "err");
          }
        }
      });
    } catch {}
  }

  // let the boot screen breathe for a moment, then hand over
  // wait until the first frame is actually painted, then hold briefly
  requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(finishBoot, 700)));
});

/* Fluent reveal highlight */
document.addEventListener("pointermove", (e) => {
  const el = e.target.closest?.(".tool, .nav-item, .primary, .pill-btn");
  if (!el) return;
  const r = el.getBoundingClientRect();
  el.style.setProperty("--mx", `${e.clientX - r.left}px`);
  el.style.setProperty("--my", `${e.clientY - r.top}px`);
});

window.highlightLua = highlightLua;
window.gutterFor = gutterFor;
})();
