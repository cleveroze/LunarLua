//! Sandboxed execution of obfuscated chunks.
//!
//! Obfuscators that compile to a custom VM keep their constant pool encrypted
//! until the program actually runs. We therefore execute the chunk inside a
//! stripped Lua 5.1 state whose Roblox API is replaced by inert doubles, and
//! watch every string the program materialises.

use mlua::{Lua, MultiValue, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub struct Trace {
    pub strings: Vec<String>,
    pub prints: Vec<String>,
    pub error: Option<String>,
    pub instructions: u64,
}

const BOOTSTRAP: &str = r#"
local cap = __capture

-- deterministic-ish randomness so two runs look the same
math.randomseed(1337)

local function mock(name)
  local m = {}
  local mt = {}
  mt.__index = function(_, k) return mock(k) end
  mt.__newindex = function() end
  mt.__call = function() return mock(name .. "()") end
  mt.__tostring = function() return name end
  mt.__concat = function(a, b) return tostring(a) .. tostring(b) end
  mt.__add = function() return 0 end
  mt.__sub = function() return 0 end
  mt.__mul = function() return 0 end
  mt.__div = function() return 0 end
  mt.__mod = function() return 0 end
  mt.__pow = function() return 0 end
  mt.__unm = function() return 0 end
  mt.__eq = function() return false end
  mt.__lt = function() return false end
  mt.__le = function() return false end
  setmetatable(m, mt)
  return m
end

game = mock("game")
workspace = mock("workspace")
script = mock("script")
Enum = mock("Enum")
Instance = { new = function() return mock("Instance") end }
Vector3 = function(...) return mock("Vector3") end
Vector2 = function(...) return mock("Vector2") end
CFrame = function(...) return mock("CFrame") end
Color3 = function(...) return mock("Color3") end
UDim = function(...) return mock("UDim") end
UDim2 = function(...) return mock("UDim2") end
Ray = function(...) return mock("Ray") end
typeof = function(v) return type(v) end
tick = function() return 0 end
time = function() return 0 end
wait = function() return 0 end
spawn = function(f) if type(f) == "function" then pcall(f) end end
delay = function() end
warn = function(...) end

task = { wait = function() return 0 end, spawn = spawn, delay = delay, defer = delay }

-- Lua 5.1 has no bit library; obfuscators use it heavily.
local function tobit(x)
  x = math.fmod(x, 4294967296)
  if x < 0 then x = x + 4294967296 end
  return math.floor(x)
end
bit32 = {
  band = function(a, b)
    a, b = tobit(a), tobit(b)
    local r, bit = 0, 1
    for _ = 1, 32 do
      if a % 2 == 1 and b % 2 == 1 then r = r + bit end
      a, b, bit = math.floor(a / 2), math.floor(b / 2), bit * 2
    end
    return r
  end,
  bor = function(a, b)
    a, b = tobit(a), tobit(b)
    local r, bit = 0, 1
    for _ = 1, 32 do
      if a % 2 == 1 or b % 2 == 1 then r = r + bit end
      a, b, bit = math.floor(a / 2), math.floor(b / 2), bit * 2
    end
    return r
  end,
  bxor = function(a, b)
    a, b = tobit(a), tobit(b)
    local r, bit = 0, 1
    for _ = 1, 32 do
      if (a % 2) ~= (b % 2) then r = r + bit end
      a, b, bit = math.floor(a / 2), math.floor(b / 2), bit * 2
    end
    return r
  end,
  bnot = function(a) return 4294967295 - tobit(a) end,
  lshift = function(a, n) return tobit(tobit(a) * 2 ^ n) end,
  rshift = function(a, n) return math.floor(tobit(a) / 2 ^ n) end,
  arshift = function(a, n) return math.floor(tobit(a) / 2 ^ n) end,
  btest = function(a, b) return bit32.band(a, b) ~= 0 end,
}
bit = bit32

-- capture every string the payload builds
local function wrap(mod, names)
  for _, name in ipairs(names) do
    local real = mod[name]
    if type(real) == "function" then
      mod[name] = function(...)
        local r = real(...)
        if type(r) == "string" then cap(r) end
        return r
      end
    end
  end
end
wrap(string, { "char", "sub", "rep", "gsub", "format", "reverse", "lower", "upper" })
wrap(table, { "concat" })

local real_byte = string.byte
string.byte = function(s, ...) return real_byte(s, ...) end

-- loadstring is a decoding boundary in many packers
local real_loadstring = loadstring or load
loadstring = function(src, chunkname)
  if type(src) == "string" then cap(src) end
  return function() return nil end
end
load = loadstring

getfenv = function() return _G end
setfenv = function(f) return f end
newproxy = function() return mock("proxy") end
debug = {
  getinfo = function() return { short_src = "payload", currentline = 0, linedefined = 0, name = "?" } end,
  traceback = function() return "" end,
  sethook = function() end,
  getupvalue = function() return nil end,
  setupvalue = function() return true end,
}

os = { time = function() return 0 end, clock = function() return 0 end, date = function() return "" end }
io = nil
package = nil
require = nil
dofile = nil
loadfile = nil
collectgarbage = function() end
"#;

pub fn trace(source: &str, budget_ms: u64, capture_limit: usize) -> Trace {
    let mut out = Trace::default();
    let collected: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let prints: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    let lua = Lua::new();
    let _ = lua.set_memory_limit(128 * 1024 * 1024);

    let sink = collected.clone();
    if let Ok(f) = lua.create_function(move |_, s: mlua::String| {
        let bytes = s.as_bytes();
        sink.borrow_mut().push(String::from_utf8_lossy(&bytes).to_string());
        Ok(())
    }) {
        let _ = lua.globals().set("__capture", f);
    }

    let print_sink = prints.clone();
    let print_cap = collected.clone();
    if let Ok(f) = lua.create_function(move |_, args: MultiValue| {
        let mut line = String::new();
        for (i, v) in args.iter().enumerate() {
            if i > 0 {
                line.push_str(", ");
            }
            line.push_str(&render(v));
        }
        print_cap.borrow_mut().push(line.clone());
        print_sink.borrow_mut().push(line);
        Ok(())
    }) {
        let _ = lua.globals().set("print", f);
    }

    // instruction budget
    let start = Instant::now();
    let budget = Duration::from_millis(budget_ms.max(200));
    let ticks = Rc::new(std::cell::Cell::new(0u64));
    let tick_sink = ticks.clone();
    let hooked = lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(50_000),
        move |lua, _debug| {
            tick_sink.set(tick_sink.get() + 1);
            if start.elapsed() > budget {
                lua.remove_hook();
                return Err(mlua::Error::runtime("execution budget exceeded"));
            }
            Ok(mlua::VmState::Continue)
        },
    );
    let _ = hooked;

    if let Err(e) = lua.load(BOOTSTRAP).set_name("lunar-bootstrap").exec() {
        out.error = Some(format!("bootstrap: {e}"));
        return out;
    }

    let run = lua.load(source).set_name("payload").exec();
    if let Err(e) = run {
        out.error = Some(e.to_string());
    }

    let mut seen = std::collections::HashSet::new();
    for s in collected.borrow().iter() {
        if s.len() < 2 || s.len() > 4096 {
            continue;
        }
        let printable = s.chars().filter(|c| !c.is_control()).count() as f32 / s.chars().count() as f32;
        if printable < 0.92 {
            continue;
        }
        if s.chars().all(|c| c == ' ' || c == '\t') {
            continue;
        }
        if seen.insert(s.clone()) {
            out.strings.push(s.clone());
            if out.strings.len() >= capture_limit {
                break;
            }
        }
    }
    // drop noise: binary garbage, VM error strings and partial decode fragments
    let snapshot = out.strings.clone();
    out.strings.retain(|s| {
        !s.contains('\u{fffd}')
            && !s.starts_with("[string \"")
            && !s.contains("attempt to ")
            && !snapshot
                .iter()
                .any(|t| t != s && t.len() > s.len() && t.contains(s.as_str()) && s.len() <= 24)
    });

    out.instructions = ticks.get() * 50_000;
    out.prints = prints.borrow().clone();
    out
}

fn render(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.to_string_lossy().to_string(),
        _ => format!("<{}>", v.type_name()),
    }
}
