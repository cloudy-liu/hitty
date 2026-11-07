#!/usr/bin/env python3
"""Prepare an isolated sample project and options for native README captures."""

import json
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    repo = Path(__file__).resolve().parents[1]
    output = Path(sys.argv[1]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    project = output / "ctty7"
    for directory in ("src", "docs", "archive"):
        (project / directory).mkdir(parents=True)
    for source, target in (
        ("docs/examples/workbench-tour.md", "README.md"),
        ("tests/fixtures/document-tabs/src/settings.rs", "src/settings.rs"),
        ("tests/fixtures/document-tabs/overview.html", "overview.html"),
        ("tests/fixtures/document-tabs/docs/maintenance/release.md", "docs/release.md"),
    ):
        shutil.copyfile(repo / source, project / target)
    (project / ".gitignore").write_text("target/\narchive/\n", encoding="utf-8")
    (project / "archive/README.md").write_text(
        "# Playground\n\nTry a new idea without closing the current project.\n",
        encoding="utf-8",
    )
    for directory in (project, project / "archive"):
        for args in (
            ("init", "-b", "docs/workbench"),
            ("config", "user.name", "Screenshot fixture"),
            ("config", "user.email", "screenshots@example.invalid"),
            ("config", "core.autocrlf", "true"),
            ("add", "."),
            ("commit", "-m", "Prepare sample workspace"),
        ):
            subprocess.run(["git", "-C", str(directory), *args], check=True, capture_output=True)
    source = project / "src/settings.rs"
    source.write_text(
        source.read_text(encoding="utf-8")
        .replace("    pub restore_sessions: bool,", "    pub restore_sessions: bool,\n    pub preview_local_html: bool,")
        .replace("            restore_sessions: true,", "            restore_sessions: true,\n            preview_local_html: true,"),
        encoding="utf-8",
    )
    for name, dark, active, preview in (
        ("workspace-light", False, 0, True),
        ("workspace-dark", True, 0, True),
        ("editor-light", False, 1, False),
        ("html-preview-light", False, 2, True),
    ):
        options = {
            "root": str(project),
            "files": ["README.md", "src/settings.rs", "overview.html"],
            "output": str(output / f"{name}.ready.json"),
            "active": active,
            "preview": preview,
            "width": 1380,
            "height": 760,
            "dark": dark,
            "dock": False,
            "readme": True,
            "hold_seconds": 600,
        }
        (output / f"{name}.json").write_text(json.dumps(options, indent=2), encoding="utf-8")
    for name, sidebar in (("remote-top", False), ("remote-sidebar", True)):
        options = {"sidebar": sidebar, "auto_names": True,
                   "output": str(output / f"{name}.png"), "hold_seconds": 600}
        (output / f"{name}.json").write_text(json.dumps(options, indent=2), encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()
