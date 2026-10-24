#!/usr/bin/env python3
"""Draw the app icon and the two tray icons (needs Pillow).

  app-icon.png  1024px source for `tauri icon` (dark tile, coral + mint gauge arcs)
  tray-mac.png  black template image, macOS tints it for light/dark menu bars
  tray-win.png  coloured gauge that reads on light and dark Windows taskbars
"""

from pathlib import Path
from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
OUT.mkdir(parents=True, exist_ok=True)
CORAL, MINT, TRACK = (255, 138, 91), (61, 220, 151), (52, 57, 68)


def gauge(size, ring, colors, track, bg=None, radius=0, needle=None, scale=4):
    """A gauge ring drawn at `scale`x and shrunk, for smooth edges."""
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if bg:
        d.rounded_rectangle([0, 0, s - 1, s - 1], radius=radius * scale, fill=bg)
    pad, width = ring[0] * scale, ring[1] * scale
    box = [pad, pad, s - pad, s - pad]
    if track:
        d.arc(box, 0, 360, fill=track, width=width)
    for start, end, color in colors:
        d.arc(box, start, end, fill=color, width=width)
    if needle:
        c = s / 2
        d.line([c, c, c + needle[0] * scale, c - needle[1] * scale], fill=needle[2], width=int(width * 0.8))
        r = width * 0.7
        d.ellipse([c - r, c - r, c + r, c + r], fill=needle[2])
    return img.resize((size, size), Image.LANCZOS)


# App icon: dark rounded tile, two arcs (Claude coral, ChatGPT mint), gap at the bottom.
gauge(1024, (190, 96), [(135, 300, CORAL), (312, 405, MINT)], TRACK,
      bg=(18, 20, 26, 255), radius=230, needle=(170, 120, (232, 234, 237, 255))).save(OUT / "app-icon.png")

# macOS template: pure black on transparent, open ring + needle (44px = 22pt @2x).
gauge(44, (5, 5), [(135, 405, (0, 0, 0, 255))], None, needle=(9, 6, (0, 0, 0, 255))).save(OUT / "tray-mac.png")

# Windows: the coloured gauge on a grey track, readable on light and dark taskbars.
gauge(32, (3, 4), [(135, 300, CORAL), (312, 405, MINT)], (120, 126, 138, 255),
      needle=(7, 5, (200, 204, 210, 255))).save(OUT / "tray-win.png")
print("icons written to", OUT)
