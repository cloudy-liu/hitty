#!/usr/bin/env python3
"""Sync every bundled Herdr manifest at one commit, or check it offline."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re
import sys
import tomllib
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "herdrdev/herdr"


def manifest_names(root):
    source = (root / "src/terminal/screen_status.rs").read_text(encoding="utf-8")
    names = set(re.findall(r'include_str!\(\s*"../../assets/agent-detection/([\w-]+\.toml)"\s*\)', source))
    if not names:
        raise ValueError("the detection engine must register its bundled manifests")
    return names


def engine_version(root):
    source = (root / "src/terminal/screen_status.rs").read_text(encoding="utf-8")
    match = re.search(r"const MANIFEST_ENGINE_VERSION: u32 = (\d+);", source)
    if match is None:
        raise ValueError("the detection engine must declare its manifest engine version")
    return int(match[1])


def check(root):
    directory = root / "assets/agent-detection"
    inventory = json.loads((directory / "upstream.json").read_text(encoding="utf-8"))
    if inventory["repository"] != REPOSITORY or not re.fullmatch(r"[0-9a-f]{40}", inventory["commit"]):
        raise ValueError("upstream.json must identify one pinned Herdr commit")
    actual = {path.name for path in directory.glob("*.toml")}
    if manifest_names(root) != actual or actual != set(inventory["manifests"]):
        raise ValueError("upstream.json must cover every bundled manifest, with no missing or extra files")
    for name, record in inventory["manifests"].items():
        content = (directory / name).read_bytes()
        if hashlib.sha256(content).hexdigest() != record["sha256"]:
            raise ValueError(f"{name}: differs from the pinned upstream bytes; use --update to sync all manifests")
        manifest = tomllib.loads(content.decode("utf-8"))
        if manifest["version"] != record["version"]:
            raise ValueError(f"{name}: version differs from upstream.json")
        if manifest.get("min_engine_version", 1) > engine_version(root):
            raise ValueError(f"{name}: requires a newer detection engine")
    print(f"Verified all {len(inventory['manifests'])} manifests at {inventory['commit']}")


def update(root, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("--update requires a full commit SHA, not a moving branch or tag")
    directory = root / "assets/agent-detection"
    supported_version = engine_version(root)

    def fetch(name):
        upstream_name = "github-copilot.toml" if name == "copilot.toml" else name
        path = f"src/detect/manifests/{upstream_name}"
        with urlopen(f"https://raw.githubusercontent.com/{REPOSITORY}/{commit}/{path}", timeout=30) as response:
            content = response.read()
        manifest = tomllib.loads(content.decode("utf-8"))
        if manifest.get("min_engine_version", 1) > supported_version:
            raise ValueError(f"{name}: requires a newer detection engine")
        return name, content, {"path": path, "version": manifest["version"], "sha256": hashlib.sha256(content).hexdigest()}

    # Fetch and validate the whole set before changing any checked-in file.
    with ThreadPoolExecutor(max_workers=4) as pool:
        fetched = list(pool.map(fetch, sorted(manifest_names(root))))
    inventory = {"repository": REPOSITORY, "commit": commit, "manifests": {name: record for name, _, record in fetched}}
    for name, content, _ in fetched:
        (directory / name).write_bytes(content)
    (directory / "upstream.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8", newline="\n")
    check(root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--check", action="store_true", help="verify all manifests offline (the default)")
    action.add_argument("--update", metavar="COMMIT", help="sync every supported agent from a pinned Herdr commit")
    args = parser.parse_args()
    try:
        update(ROOT, args.update) if args.update else check(ROOT)
    except (OSError, ValueError, KeyError) as error:
        print(f"agent detection: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
