"""Branded NSIS artwork: 164x314 sidebar and 150x57 header (24-bit BMP)."""
from PIL import Image, ImageDraw, ImageFont
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "src-tauri", "installer")
os.makedirs(OUT, exist_ok=True)

DARK_TOP = (10, 11, 13)
DARK_BOT = (24, 27, 34)
BLUE = (10, 132, 255)
WHITE = (255, 255, 255)
GREY = (150, 158, 172)


def font(size, bold=True):
    candidates = [
        r"C:\Windows\Fonts\seguisb.ttf" if bold else r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arialbd.ttf" if bold else r"C:\Windows\Fonts\arial.ttf",
    ]
    for c in candidates:
        if os.path.exists(c):
            try:
                return ImageFont.truetype(c, size)
            except OSError:
                pass
    return ImageFont.load_default()


def vertical_gradient(w, h, top, bottom):
    img = Image.new("RGB", (w, h))
    px = img.load()
    for y in range(h):
        t = y / max(1, h - 1)
        c = tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        for x in range(w):
            px[x, y] = c
    return img


def mark(size):
    src = Image.open(os.path.join(ROOT, "web", "assets", "logo-raw.png")).convert("RGBA")
    px = src.load()
    w, h = src.size
    out = Image.new("RGBA", src.size, (0, 0, 0, 0))
    op = out.load()
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            m = min(r, g, b)
            op[x, y] = (r, g, b, int(a * (255 - m) / 255))
    return out.resize((size, size), Image.LANCZOS)


def glow(img, box, colour, blur):
    from PIL import ImageFilter
    layer = Image.new("RGB", img.size, (0, 0, 0))
    ImageDraw.Draw(layer).ellipse(box, fill=colour)
    layer = layer.filter(ImageFilter.GaussianBlur(blur))
    return Image.blend(img, Image.blend(img, layer, 0.55), 0.45)


def sidebar():
    w, h = 164, 314
    img = vertical_gradient(w, h, DARK_TOP, DARK_BOT).convert("RGBA")
    img = glow(img.convert("RGB"), (-40, -60, 160, 140), (16, 62, 140), 46).convert("RGBA")

    logo = mark(64)
    img.alpha_composite(logo, (50, 40))

    f_title = font(17, True)
    f_sub = font(9, False)
    f_ver = font(8, False)
    d = ImageDraw.Draw(img)
    tw = d.textlength("LunarLua", font=f_title)
    d.text(((w - tw) / 2, 120), "LunarLua", font=f_title, fill=WHITE)
    sw = d.textlength("Lua toolkit", font=f_sub)
    d.text(((w - sw) / 2, 143), "Lua toolkit", font=f_sub, fill=GREY)

    d.line([(18, 170), (146, 170)], fill=(38, 44, 56), width=1)
    d.line([(18, 170), (60, 170)], fill=BLUE, width=1)

    d.text((18, 182), "deobfuscate", font=f_sub, fill=(120, 128, 142))
    d.text((18, 196), "obfuscate", font=f_sub, fill=(120, 128, 142))
    d.text((18, 210), "sandbox trace", font=f_sub, fill=(120, 128, 142))

    d.text((18, 290), "v1.0.0", font=f_ver, fill=(92, 99, 112))
    return img


def header():
    w, h = 150, 57
    img = vertical_gradient(w, h, DARK_TOP, (18, 20, 26)).convert("RGBA")
    logo = mark(26)
    img.alpha_composite(logo, (8, 16))
    d = ImageDraw.Draw(img)
    d.text((40, 18), "LunarLua", font=font(13, True), fill=WHITE)
    d.text((40, 34), "installer", font=font(8, False), fill=GREY)
    return img


sidebar().convert("RGB").save(os.path.join(OUT, "sidebar.bmp"), "BMP")
header().convert("RGB").save(os.path.join(OUT, "header.bmp"), "BMP")
print("wrote", OUT)
