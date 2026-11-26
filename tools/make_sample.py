"""Builds the obfuscated demo script shipped with the app.

The output mimics a classic string-table pack: every literal is escaped into a
table, a one-line decoder hands them back, and a few dead locals are added.
"""
import io

ORIGINAL = '''-- LunarLua demo: a tiny Roblox client script
local Players = game:GetService("Players")
local player = Players.LocalPlayer

local function notify(message)
    print("[LunarLua]", message)
end

notify("script loaded")
notify("waiting for character")

local ready = true
if ready then
    print("ready")
end
'''

STRINGS = [
    "Players",
    "[LunarLua]",
    "script loaded",
    "waiting for character",
    "ready",
]


def escape(s: str) -> str:
    # every byte as a decimal escape, the way wearedevs packs literals
    return '"' + "".join("\\%d" % ch for ch in s.encode("utf-8")) + '"'


def main() -> None:
    table = "local t={" + ",".join(escape(s) for s in STRINGS) + "}\n"
    decoder = "local function d(i)return t[i]end\n"

    body = io.StringIO()
    body.write('local Players=game:GetService(d(1))\n')
    body.write('local player=Players.LocalPlayer\n')
    body.write('local z1=741236 local z2=91827 local z3=0\n')
    body.write('local function notify(message)\n')
    body.write(' print(d(2),message)\n')
    body.write('end\n')
    body.write('notify(d(3))\n')
    body.write('notify(d(4))\n')
    body.write('local ready=true\n')
    body.write('if ready then print(d(5)) end\n')
    body.write('if z1>999999 then z2=z2*2 end\n')

    sample = "--[[ v1.0.0 https://wearedevs.net/obfuscator ]]\n" + table + decoder + body.getvalue()
    with open("web/assets/sample.lua", "w", encoding="utf-8", newline="\n") as fh:
        fh.write(sample)
    print("wrote web/assets/sample.lua", len(sample), "bytes")


if __name__ == "__main__":
    main()
