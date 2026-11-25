#!/usr/bin/env python3
"""Audit transparency & color purity, trim alpha margins, resize, and emit WebP + manifest.json."""
import json
import re
import sys
from pathlib import Path
from PIL import Image

TOOLS_DIR = Path(__file__).resolve().parent
RAW_DIR = TOOLS_DIR / "raw-assets"
OUT_DIR = TOOLS_DIR.parent / "src" / "assets" / "studio"
MANIFEST_JS = TOOLS_DIR / "manifest.mjs"


def load_manifest_meta():
    text = MANIFEST_JS.read_text(encoding="utf-8")
    items = []
    pattern = re.compile(
        r"\{\s*id:\s*'([^']+)',\s*shape:\s*'([^']+)',\s*transparent:\s*(true|false),\s*editOf:\s*(null|'[^']+'),\s*targetWidth:\s*(\d+),\s*slot:\s*'([^']+)'",
        re.MULTILINE,
    )
    for m in pattern.finditer(text):
        edit_of = None if m.group(4) == "null" else m.group(4).strip("'")
        items.append({
            "id": m.group(1),
            "shape": m.group(2),
            "transparent": m.group(3) == "true",
            "editOf": edit_of,
            "targetWidth": int(m.group(5)),
            "slot": m.group(6),
        })
    return items


def audit_rgba(img: Image.Image):
    rgba = img.convert("RGBA")
    w, h = rgba.size
    total = w * h
    alpha = rgba.getchannel("A")
    amin, amax = alpha.getextrema()
    hist = alpha.histogram()
    zero = sum(hist[0:5])
    near_opaque = sum(hist[245:256])
    semi = total - zero - near_opaque

    pixels = rgba.load()
    border_alphas = []
    for x in range(w):
        border_alphas.append(pixels[x, 0][3])
        border_alphas.append(pixels[x, h - 1][3])
    for y in range(h):
        border_alphas.append(pixels[0, y][3])
        border_alphas.append(pixels[w - 1, y][3])
    border_mean = sum(border_alphas) / max(len(border_alphas), 1)

    step = max(1, min(w, h) // 160)
    purple_hits = 0
    sampled = 0
    for y in range(0, h, step):
        for x in range(0, w, step):
            r, g, b, a = pixels[x, y]
            if a < 40:
                continue
            sampled += 1
            if b > g + 28 and r > g + 18 and (b - g) > 35:
                purple_hits += 1
    purple_pct = (purple_hits / max(sampled, 1)) * 100.0

    return {
        "w": w,
        "h": h,
        "mode": img.mode,
        "alpha_min": amin,
        "alpha_max": amax,
        "pct_transparent": zero * 100.0 / total,
        "pct_semi": semi * 100.0 / total,
        "pct_opaque": near_opaque * 100.0 / total,
        "border_mean_alpha": border_mean,
        "purple_pct": purple_pct,
    }


def de_haze_alpha(rgba: Image.Image, cutoff: int = 12) -> Image.Image:
    alpha = rgba.getchannel("A")
    cleaned = alpha.point(lambda a: 0 if a <= cutoff else a)
    out = rgba.copy()
    out.putalpha(cleaned)
    return out


def alpha_bbox(rgba: Image.Image, threshold: int = 8, pad: int = 8):
    alpha = rgba.getchannel("A").point(lambda a: 255 if a > threshold else 0)
    bbox = alpha.getbbox()
    if not bbox:
        return (0, 0, rgba.width, rgba.height)
    x0, y0, x1, y1 = bbox
    return (
        max(0, x0 - pad),
        max(0, y0 - pad),
        min(rgba.width, x1 + pad),
        min(rgba.height, y1 + pad),
    )


def union_bbox(b1, b2):
    return (min(b1[0], b2[0]), min(b1[1], b2[1]), max(b1[2], b2[2]), max(b1[3], b2[3]))


def main():
    items = load_manifest_meta()
    if not items:
        print("Failed to parse manifest items.", file=sys.stderr)
        sys.exit(1)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    raw_images = {}
    audits = {}

    for item in items:
        raw_path = RAW_DIR / f"{item['id']}.png"
        if not raw_path.exists():
            continue
        img = Image.open(raw_path)
        stats = audit_rgba(img)
        audits[item["id"]] = stats
        raw_images[item["id"]] = img.convert("RGBA") if item["transparent"] else img.convert("RGB")
        print(
            f"[AUDIT] {item['id']:18s} {stats['w']}x{stats['h']} mode={stats['mode']:4s} "
            f"alpha={stats['alpha_min']}..{stats['alpha_max']} "
            f"(trans:{stats['pct_transparent']:.1f}% semi:{stats['pct_semi']:.1f}% opq:{stats['pct_opaque']:.1f}%) "
            f"borderA={stats['border_mean_alpha']:.1f} purple={stats['purple_pct']:.2f}%"
        )

    edit_groups = {}
    for item in items:
        if item["id"] not in raw_images:
            continue
        if item["transparent"]:
            img = raw_images[item["id"]]
            stats = audits[item["id"]]
            # Only copy parent alpha if the edit introduced opaque/hazy background leakage on borders
            if item["editOf"] and item["editOf"] in raw_images:
                parent = raw_images[item["editOf"]]
                if stats["border_mean_alpha"] > 8.0 and parent.size == img.size:
                    print(f"[ALPHA] Reusing clean alpha from {item['editOf']} onto {item['id']} (borderA={stats['border_mean_alpha']:.1f})")
                    img.putalpha(parent.getchannel("A"))
            elif stats["border_mean_alpha"] > 0.5 and stats["border_mean_alpha"] < 18.0:
                img = de_haze_alpha(img, cutoff=12)
            raw_images[item["id"]] = img
            group_key = item["editOf"] or item["id"]
            edit_groups.setdefault(group_key, []).append(item["id"])

    group_boxes = {}
    for gkey, ids in edit_groups.items():
        box = None
        for gid in ids:
            b = alpha_bbox(raw_images[gid])
            box = b if box is None else union_bbox(box, b)
        for gid in ids:
            group_boxes[gid] = box

    manifest_out = {}
    for item in items:
        iid = item["id"]
        if iid not in raw_images:
            continue
        img = raw_images[iid]
        if item["transparent"] and iid in group_boxes:
            img = img.crop(group_boxes[iid])

        target_w = item["targetWidth"]
        if img.width > target_w:
            target_h = max(1, round(img.height * (target_w / img.width)))
            img = img.resize((target_w, target_h), Image.Resampling.LANCZOS)

        out_path = OUT_DIR / f"{iid}.webp"
        q = 88 if item["transparent"] else 85
        img.save(out_path, format="WEBP", quality=q, method=6)
        size_bytes = out_path.stat().st_size
        manifest_out[iid] = {
            "width": img.width,
            "height": img.height,
            "transparent": item["transparent"],
            "bytes": size_bytes,
            "slot": item["slot"],
        }
        print(f"[WEBP]  {iid:18s} -> {out_path.name} ({img.width}x{img.height}, {size_bytes/1024:.1f} KB)")

    (OUT_DIR / "manifest.json").write_text(json.dumps(manifest_out, indent=2) + "\n", encoding="utf-8")
    print(f"\nProcessed {len(manifest_out)} asset(s) into {OUT_DIR}")


if __name__ == "__main__":
    main()
