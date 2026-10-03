"""Regenerates assets/icon.png and assets/icon.ico (the app icon).

    python3 scripts/make-icon.py   (needs Pillow)

A small neural graph in the theme's accent color on the dark theme background.
Drawn at 4x and downsampled for smooth edges.
"""

from pathlib import Path

from PIL import Image, ImageDraw

BG = (27, 28, 32, 255)
ACCENT = (146, 132, 255, 255)
EDGE = (146, 132, 255, 150)
SIZE = 256
SCALE = 4

# Node positions in a 0..1 square, then edges between them by index.
NODES = [(0.27, 0.30), (0.68, 0.24), (0.50, 0.52), (0.24, 0.72), (0.74, 0.70)]
EDGES = [(0, 1), (0, 2), (1, 2), (2, 3), (2, 4), (3, 4), (1, 4)]
RADII = [0.075, 0.065, 0.10, 0.065, 0.075]


def draw() -> Image.Image:
    n = SIZE * SCALE
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, n - 1, n - 1), radius=int(n * 0.22), fill=BG)
    pts = [(x * n, y * n) for x, y in NODES]
    for a, b in EDGES:
        d.line((pts[a], pts[b]), fill=EDGE, width=int(n * 0.028))
    for (x, y), r in zip(pts, RADII):
        r *= n
        d.ellipse((x - r, y - r, x + r, y + r), fill=ACCENT)
    return img.resize((SIZE, SIZE), Image.LANCZOS)


def main() -> None:
    out = Path(__file__).resolve().parent.parent / "assets"
    out.mkdir(exist_ok=True)
    img = draw()
    img.save(out / "icon.png", optimize=True)
    img.save(out / "icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])


if __name__ == "__main__":
    main()
