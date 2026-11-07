"""Vendor the Symbols file icon theme (MIT, Miguel Solorio) into assets/.

    python scripts/sync-symbols-icons.py <symbols>

<symbols> is either a checkout of github.com/miguelsolorio/vscode-symbols at
the tag being taken, or an installed copy of the extension
(~/.vscode/extensions/miguelsolorio.symbols-<version>-universal). Both lay the
theme out the same way under src/.

The vendored tree is replaced wholesale, so an icon upstream deleted does not
linger here. build.rs embeds every SVG it finds; the theme JSON is read as is
by `ui::file_icons`, so nothing else needs editing after a sync.
"""

import json
import shutil
import sys
from pathlib import Path

DEST = Path(__file__).resolve().parent.parent / "assets" / "file-icons" / "symbols"


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    root = Path(sys.argv[1]).expanduser().resolve()
    src = root / "src"
    theme = src / "symbol-icon-theme.json"
    if not theme.is_file():
        sys.exit(f"{theme} not found; is {root} a Symbols checkout?")

    version = json.loads((root / "package.json").read_text(encoding="utf-8"))["version"]
    license_file = next(
        (p for p in (root / "LICENSE.txt", root / "LICENSE", root / "LICENSE.md") if p.is_file()),
        None,
    )
    if license_file is None:
        sys.exit(f"no LICENSE in {root}")

    if DEST.exists():
        shutil.rmtree(DEST)
    for kind in ("files", "folders"):
        out = DEST / kind
        out.mkdir(parents=True)
        for svg in sorted((src / "icons" / kind).glob("*.svg")):
            shutil.copyfile(svg, out / svg.name)
    shutil.copyfile(theme, DEST / "symbol-icon-theme.json")
    shutil.copyfile(license_file, DEST / "LICENSE")
    (DEST / "VERSION").write_text(f"{version}\n", encoding="utf-8")

    count = sum(1 for _ in DEST.rglob("*.svg"))
    print(f"vendored Symbols {version}: {count} icons -> {DEST}")


if __name__ == "__main__":
    main()
