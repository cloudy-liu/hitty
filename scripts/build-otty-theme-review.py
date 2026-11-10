"""Build a portable, offline review of the verified native Otty screenshots."""

import argparse
import base64
import hashlib
import json
from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "docs/testing/otty-theme-review.html")
    args = parser.parse_args()
    directory = ROOT / "docs/testing/images/otty-hidpi/after"
    capture = json.loads((directory / "captures.json").read_text(encoding="utf-8"))
    assert capture["configuration"]["right_panel_visible"] is True
    # The review documents all candidates, including the profiles since removed.
    manifest = json.loads((ROOT / "docs/testing/otty-theme-selection.json").read_text(encoding="utf-8"))
    records = {record["scene"]: record for record in capture["captures"]}

    def image(scene):
        record = records[scene]
        assert record["layout"] == "left", scene
        raw = (directory / record["image"]).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == record["image_sha256"], scene
        assert raw[:8] == b"\x89PNG\r\n\x1a\n", scene
        assert struct.unpack(">II", raw[16:24]) == (2104, 1402), scene
        assert record["native"]["dpi"] == 168, scene
        return "data:image/png;base64," + base64.b64encode(raw).decode("ascii")

    themes = []
    for theme in sorted(manifest["themes"], key=lambda entry: entry["name"].casefold()):
        materials = ["unverified"] if theme["id"] in {
            "otty-acrylic-dark", "otty-ayu-dark", "otty-ayu-light", "otty-floating-card",
            "otty-glass-dark", "otty-glass-light", "otty-one-light",
        } else []
        themes.append({
            "id": theme["id"], "name": theme["name"].removesuffix(" - otty"),
            "mode": theme["mode"], "author": theme["author"],
            "materials": materials,
            "images": {"left": image(f"left-{theme['id']}")},
        })
    assert len(themes) == 25
    data = {
        "version": "hitty-typography-2026-11-10",
        "date": "2026-11-10",
        "binarySha256": capture["binary_sha256"],
        "configuration": {key: capture["configuration"][key] for key in (
            "font_family", "font_size", "ui_font_size", "line_height")},
        "sidebarCrops": {"left": [0, 0, 388, 1402], "right": [1644, 0, 460, 1402]},
        "themes": themes,
        "decision": manifest["decision"],
        "baseline": {"left": image("left-baseline")},
    }
    template = (ROOT / "docs/testing/otty-theme-review.template.html").read_text(encoding="utf-8")
    assert template.count("__REVIEW_DATA__") == 1
    document = template.replace("__REVIEW_DATA__", json.dumps(data, ensure_ascii=False).replace("<", "\\u003c"))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(document, encoding="utf-8", newline="\n")
    print(f"Built {args.out}: 25 dual-sidebar captures + 1 Hitty reference, {args.out.stat().st_size:,} bytes")


if __name__ == "__main__":
    main()
