"""Generate the BiBoTracking icon sets from the BrandMark (ui.tsx): a purple
gradient tile (150deg, #8170f2 -> #6c5ce7, like `.welcome .brand-mark`) holding
the white circle + pulse-line glyph.

Writes:
  - apps/desktop/src-tauri/icons/  (app PNGs, Square*Logo, StoreLogo, .icns, .ico)
  - apps/desktop/src-tauri/icons/tray/  (44px state-tinted glyphs)
  - apps/extension/icons/  (16/32/48/128 full-bleed tiles)

Run:  python3 scripts/gen-icons.py   (then scripts/gen-env-icons.py for dev/stg)
Needs: Pillow, macOS `iconutil` for the .icns.
"""
import os
import subprocess
import tempfile
from PIL import Image, ImageDraw

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ICONS = os.path.join(ROOT, "apps/desktop/src-tauri/icons")
TRAY_DIR = os.path.join(ICONS, "tray")
EXT_DIR = os.path.join(ROOT, "apps/extension/icons")

GRAD_A = (0x81, 0x70, 0xF2, 255)  # #8170f2
GRAD_B = (0x6C, 0x5C, 0xE7, 255)  # #6c5ce7 (--brand)
WHITE = (255, 255, 255, 255)
GREEN = (47, 168, 90, 255)   # tray: tracking
AMBER = (224, 164, 0, 255)   # tray: idle
RED = (217, 69, 60, 255)     # tray: paused

# BrandMark SVG geometry (viewBox 0 0 24 24, stroke-width 2, round caps/joins)
PULSE = [(4.5, 12), (7.7, 12), (9.5, 7.6), (11.9, 16.4), (13.7, 12), (18.2, 12)]
RING_C, RING_R, STROKE = (12, 12), 9, 2


def draw_brand_glyph(d, cx, cy, glyph_px, color):
    """Draw the circle + pulse line, scaled so viewBox 24 -> glyph_px."""
    g = glyph_px / 24.0
    w = max(1, round(STROKE * g))
    # ring (PIL strokes inward from the bbox, so pad by w/2 to center it on r)
    rr = RING_R * g + w / 2.0
    ox, oy = cx + (RING_C[0] - 12) * g, cy + (RING_C[1] - 12) * g
    d.ellipse([ox - rr, oy - rr, ox + rr, oy + rr], outline=color, width=w)
    # pulse polyline with round joins + caps
    pts = [(cx + (x - 12) * g, cy + (y - 12) * g) for x, y in PULSE]
    d.line(pts, fill=color, width=w, joint="curve")
    for px, py in (pts[0], pts[-1]):
        d.ellipse([px - w / 2, py - w / 2, px + w / 2, py + w / 2], fill=color)


def gradient_tile(S, radius):
    """S x S rounded-rect tile filled with the 150deg brand gradient."""
    big = int(S * 1.6)
    grad = Image.linear_gradient("L").resize((big, big), Image.BILINEAR)
    grad = grad.rotate(-30, resample=Image.BICUBIC)  # ~CSS 150deg
    grad = grad.crop(((big - S) // 2, (big - S) // 2,
                      (big - S) // 2 + S, (big - S) // 2 + S))
    a = Image.new("RGBA", (S, S), GRAD_A)
    b = Image.new("RGBA", (S, S), GRAD_B)
    tile = Image.composite(b, a, grad)
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1],
                                           radius=radius, fill=255)
    tile.putalpha(mask)
    return tile


def app_master(size=1024, margin_frac=0.085, ss=2):
    """App icon: transparent margin, gradient tile, white glyph (like welcome)."""
    S = size * ss
    m = int(S * margin_frac)
    tile_span = S - 2 * m
    tile = gradient_tile(tile_span, radius=int(tile_span * 16 / 56))
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.alpha_composite(tile, (m, m))
    d = ImageDraw.Draw(img)
    draw_brand_glyph(d, S / 2, S / 2, tile_span * 34 / 56, WHITE)
    return img.resize((size, size), Image.LANCZOS)


def ext_master(size=1024, ss=2):
    """Extension icon: full-bleed tile (no margin), same glyph."""
    S = size * ss
    img = gradient_tile(S, radius=int(S * 16 / 56))
    d = ImageDraw.Draw(img)
    draw_brand_glyph(d, S / 2, S / 2, S * 34 / 56, WHITE)
    return img.resize((size, size), Image.LANCZOS)


def write_app_set(master):
    for name, px in {
        "32x32.png": 32, "64x64.png": 64, "128x128.png": 128,
        "128x128@2x.png": 256, "icon.png": 512, "StoreLogo.png": 50,
        "Square30x30Logo.png": 30, "Square44x44Logo.png": 44,
        "Square71x71Logo.png": 71, "Square89x89Logo.png": 89,
        "Square107x107Logo.png": 107, "Square142x142Logo.png": 142,
        "Square150x150Logo.png": 150, "Square284x284Logo.png": 284,
        "Square310x310Logo.png": 310,
    }.items():
        master.resize((px, px), Image.LANCZOS).save(os.path.join(ICONS, name))
    master.resize((256, 256), Image.LANCZOS).save(
        os.path.join(ICONS, "icon.ico"),
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    with tempfile.TemporaryDirectory() as tmp:
        iconset = os.path.join(tmp, "icon.iconset")
        os.makedirs(iconset)
        for s in (16, 32, 64, 128, 256, 512):
            master.resize((s, s), Image.LANCZOS).save(
                os.path.join(iconset, f"icon_{s}x{s}.png"))
            master.resize((s * 2, s * 2), Image.LANCZOS).save(
                os.path.join(iconset, f"icon_{s}x{s}@2x.png"))
        subprocess.run(["iconutil", "-c", "icns", iconset,
                        "-o", os.path.join(ICONS, "icon.icns")], check=True)
    print("wrote app icon set ->", ICONS)


def write_tray(size=44, ss=8):
    """Menu-bar glyphs: BrandMark line-art only, tinted per tracker state."""
    for name, color in (("tracking", GREEN), ("idle", AMBER), ("paused", RED)):
        S = size * ss
        img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        draw_brand_glyph(ImageDraw.Draw(img), S / 2, S / 2, S * 0.92, color)
        img.resize((size, size), Image.LANCZOS).save(
            os.path.join(TRAY_DIR, f"tray-{name}.png"))
    print("wrote tray set ->", TRAY_DIR)


def write_extension(master):
    for px in (16, 32, 48, 128):
        master.resize((px, px), Image.LANCZOS).save(
            os.path.join(EXT_DIR, f"{px}.png"))
    print("wrote extension set ->", EXT_DIR)


if __name__ == "__main__":
    write_app_set(app_master())
    write_tray()
    write_extension(ext_master())
