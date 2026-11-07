"""Prepare a real isolated Git repository for matching native History captures."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request


AUTHORS = [
    ("l0ng-ai", "24760907+l0ng-ai@users.noreply.github.com", "24760907"),
    ("webdev", "86946125+biztex@users.noreply.github.com", "86946125"),
    ("ayamir", "61657399+ayamir@users.noreply.github.com", "61657399"),
]


def prepare(destination: Path, download_avatars: bool, profile_commits: int = 0,
            detail_body: bool = False) -> None:
    if destination.exists():
        raise SystemExit("Refusing to reuse an existing evidence directory")
    root = destination / "history-workspace"
    root.mkdir(parents=True)
    tick = 0

    def git(*args: str, author: int = 0) -> None:
        nonlocal tick
        name, email, _ = AUTHORS[author % len(AUTHORS)]
        env = dict(os.environ)
        env.update(
            GIT_AUTHOR_NAME=name,
            GIT_AUTHOR_EMAIL=email,
            GIT_COMMITTER_NAME=name,
            GIT_COMMITTER_EMAIL=email,
            GIT_AUTHOR_DATE=f"2026-10-20T{10 + tick // 60:02}:{tick % 60:02}:00Z",
            GIT_COMMITTER_DATE=f"2026-10-20T{10 + tick // 60:02}:{tick % 60:02}:00Z",
        )
        subprocess.run(
            ["git", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=", *args],
            cwd=root, env=env, check=True, capture_output=True,
        )
        tick += 1

    git("init", "--initial-branch=main")
    (root / "README.md").write_text(
        "# Git History\n\n"
        "A native acceptance workspace for reviewing commit history.\n\n"
        "## Read a commit at a glance\n\n"
        "The commit subject stays on its own line. Author and age sit below it, "
        "alongside branch and release labels.\n\n"
        "## Keep the graph\n\n"
        "Follow the colored lanes to see where a branch leaves and rejoins. "
        "Merge nodes and tags remain visible beside author avatars.\n\n"
        "## Review the changes\n\n"
        "Choose a commit to open its changes in the existing Git Diff View. "
        "Switch between unified and side-by-side views with the same controls.\n\n"
        "This repository contains controlled commits for screenshot acceptance.\n",
        encoding="utf-8",
    )
    git("add", ".")
    git("commit", "-m", "docs: describe the history acceptance workspace")
    git("tag", "v0.8.1")
    for i, subject in enumerate([
        "feat(history): preserve branch and release labels",
        "fix(history): keep full commit scopes in narrow sidebars",
        "perf(history): render only the visible commit rows",
    ]):
        (root / "history.txt").write_text(f"History layout revision {i + 1}\n")
        git("add", ".")
        git("commit", "-m", subject, author=i)
    git("checkout", "-b", "avatar-cache")
    (root / "avatars.txt").write_text("Shared author images, with local initials.\n")
    git("add", ".")
    git("commit", "-m", "feat(avatars): reuse images across repeated authors", author=2)
    (root / "avatars.txt").write_text("Shared author images, with an offline cache.\n")
    git("add", ".")
    message = ["-m", "fix(avatars): retain cached photos while offline"]
    if detail_body:
        message += ["-m", "Keep the last successful author photo available when a refresh fails.\n"
                    "Repeated commits from the same author share one cached image.\n\n"
                    "The selected commit opens in the existing Git Diff View.\n"
                    "History stays available for reviewing the next commit.\n"
                    "The existing unified and split controls remain in the toolbar."]
    git("commit", *message, author=1)
    git("checkout", "main")
    for i, subject in enumerate([
        "fix(diff): use the existing unified and side-by-side controls",
        "feat(history): show author and age beneath the commit subject",
        "fix(history): preserve tag labels when the sidebar narrows",
        "perf(avatars): bound background requests and image memory",
    ]):
        (root / "history.txt").write_text(f"History layout revision {i + 4}\n")
        git("add", ".")
        git("commit", "-m", subject, author=i + 1)
    git("merge", "--no-ff", "avatar-cache", "-m", "Merge branch 'avatar-cache'")
    git("tag", "v0.8.2")
    if profile_commits:
        if not 11 <= profile_commits <= 5000:
            raise SystemExit("Profile commit count must be between 11 and 5000")
        parent = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=root
        ).decode().strip()
        records = ["feature done\n"]
        for index in range(1, profile_commits - 11 + 1):
            name, email, _ = AUTHORS[index % len(AUTHORS)]
            subject = f"perf(history): controlled long-history sample {index:04}"
            contents = f"History performance sample {index}\n"
            timestamp = 1792494000 + index
            records.append(
                f"commit refs/heads/main\nmark :{index}\n"
                f"author {name} <{email}> {timestamp} +0000\n"
                f"committer {name} <{email}> {timestamp} +0000\n"
                f"data {len(subject.encode())}\n{subject}\n"
                f"from {parent if index == 1 else ':' + str(index - 1)}\n"
                f"M 100644 inline history.txt\ndata {len(contents.encode())}\n{contents}\n"
            )
        records.append("done\n")
        subprocess.run(
            ["git", "fast-import", "--quiet"], cwd=root,
            input="".join(records).encode(), check=True, capture_output=True,
        )
        git("reset", "--hard", "main")

    images = {}
    if download_avatars:
        for _, email, account_id in AUTHORS:
            url = f"https://avatars.githubusercontent.com/u/{account_id}?s=96&v=4"
            request = urllib.request.Request(url, headers={"User-Agent": "ctty7-acceptance"})
            with urllib.request.urlopen(request, timeout=15) as response:
                data = response.read(256 * 1024 + 1)
            if len(data) > 256 * 1024:
                raise SystemExit("Avatar exceeds the fixture image limit")
            images[hashlib.sha256(email.strip().lower().encode()).hexdigest()] = data

    cases = []
    for dark in [False, True]:
        for width in [320, 260]:
            case = ("dark" if dark else "light") + f"-{width}"
            config = destination / f"{case}-config"
            cache = config / "cache" / "avatars"
            cache.mkdir(parents=True)
            for key, data in images.items():
                (cache / f"{key}.png").write_bytes(data)
            options = {
                "root": str(root.resolve()),
                "files": ["README.md"],
                "output": str((destination / f"{case}-ready.json").resolve()),
                "preview": True,
                "width": 1280,
                "height": 800,
                "dark": dark,
                "history": True,
                "history_width": width,
                "history_height": 400,
                "history_profile": bool(profile_commits),
                "hold_seconds": 30 if profile_commits else 600,
            }
            (destination / f"{case}.json").write_text(json.dumps(options, indent=2))
            cases.append({"case": case, "config": str(config.resolve())})
    (destination / "manifest.json").write_text(
        json.dumps({"cases": cases, "seeded_avatars": len(images)}, indent=2)
    )
    print(json.dumps({"directory": str(destination.resolve()), "cases": cases}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--download-avatars", action="store_true")
    parser.add_argument("--profile-commits", type=int, default=0)
    parser.add_argument("--detail-body", action="store_true")
    args = parser.parse_args()
    prepare(args.destination, args.download_avatars, args.profile_commits, args.detail_body)
