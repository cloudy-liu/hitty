"""Capture the same isolated application scene before/after full theme styles."""
import argparse
import ctypes
import hashlib
import json
import os
import re
from pathlib import Path
import runpy
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
wait_for = runpy.run_path(str(ROOT / "scripts/bootstrap-native-smoke.py"))["wait_for"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", choices=["before", "after"], required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    args = parser.parse_args()
    app = args.bin_dir / "hitty-app.exe"
    executable = args.bin_dir / "hitty.exe"
    app, executable = app.resolve(), executable.resolve()
    evidence = ROOT / "docs/testing/images/otty-theme-fidelity" / args.stage
    evidence.mkdir(parents=True, exist_ok=True)
    profile = Path(tempfile.mkdtemp(prefix=f"hitty-otty-{args.stage}-"))
    if args.stage == "before":
        themes = profile / "themes"
        themes.mkdir()
        for source in (ROOT / "assets/themes/otty").glob("*.yaml"):
            (themes / source.name).write_text(
                source.read_text(encoding="utf-8").split("\nstyle:\n")[0] + "\n",
                encoding="utf-8")
        assert len(list(themes.glob("*.yaml"))) == 25
    sample = str(ROOT / "scripts/otty-theme-review-sample.ps1").replace("'", "''")
    config = {
        "theme_preset": "one_light", "theme_follow_system": False,
        "theme_legible_palette": False, "window_blur": None,
        "window_backdrop": "auto", "window_opacity": None,
        "gui_language": "en", "font_size": 15, "ui_font_size": 14,
        "check_for_updates": False, "auto_download_updates": False,
        "install_cli_on_path": False, "show_tray_icon": False,
        "shell": {"program": "powershell.exe", "args": [
            "-NoLogo", "-NoProfile", "-NoExit", "-ExecutionPolicy", "Bypass",
            "-Command", "Set-Location $env:SystemRoot; "
            "function global:prompt { 'PS theme-review> ' }; " + f"& '{sample}'"]},
    }
    config_path = profile / "config.json"

    def save():
        temporary = profile / "config.tmp"
        temporary.write_text(json.dumps(config), encoding="utf-8")
        temporary.replace(config_path)

    save()
    (profile / "window.json").write_text(
        '{"x":80,"y":80,"width":1200,"height":800}', encoding="utf-8")
    env = {k: v for k, v in os.environ.items() if k != "HITTY" and not k.startswith("HITTY_")}
    env.update(HITTY_CONFIG_DIR=str(profile), HITTY_DATA_DIR=str(profile / "data"))

    def cli(*command, check=True):
        result = subprocess.run([str(executable), *command], env=env, capture_output=True,
                                text=True, encoding="utf-8", errors="replace", timeout=30)
        if check and result.returncode:
            raise RuntimeError(result.stderr or result.stdout)
        return result

    entries = json.loads((ROOT / "assets/themes/otty/manifest.json").read_text(encoding="utf-8"))["themes"]
    modes = {entry["id"]: entry["mode"] for entry in entries}
    modes["one_light"] = "light"
    ids = sorted(entry["id"] for entry in entries)
    log_path = ROOT / f".hitty-dev-otty-review/logs/fidelity-{args.stage}-native.log"
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("wb") as log:
        process = subprocess.Popen([str(app), "--config-dir", str(profile)], env=env,
                                   cwd=os.environ["SystemRoot"],
                                   stdout=log, stderr=log, creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            def panes():
                if process.poll() is not None:
                    raise RuntimeError(f"Application exited: {process.returncode}")
                result = cli("pane", "ls", "--json", check=False)
                return json.loads(result.stdout)["panes"] if result.returncode == 0 else None

            pane = wait_for(panes, "theme fidelity terminal")[0]
            address = "%" + str(pane["pane"])
            wait_for(lambda: "Glass variants" in cli("capture", address, "--plain").stdout,
                     "complete sample output")
            cli("ws", "rename", str(pane["workspace"]), "Theme fidelity")
            records = []
            scenes = [(theme, "left", False, theme)
                      for theme in ["one_light", *ids, "one_light"]]
            scenes += [(theme, "top", False, theme + "-top") for theme in
                       ["otty-april", "otty-floating-card", "otty-nord", "otty-glass-light"]]
            scenes += [(theme, "left", True, theme + "-details") for theme in
                       ["otty-acrylic-dark", "otty-glass-light"]]
            for theme_id, layout, details, scene in scenes:
                config["theme_preset"] = theme_id
                config["tab_bar_position"] = layout
                config["right_panel_visible"] = details
                save()
                image_path = evidence / f"{scene}.png"
                result = subprocess.run([
                    "powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                    str(ROOT / "scripts/capture-native-window.ps1"), "-ProcId", str(process.pid),
                    "-Out", str(image_path), "-SettleMs", "1300",
                ], capture_output=True, text=True, check=True, timeout=30)
                native_mode = ctypes.c_int()
                hwnd = int(re.search(r"hwnd=(\d+)", result.stdout)[1])
                native_result = ctypes.windll.dwmapi.DwmGetWindowAttribute(
                    ctypes.c_void_p(hwnd), 20, ctypes.byref(native_mode), ctypes.sizeof(native_mode))
                if args.stage == "after" and native_result == 0:
                    assert bool(native_mode.value) == (modes[theme_id] == "dark"), (theme_id, native_mode.value)
                text = cli("capture", address, "--plain").stdout
                assert "HITTY / OTTY THEME COMPARISON" in text, theme_id
                records.append({
                    "theme": theme_id, "scene": scene, "layout": layout,
                    "details": details, "capture": result.stdout.strip(),
                    "native_dark_mode": native_mode.value if native_result == 0 else None,
                    "image_sha256": hashlib.sha256(image_path.read_bytes()).hexdigest(),
                    "text_sha256": hashlib.sha256(text.encode()).hexdigest(),
                    "pane": pane["pane"],
                })
                print(f"{args.stage}: {scene}", flush=True)
            assert records[0]["text_sha256"] == records[len(ids) + 1]["text_sha256"], "switching lost terminal output"
            metadata = {
                "stage": args.stage, "app": str(app),
                "binary_sha256": hashlib.sha256(app.read_bytes()).hexdigest(),
                "profile": str(profile), "configuration": config,
                "logical_window": [1200, 800], "captures": records,
                "same_pane_across_all_themes": True,
                "original_terminal_output_restored": True,
            }
            (evidence / "captures.json").write_text(
                json.dumps(metadata, indent=2) + "\n", encoding="utf-8", newline="\n"
            )
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=15)
            cli("server", "stop", check=False)


if __name__ == "__main__":
    main()
