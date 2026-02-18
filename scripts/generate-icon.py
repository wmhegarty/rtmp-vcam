#!/usr/bin/env python3
"""Generate the RTMPVirtualCamera app icon.

Design: Camera lens (dark, concentric rings) with a red broadcast dot
on a rounded-rect gradient background. Clean, modern macOS style.
"""

import math
from PIL import Image, ImageDraw, ImageFilter, ImageFont

def draw_icon(size):
    """Draw the app icon at the given size."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Scale factor relative to 1024
    s = size / 1024.0
    center = size / 2

    # --- Background: rounded rectangle with gradient ---
    # macOS icons use ~22.37% corner radius
    corner_r = int(size * 0.2237)

    # Draw gradient background
    for y in range(size):
        t = y / size
        # Dark gradient: top #2D3748 → bottom #1A202C
        r = int(45 + (26 - 45) * t)
        g = int(55 + (32 - 55) * t)
        b = int(72 + (44 - 72) * t)
        draw.line([(0, y), (size - 1, y)], fill=(r, g, b, 255))

    # Apply rounded rectangle mask
    mask = Image.new("L", (size, size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle([0, 0, size - 1, size - 1], radius=corner_r, fill=255)
    img.putalpha(mask)

    draw = ImageDraw.Draw(img)

    # --- Camera lens (outer ring) ---
    lens_r = int(320 * s)
    lens_cx, lens_cy = center, center + int(20 * s)

    # Outer dark ring
    outer_r = lens_r + int(40 * s)
    draw.ellipse(
        [lens_cx - outer_r, lens_cy - outer_r,
         lens_cx + outer_r, lens_cy + outer_r],
        fill=(20, 25, 35, 255)
    )

    # Metallic ring
    ring_w = int(30 * s)
    for i in range(ring_w):
        t = i / ring_w
        # Silver gradient for the ring
        v = int(100 + 80 * math.sin(t * math.pi))
        r_ring = outer_r - i
        draw.ellipse(
            [lens_cx - r_ring, lens_cy - r_ring,
             lens_cx + r_ring, lens_cy + r_ring],
            outline=(v, v, v + 10, 255)
        )

    # Inner dark area (lens glass)
    draw.ellipse(
        [lens_cx - lens_r, lens_cy - lens_r,
         lens_cx + lens_r, lens_cy + lens_r],
        fill=(15, 18, 28, 255)
    )

    # --- Iris blades (aperture) ---
    blade_r = int(240 * s)
    inner_r = int(120 * s)
    n_blades = 6
    for i in range(n_blades):
        angle = (i * 2 * math.pi / n_blades) - math.pi / 2
        next_angle = ((i + 1) * 2 * math.pi / n_blades) - math.pi / 2
        mid_angle = (angle + next_angle) / 2

        # Each blade is a polygon
        pts = []
        pts.append((
            lens_cx + blade_r * math.cos(angle),
            lens_cy + blade_r * math.sin(angle)
        ))
        pts.append((
            lens_cx + blade_r * 0.85 * math.cos(mid_angle),
            lens_cy + blade_r * 0.85 * math.sin(mid_angle)
        ))
        pts.append((
            lens_cx + inner_r * math.cos(mid_angle + 0.15),
            lens_cy + inner_r * math.sin(mid_angle + 0.15)
        ))
        pts.append((
            lens_cx + inner_r * math.cos(angle + 0.15),
            lens_cy + inner_r * math.sin(angle + 0.15)
        ))

        # Dark blue-grey blades with slight variation
        shade = 35 + (i % 2) * 8
        draw.polygon(pts, fill=(shade, shade + 5, shade + 15, 200))

    # --- Center glass (lens highlight) ---
    glass_r = int(100 * s)
    # Dark center
    draw.ellipse(
        [lens_cx - glass_r, lens_cy - glass_r,
         lens_cx + glass_r, lens_cy + glass_r],
        fill=(8, 10, 20, 255)
    )

    # Specular highlight on glass
    hl_x = lens_cx - int(30 * s)
    hl_y = lens_cy - int(30 * s)
    hl_r = int(45 * s)
    for i in range(hl_r, 0, -1):
        alpha = int(80 * (1 - i / hl_r) ** 2)
        draw.ellipse(
            [hl_x - i, hl_y - i, hl_x + i, hl_y + i],
            fill=(180, 200, 255, alpha)
        )

    # Subtle ring reflection on glass
    for i in range(3):
        ref_r = glass_r - int((10 + i * 15) * s)
        if ref_r > 0:
            draw.ellipse(
                [lens_cx - ref_r, lens_cy - ref_r,
                 lens_cx + ref_r, lens_cy + ref_r],
                outline=(60, 70, 100, 30)
            )

    # --- Broadcast indicator (red dot with glow, top-right) ---
    dot_x = lens_cx + int(220 * s)
    dot_y = lens_cy - int(220 * s)
    dot_r = int(55 * s)

    # Red glow
    for i in range(int(40 * s), 0, -1):
        alpha = int(60 * (1 - i / (40 * s)))
        r_glow = dot_r + i
        draw.ellipse(
            [dot_x - r_glow, dot_y - r_glow,
             dot_x + r_glow, dot_y + r_glow],
            fill=(255, 30, 30, alpha)
        )

    # Solid red dot
    draw.ellipse(
        [dot_x - dot_r, dot_y - dot_r,
         dot_x + dot_r, dot_y + dot_r],
        fill=(230, 40, 40, 255)
    )

    # White highlight on dot
    hl_dot_r = int(18 * s)
    hl_dot_x = dot_x - int(12 * s)
    hl_dot_y = dot_y - int(12 * s)
    draw.ellipse(
        [hl_dot_x - hl_dot_r, hl_dot_y - hl_dot_r,
         hl_dot_x + hl_dot_r, hl_dot_y + hl_dot_r],
        fill=(255, 150, 150, 120)
    )

    # --- Broadcast waves (from the red dot) ---
    for i in range(3):
        wave_r = dot_r + int((30 + i * 28) * s)
        arc_width = max(int(3 * s), 1)
        # Draw arc segment (upper-right quadrant)
        bbox = [dot_x - wave_r, dot_y - wave_r, dot_x + wave_r, dot_y + wave_r]
        alpha = int(150 - i * 40)
        draw.arc(bbox, 200, 290, fill=(255, 80, 80, alpha), width=arc_width)

    # --- "RTMP" text at bottom ---
    text_y = lens_cy + int(290 * s)
    font_size = int(72 * s)
    try:
        font = ImageFont.truetype("/System/Library/Fonts/SFCompact.ttf", font_size)
    except (OSError, IOError):
        try:
            font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", font_size)
        except (OSError, IOError):
            font = ImageFont.load_default()

    text = "RTMP"
    bbox = draw.textbbox((0, 0), text, font=font)
    text_w = bbox[2] - bbox[0]
    text_x = center - text_w / 2
    draw.text((text_x, text_y), text, fill=(200, 210, 230, 200), font=font)

    return img


def main():
    import json
    import os
    import sys

    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    icon_dir = os.path.join(
        base_dir, "swift", "CameraExtension", "Assets.xcassets", "AppIcon.appiconset"
    )
    os.makedirs(icon_dir, exist_ok=True)

    # macOS icon sizes: 16, 32, 64, 128, 256, 512, 1024
    # Each has 1x and 2x variants
    sizes = [
        (16, 1), (16, 2),
        (32, 1), (32, 2),
        (128, 1), (128, 2),
        (256, 1), (256, 2),
        (512, 1), (512, 2),
    ]

    # Generate the master 1024px icon
    print("Generating 1024px master icon...")
    master = draw_icon(1024)

    contents_images = []
    for pt_size, scale in sizes:
        px_size = pt_size * scale
        filename = f"icon_{pt_size}x{pt_size}@{scale}x.png"
        filepath = os.path.join(icon_dir, filename)

        if px_size == 1024:
            icon = master
        else:
            icon = master.resize((px_size, px_size), Image.LANCZOS)

        icon.save(filepath, "PNG")
        print(f"  {filename} ({px_size}x{px_size}px)")

        contents_images.append({
            "filename": filename,
            "idiom": "mac",
            "scale": f"{scale}x",
            "size": f"{pt_size}x{pt_size}"
        })

    # Contents.json
    contents = {
        "images": contents_images,
        "info": {"author": "xcode", "version": 1}
    }
    contents_path = os.path.join(icon_dir, "Contents.json")
    with open(contents_path, "w") as f:
        json.dump(contents, f, indent=2)

    # Also create the top-level Assets.xcassets/Contents.json
    assets_dir = os.path.join(base_dir, "swift", "CameraExtension", "Assets.xcassets")
    assets_contents = os.path.join(assets_dir, "Contents.json")
    if not os.path.exists(assets_contents):
        with open(assets_contents, "w") as f:
            json.dump({"info": {"author": "xcode", "version": 1}}, f, indent=2)

    print(f"\nIcon set generated at: {icon_dir}")
    print(f"Contents.json written with {len(contents_images)} entries")


if __name__ == "__main__":
    main()
