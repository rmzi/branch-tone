#!/usr/bin/env python3
"""Generate macOS menu bar template icons for branch-tone.

Design: speaker cone on the left, three gracefully curving stems with
teardrop leaves emanating from it — organic branches replacing sound waves.

Template images: black shapes on transparent background.
macOS handles light/dark mode automatically.

Produces:
  icon.png           (22x22)  - normal state
  icon@2x.png        (44x44)  - normal state retina
  icon-muted.png     (22x22)  - muted state
  icon-muted@2x.png  (44x44)  - muted state retina
"""

from PIL import Image, ImageDraw
import math, os

OUT = os.path.dirname(os.path.abspath(__file__))


def bezier(p0, p1, p2, p3, t):
    """Cubic bezier at parameter t."""
    u = 1 - t
    return (
        u**3 * p0[0] + 3*u**2*t * p1[0] + 3*u*t**2 * p2[0] + t**3 * p3[0],
        u**3 * p0[1] + 3*u**2*t * p1[1] + 3*u*t**2 * p2[1] + t**3 * p3[1],
    )


def draw_thick_curve(draw, points, width, color):
    """Draw a smooth thick curve through a list of points."""
    for i in range(len(points) - 1):
        draw.line([points[i], points[i+1]], fill=color, width=width)


def draw_leaf(draw, base_x, base_y, angle, length, width, color):
    """Draw a teardrop/elliptical leaf at given position and angle."""
    # Generate leaf outline as a series of points
    pts = []
    n = 16
    for i in range(n + 1):
        t = i / n
        # Teardrop shape: wider at base, pointed at tip
        # Width varies as sin curve, shifted to be wider near base
        w = math.sin(t * math.pi) * width * (1 - 0.3 * t)
        along = t * length

        # Points on both sides of the leaf midline
        cos_a = math.cos(angle)
        sin_a = math.sin(angle)

        cx = base_x + along * cos_a
        cy = base_y + along * sin_a

        # Perpendicular
        px, py = -sin_a, cos_a

        if i == 0 or i == n:
            pts.append((cx, cy))
        else:
            pts.append((cx + px * w, cy + py * w))

    # Close the shape by going back along the other side
    for i in range(n, -1, -1):
        t = i / n
        w = math.sin(t * math.pi) * width * (1 - 0.3 * t)
        along = t * length

        cos_a = math.cos(angle)
        sin_a = math.sin(angle)
        cx = base_x + along * cos_a
        cy = base_y + along * sin_a
        px, py = -sin_a, cos_a

        if i == 0 or i == n:
            continue
        pts.append((cx - px * w, cy - py * w))

    if len(pts) >= 3:
        draw.polygon(pts, fill=color)


def draw_icon(size, muted=False):
    """Draw branch-tone icon at given size."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    s = size
    f = s / 44.0  # scale relative to 44px base
    black = (0, 0, 0, 255)

    # === SPEAKER (left side) ===
    # Position speaker to use left ~40% of icon
    # Rear rectangle (small box)
    rx1 = int(2 * f)
    ry1 = int(18 * f)
    rx2 = int(6 * f)
    ry2 = int(26 * f)
    draw.rectangle([rx1, ry1, rx2, ry2], fill=black)

    # Cone (trapezoid widening to the right)
    cone_right = int(16 * f)
    cone_top = int(12 * f)
    cone_bot = int(32 * f)
    draw.polygon([
        (rx2, ry1),
        (rx2, ry2),
        (cone_right, cone_bot),
        (cone_right, cone_top),
    ], fill=black)

    if muted:
        # Diagonal slash through speaker
        slash_w = max(2, int(2.5 * f))
        draw.line(
            [(cone_right + int(3*f), cone_top - int(3*f)),
             (rx1 - int(1*f), cone_bot + int(3*f))],
            fill=black, width=slash_w
        )
    else:
        # === THREE BRANCH STEMS WITH LEAVES ===
        stem_origin_x = cone_right + int(2 * f)
        center_y = s / 2.0

        # Each stem: (y_offset, curvature, reach_fraction)
        stem_defs = [
            (0, 0.15, 1.0),          # middle: nearly straight, longest
            (-5.5 * f, -0.55, 0.82), # top: curves upward
            (5.5 * f, 0.55, 0.82),   # bottom: curves downward
        ]

        max_reach = int(23 * f)
        stem_w = max(1, int(1.3 * f))

        for y_off, curve, reach in stem_defs:
            # Build bezier control points for the stem
            sx = stem_origin_x
            sy = center_y + y_off * 0.3  # stems converge near speaker
            ex = sx + max_reach * reach
            ey = center_y + y_off + curve * max_reach * 0.5

            # Control points for smooth S-ish curve
            cp1 = (sx + max_reach * reach * 0.3, sy + y_off * 0.3)
            cp2 = (sx + max_reach * reach * 0.7, ey - curve * max_reach * 0.1)

            # Sample curve
            n_pts = 30
            pts = [bezier((sx, sy), cp1, cp2, (ex, ey), t/n_pts) for t in range(n_pts + 1)]
            draw_thick_curve(draw, pts, stem_w, black)

            # Leaves along the stem
            n_leaves = 3 if reach > 0.9 else 2
            for li in range(n_leaves):
                t = 0.3 + li * 0.25
                idx = int(t * n_pts)
                px, py = pts[min(idx, len(pts)-1)]

                # Tangent
                idx_next = min(idx + 2, len(pts)-1)
                idx_prev = max(idx - 2, 0)
                tdx = pts[idx_next][0] - pts[idx_prev][0]
                tdy = pts[idx_next][1] - pts[idx_prev][1]
                stem_angle = math.atan2(tdy, tdx)

                # Alternate leaf sides, angled off the stem
                side = 1 if li % 2 == 0 else -1
                leaf_angle = stem_angle + side * math.pi * 0.35

                leaf_len = max(2, int(4.2 * f * (0.7 + 0.3 * t)))
                leaf_wid = max(1, int(1.6 * f * (0.7 + 0.3 * t)))

                draw_leaf(draw, px, py, leaf_angle, leaf_len, leaf_wid, black)

            # Small leaf at the tip
            tip_x, tip_y = pts[-1]
            tip_dx = pts[-1][0] - pts[-3][0]
            tip_dy = pts[-1][1] - pts[-3][1]
            tip_angle = math.atan2(tip_dy, tip_dx)
            tip_len = max(2, int(3.5 * f))
            tip_wid = max(1, int(1.3 * f))
            draw_leaf(draw, tip_x, tip_y, tip_angle, tip_len, tip_wid, black)

    return img


if __name__ == "__main__":
    for muted in [False, True]:
        for size, suffix in [(44, "@2x"), (22, "")]:
            name = f"icon-muted{suffix}.png" if muted else f"icon{suffix}.png"
            img = draw_icon(size, muted=muted)
            path = os.path.join(OUT, name)
            img.save(path)
            print(f"  {name}: {img.size}")

    # Large previews for review
    for muted in [False, True]:
        name = "preview-muted.png" if muted else "preview.png"
        img = draw_icon(256, muted=muted)
        path = os.path.join(OUT, name)
        img.save(path)
        print(f"  {name}: {img.size} (preview)")

    print("Done!")
