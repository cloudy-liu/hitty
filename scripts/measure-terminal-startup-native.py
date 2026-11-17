"""Measure actual project-sidebar clicks and split actions with isolated Cmder.

Build: cargo build --locked --features markdown-visual-tests --bin hitty-app
Requires the normal bundled ConPTY assets beside the application executable.
The draw measurement ends when GPUI's draw call returns, not at display scanout.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("cmder", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cwd", type=Path, default=Path.cwd())
    parser.add_argument("--samples", type=int, default=10)
    parser.add_argument("--budget-ms", type=float)
    parser.add_argument("--variant", choices=("baseline", "optimized"), default="baseline")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Windows and installed Cmder are required")
    if not 1 <= args.samples <= 100:
        parser.error("samples must be 1..100")
    if args.budget_ms is not None and (not math.isfinite(args.budget_ms) or args.budget_ms <= 0):
        parser.error("budget must be finite and positive")
    app = args.app.resolve()
    for file in (app, app.parent / "conpty.dll", app.parent / "OpenConsole.exe"):
        if not file.is_file():
            parser.error(f"required application asset is missing: {file}")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    repo = Path(__file__).resolve().parents[1]
    subprocess.run([os.sys.executable, str(repo / "scripts/prepare-cmder-startup.py"),
                    str(args.cmder.resolve()), str(output / "cmder"), "--samples", "1",
                    "--cwd", str(args.cwd.resolve())], check=True)
    fixture = output / "cmder" / args.variant
    config_dir = fixture / "hitty-profile"
    config = json.loads((config_dir / "config.json").read_text())
    shell = json.loads((fixture / "options.json").read_text())["shell"]
    config.update(shell=shell, check_for_updates=False, auto_download_updates=False,
                  install_cli_on_path=False, show_tray_icon=False, restore_session=False,
                  sidebar_width=240, sidebar_pinned_groups=[str(args.cwd.resolve())],
                  theme_follow_system=False, theme_preset="dark", cursor_blink=False)
    (config_dir / "config.json").write_text(json.dumps(config), encoding="utf-8")
    (config_dir / "terminal-startup-fixture").touch()
    result = output / "measurements.json"
    options = output / "native-options.json"
    options.write_text(json.dumps({"cwd": str(args.cwd.resolve()), "output": str(result),
                                  "samples": args.samples, "budget_ms": args.budget_ms}), encoding="utf-8")
    environment = os.environ.copy()
    for key in list(environment):
        low = key.lower()
        if low.startswith(("hitty", "cmder_", "clink_", "lib_", "print_")) or low in (
            "aliases", "user_aliases", "fast_init", "debug_output", "verbose_output",
            "git_install_root", "hc_session", "hc_terminal_id",
        ):
            del environment[key]
    environment.update(HITTY_CONFIG_DIR=str(config_dir), HITTY_DATA_DIR=str(config_dir / "data"))
    with (output / "native.log").open("wb") as log:
        try:
            run = subprocess.run([str(app), "--config-dir", str(config_dir),
                                  "--terminal-startup", str(options)], env=environment,
                                 stdout=log, stderr=log, timeout=90 + args.samples * 90,
                                 creationflags=subprocess.CREATE_NO_WINDOW)
        finally:
            # Only the daemon owned by this newly created, marked profile.
            subprocess.run([str(app), "--config-dir", str(config_dir), "--stop-daemon"],
                           env=environment, stdout=log, stderr=log, check=True, timeout=30,
                           creationflags=subprocess.CREATE_NO_WINDOW)
    if not result.exists():
        raise RuntimeError(f"No measurements; see {output / 'native.log'} (exit {run.returncode})")
    rows = json.loads(result.read_text())
    with app.open("rb") as binary:
        binary_hash = hashlib.file_digest(binary, "sha256").hexdigest()
    summary = {"binary": str(app), "binary_sha256": binary_hash,
               "cwd": str(args.cwd.resolve()),
               "cmder_variant": args.variant,
               "scope": "native sidebar mouse dispatch / split action through prompt observation and GPUI draw return; not display scanout",
               "cold_fixture": next((r for r in rows if r.get("entry") == "cold_fixture"), None),
               "errors": [r["error"] for r in rows if "error" in r]}
    for entry in ("sidebar_button", "split_action"):
        timings = sorted(r["draw_after_prompt_ms"] for r in rows if r.get("entry") == entry)
        if timings:
            p95 = timings[math.ceil(len(timings) * .95) - 1]
            summary[entry] = {"samples": len(timings), "p50_ms": statistics.median(timings),
                              "p95_ms": p95, "max_ms": max(timings),
                              "budget_ms": args.budget_ms,
                              "within_budget": args.budget_ms is None or p95 < args.budget_ms}
    (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, indent=2))
    if run.returncode or summary["errors"] or any(summary.get(entry, {}).get("samples") != args.samples
                                                  for entry in ("sidebar_button", "split_action")):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
