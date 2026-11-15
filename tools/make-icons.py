"""Builds Dozy's icons from the flat peach sheep drawings: assets/dozy-source.png (jumping, keep awake on)
and assets/dozy-idle-source.png (standing, keep awake off).

The source uses three colors that all sit on one line from white to its orange outline, so each pixel is
re-colored by how far along that line it is, against a top-to-bottom orange gradient. Antialiased edges
land between stops and stay smooth.

Writes assets/dozy.png and assets/dozy-idle.png (the logos), assets/Dozy.icns (macOS app icon) and the raw
RGBA tray icons: assets/tray-mac-{awake,idle}.rgba (wide, because macOS sizes menu bar icons by height; both
the same width so the menu bar doesn't shift when they swap) and assets/tray-win-{awake,idle}.rgba (square,
as Windows requires). Dozy greys them out itself while warming is off.
Run from the repo root: python3 tools/make-icons.py
"""
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ASSETS = Path("assets")
SOURCE_OUTLINE_G = 182  # green channel of the source's outline; white is 255

# (position along white→outline, color at the top of the sheep, color at the bottom)
STOPS = [
    (0.00, (255, 255, 255), (255, 255, 255)),
    (0.12, (255, 246, 232), (255, 236, 216)),  # face and legs
    (0.41, (255, 209, 150), (255, 170, 110)),  # wool
    (1.00, (255, 150, 50), (240, 92, 30)),     # outline
]
TRAY_MAC = (56, 36)  # 18 pt tall at 2x, wide enough for the standing sheep
TRAY_WIN = (32, 32)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def recolor(src: Image.Image) -> Image.Image:
    src = src.convert("RGBA")
    w, h = src.size
    out = Image.new("RGBA", src.size)
    px, dst = src.load(), out.load()
    for y in range(h):
        v = y / (h - 1)
        ramp = [(s, lerp(top, bottom, v)) for s, top, bottom in STOPS]
        for x in range(w):
            r, g, b, a = px[x, y]
            if a == 0:
                continue
            s = min(max((255 - g) / (255 - SOURCE_OUTLINE_G), 0.0), 1.0)
            for (s0, c0), (s1, c1) in zip(ramp, ramp[1:]):
                if s <= s1:
                    dst[x, y] = (*lerp(c0, c1, (s - s0) / (s1 - s0)), a)
                    break
    return out


def fit(img: Image.Image, size, margin: float) -> Image.Image:
    """Scales img to fit a (w, h) box (or a square) with `margin` (fraction) on each side, centered."""
    w, h = size if isinstance(size, tuple) else (size, size)
    scaled = img.copy()
    scaled.thumbnail((round(w * (1 - 2 * margin)), round(h * (1 - 2 * margin))), Image.LANCZOS)
    canvas = Image.new("RGBA", (w, h))
    canvas.alpha_composite(scaled, ((w - scaled.width) // 2, (h - scaled.height) // 2))
    return canvas


def app_icon(logo: Image.Image, size: int = 1024) -> Image.Image:
    """The sheep on a rounded square, sized to Apple's icon grid (824 px body in a 1024 canvas)."""
    body, inset = round(size * 824 / 1024), round(size * 100 / 1024)
    bg = Image.new("RGBA", (size, size))
    grad = Image.new("RGBA", (body, body))
    d = ImageDraw.Draw(grad)
    for y in range(body):
        d.line([(0, y), (body, y)], fill=(*lerp((255, 249, 240), (255, 228, 200), y / body), 255))
    mask = Image.new("L", (body, body))
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, body - 1, body - 1], radius=round(body * 0.225), fill=255)
    shadow = Image.new("RGBA", (size, size))
    ImageDraw.Draw(shadow).rounded_rectangle(
        [inset, inset + size // 80, inset + body, inset + body + size // 80], radius=round(body * 0.225), fill=(120, 60, 20, 70)
    )
    bg.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(size / 100)))
    bg.paste(grad, (inset, inset), mask)
    bg.alpha_composite(fit(logo, body, 0.12), (inset, inset))
    return bg


def tray(logo: Image.Image, size) -> bytes:
    return fit(logo, size, 0.0).tobytes()


def main():
    logo = recolor(Image.open(ASSETS / "dozy-source.png"))
    logo.save(ASSETS / "dozy.png", optimize=True)
    idle = recolor(Image.open(ASSETS / "dozy-idle-source.png"))
    idle.save(ASSETS / "dozy-idle.png", optimize=True)

    icon = app_icon(logo)
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "Dozy.iconset"
        iconset.mkdir()
        for pt in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                name = f"icon_{pt}x{pt}{'@2x' if scale == 2 else ''}.png"
                icon.resize((pt * scale,) * 2, Image.LANCZOS).save(iconset / name)
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(ASSETS / "Dozy.icns")], check=True)
    icon.resize((256, 256), Image.LANCZOS).save(ASSETS / "Dozy-256.png")

    for platform, size in (("mac", TRAY_MAC), ("win", TRAY_WIN)):
        for state, sheep in (("awake", logo), ("idle", idle)):
            (ASSETS / f"tray-{platform}-{state}.rgba").write_bytes(tray(sheep, size))


if __name__ == "__main__":
    main()
