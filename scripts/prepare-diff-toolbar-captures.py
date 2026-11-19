"""Extend the History fixture with eight files for native Diff toolbar checks."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def prepare(destination: Path) -> None:
    subprocess.run([
        sys.executable,
        str(Path(__file__).with_name("prepare-git-history-captures.py")),
        str(destination), "--detail-body",
    ], check=True)
    root = destination / "history-workspace"
    env = dict(os.environ)
    env.update(
        GIT_AUTHOR_NAME="Hitty review",
        GIT_AUTHOR_EMAIL="review@hitty.example.invalid",
        GIT_COMMITTER_NAME="Hitty review",
        GIT_COMMITTER_EMAIL="review@hitty.example.invalid",
        GIT_AUTHOR_DATE="2026-11-19T14:26:00Z",
        GIT_COMMITTER_DATE="2026-11-19T14:26:00Z",
    )

    def git(*args: str) -> str:
        return subprocess.run(
            ["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=", *args],
            cwd=root, env=env, check=True, capture_output=True, text=True,
        ).stdout.strip()

    names = ["diff_overlay.rs", "diff_commit.rs", "diff_rows.rs", "diff_context.rs",
             "code_editor.rs", "tab_sidebar.rs", "document_column.rs", "theme.rs"]
    source = root / "src" / "ui"
    source.mkdir(parents=True, exist_ok=True)
    for revision in range(3):
        for name in names:
            lines = [
                f"// Review sample: {name}", "", "pub fn render_review() {",
                "    let snapshot = load_snapshot();",
                "    let layout = current_layout();",
                "    let files = snapshot.files();", "",
                f"    let context_lines = {3 + revision};",
                f"    let keep_position = {str(revision > 0).lower()};", "",
                "    for file in files {",
                "        render_file(file, layout, context_lines);",
                "    }", "", "    if keep_position {",
                "        restore_file_position();", "    }", "}", "",
            ]
            (source / name).write_text("\n".join(lines), encoding="utf-8")
        if revision < 2:
            git("add", ".")
            subject = ("feat(review): add the file review samples" if revision == 0
                       else "fix(diff): preserve the review position")
            git("commit", "-m", subject, "-m",
                "Keep the current file in view when changing the diff layout.\n"
                "Share the controls between worktree and commit diffs.")

    for theme in ("light", "dark"):
        options = {
            "root": str(root), "files": ["README.md"],
            "output": str(destination / f"{theme}-ready.json"), "preview": True,
            "width": 1280, "height": 800, "dark": theme == "dark",
            "history": True, "history_width": 300, "history_height": 550,
            "hold_seconds": 600,
        }
        (destination / f"{theme}.json").write_text(
            json.dumps(options, indent=2), encoding="utf-8",
        )
    print(json.dumps({"root": str(root), "commit": git("rev-parse", "HEAD")}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    prepare(parser.parse_args().destination.resolve())
