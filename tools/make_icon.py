"""Build the LunarLua app icon from the supplied moon/lock logo.

The brand mark is flat blue on white, so the icon is a white rounded tile with a
soft blue bloom and a hairline rim - the logo itself is never altered.
"""
from PIL import Image, ImageDraw, ImageFilter
import sys

SRC = sys.argv[1] if len(sys.argv) > 1 else "web/assets/logo-raw.png"
OUT = sys.argv[2] if len(sys.argv) > 2 else "web/assets/icon.png"
S = 1024

src = Image.open(SRC).convert("RGBA")
white = Image.new("RGBA", src.size, (255, 255, 255, 255))
white.alpha_composite(src)
logo = white.convert("RGB").resize((S, S), Image.LANCZOS)

tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(tile)
d.rounded_rectangle([0, 0, S - 1, S - 1], radius=int(S * 0.22), fill=(255, 255, 255, 255))

# soft blue bloom so the white tile does not look dead
bloom = Image.new("RGBA", (S, S), (0, 0, 0, 0))
bd = ImageDraw.Draw(bloom)
bd.ellipse([S * 0.10, S * 0.10, S * 0.90, S * 0.90], fill=(30, 120, 255, 22))
bloom = bloom.filter(ImageFilter.GaussianBlur(S * 0.09))
tile = Image.alpha_composite(tile, bloom)

# the mark, slightly inset
size = int(S * 0.80)
mark = logo.resize((size, size), Image.LANCZOS).convert("RGBA")
off = (S - size) // 2
tile.alpha_composite(mark, (off, off))

# hairline rim
rim = Image.new("RGBA", (S, S), (0, 0, 0, 0))
rd = ImageDraw.Draw(rim)
rd.rounded_rectangle([3, 3, S - 4, S - 4], radius=int(S * 0.22), outline=(12, 60, 140, 40), width=3)
tile = Image.alpha_composite(tile, rim)

# keep the rounded silhouette crisp
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=int(S * 0.22), fill=255)
tile.putalpha(mask)

tile.save(OUT)
print("wrote", OUT)
