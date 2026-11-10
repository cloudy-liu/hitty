"""Capture all Otty selections and verify Hitty typography survives a round trip."""

import argparse
import ctypes
from ctypes import wintypes
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


def native_state(capture):
    hwnd = wintypes.HWND(int(re.search(r"hwnd=(\d+)", capture).group(1)))
    state = {"dpi": ctypes.windll.user32.GetDpiForWindow(hwnd)}
    for name, attribute in [("dark", 20), ("system_backdrop_type", 38)]:
        value = ctypes.c_int()
        result = ctypes.windll.dwmapi.DwmGetWindowAttribute(
            hwnd, attribute, ctypes.byref(value), ctypes.sizeof(value))
        if result and attribute == 20:
            raise RuntimeError(f"DWM attribute {attribute}: HRESULT {result}")
        state[name] = value.value if result == 0 else None
        if result:
            state[f"{name}_hresult"] = result
    return state


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", choices=("before", "after", "retained"), required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    args = parser.parse_args()
    app = (args.bin_dir / "hitty-app.exe").resolve()
    executable = (args.bin_dir / "hitty.exe").resolve()
    evidence = ROOT / "docs/testing/images/otty-hidpi" / args.stage
    evidence.mkdir(parents=True, exist_ok=True)
    scratch = ROOT / ".hitty-dev-otty-hidpi"
    scratch.mkdir(exist_ok=True)
    sample = scratch / "sample.ps1"
    sample.write_text(
        "Clear-Host\n"
        "Write-Output 'HITTY THEME RESTORE'\n"
        "Write-Output ''\n"
        "Write-Output 'Configured terminal: Hack | 15 px | line height: 1.4'\n"
        "Write-Output 'Configured interface: 14 px | both sidebars visible'\n"
        "Write-Output ''\n"
        "Write-Output 'ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz'\n"
        "Write-Output '0123456789  Il1 O0  => -> != == [ ] { } ( )'\n"
        "Write-Output '中文字体检查：主题切换后保持原有字号和清晰度。'\n"
        "Write-Output ''\n"
        "Write-Output 'THEME_RESTORE_READY'\n",
        encoding="utf-8-sig",
    )
    profile = Path(tempfile.mkdtemp(prefix=f"hitty-otty-hidpi-{args.stage}-"))
    config = {
        "theme_preset": "one_light", "theme_follow_system": False,
        "theme_style_legible_palette": True, "theme_legible_palette": True,
        "window_blur": None, "window_backdrop": "auto", "window_opacity": None,
        "gui_language": "en", "font_family": "Hack", "font_size": 15,
        "line_height": 1.4, "ui_font_size": 14, "cursor_blink": False,
        "tab_bar_position": "left", "right_panel_visible": True,
        "right_panel_tab": "info", "check_for_updates": False,
        "auto_download_updates": False, "install_cli_on_path": False,
        "show_tray_icon": False,
        "shell": {"program": "powershell.exe", "args": [
            "-NoLogo", "-NoProfile", "-NoExit", "-ExecutionPolicy", "Bypass",
            "-Command", "Set-Location $env:SystemRoot; "
            "function global:prompt { 'PS typography-review> ' }; "
            + "& '" + str(sample).replace("'", "''") + "'",
        ]},
    }
    config_path = profile / "config.json"

    def save():
        temporary = profile / "config.tmp"
        temporary.write_text(json.dumps(config), encoding="utf-8")
        temporary.replace(config_path)

    save()
    (profile / "window.json").write_text(
        '{"x":80,"y":80,"width":1200,"height":800}', encoding="utf-8")
    env = {k: v for k, v in os.environ.items()
           if k != "HITTY" and not k.startswith("HITTY_")}
    env.pop("HC_SESSION", None)
    env.pop("HC_TERMINAL_ID", None)
    env.update(HITTY_CONFIG_DIR=str(profile), HITTY_DATA_DIR=str(profile / "data"))

    def cli(*command, check=True):
        result = subprocess.run([str(executable), *command], env=env,
                                capture_output=True, text=True, encoding="utf-8",
                                errors="replace", timeout=30,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        if check and result.returncode:
            raise RuntimeError(result.stderr or result.stdout)
        return result

    with (scratch / f"{args.stage}.log").open("wb") as log:
        process = subprocess.Popen([str(app), "--config-dir", str(profile)], env=env,
                                   cwd=os.environ["SystemRoot"], stdout=log, stderr=log,
                                   creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            def panes():
                if process.poll() is not None:
                    raise RuntimeError(f"Application exited: {process.returncode}")
                result = cli("pane", "ls", "--json", check=False)
                return json.loads(result.stdout)["panes"] if result.returncode == 0 else None

            pane = wait_for(panes, "isolated terminal")[0]
            address = "%" + str(pane["pane"])
            wait_for(lambda: "THEME_RESTORE_READY" in cli("capture", address, "--plain", "--scrollback").stdout,
                     "complete sample output")
            cli("ws", "rename", str(pane["workspace"]), "Typography verification")
            records = []
            entries = json.loads((ROOT / "assets/themes/otty/manifest.json").read_text(encoding="utf-8"))["themes"]
            ids = sorted(entry["id"] for entry in entries)
            modes = {entry["id"]: entry["mode"] for entry in entries}
            modes["one_light"] = "light"
            assert ids == ["otty-solarized-light"]
            scenes = []
            for layout in ["left", "top"]:
                scenes.append((f"{layout}-baseline", "one_light", layout))
                for theme in ids:
                    scenes.append((f"{layout}-{theme}", theme, layout))
                    if args.stage != "before":
                        scenes.append((f"{layout}-return-from-{theme}", "one_light", layout))
                scenes.append((f"{layout}-restored", "one_light", layout))
            for scene, theme, layout in scenes:
                config["tab_bar_position"] = layout
                config["theme_preset"] = theme
                save()
                # All return captures must match the baseline. Keep one copy
                # of their identical pixels and retain each measured hash.
                filename = f"{layout}-restored" if "-return-from-" in scene else scene
                image_path = evidence / f"{filename}.png"
                capture = subprocess.run([
                    "powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                    str(ROOT / "scripts/capture-native-window.ps1"), "-ProcId", str(process.pid),
                    "-Out", str(image_path), "-SettleMs", "1500",
                ], capture_output=True, text=True, check=True, timeout=30)
                text = cli("capture", address, "--plain", "--scrollback").stdout
                (evidence / f"{filename}.txt").write_text(text, encoding="utf-8", newline="\n")
                assert "THEME_RESTORE_READY" in text
                native = native_state(capture.stdout)
                assert bool(native["dark"]) == (modes[theme] == "dark"), scene
                records.append({"scene": scene, "selection": theme, "layout": layout,
                                "image": image_path.name, "native": native,
                                "capture": capture.stdout.strip(),
                                "image_sha256": hashlib.sha256(image_path.read_bytes()).hexdigest(),
                                "text_sha256": hashlib.sha256(text.encode()).hexdigest()})
                print(f"{args.stage}: {scene}: {capture.stdout.strip()}", flush=True)
            restored = {}
            native_restored = {}
            for layout in ["left", "top"]:
                baseline = next(r for r in records if r["scene"] == f"{layout}-baseline")
                returns = [r for r in records if r["layout"] == layout
                           and r["selection"] == "one_light" and r is not baseline]
                restored[layout] = all(baseline["image_sha256"] == r["image_sha256"] for r in returns)
                native_restored[layout] = all(baseline["native"] == r["native"] for r in returns)
                assert all(baseline["text_sha256"] == r["text_sha256"] for r in returns)
            metadata = {"stage": args.stage, "app": str(app),
                        "binary_sha256": hashlib.sha256(app.read_bytes()).hexdigest(),
                        "profile": str(profile), "configuration": config,
                        "logical_window": [1200, 800], "captures": records,
                        "terminal_output_preserved_after_round_trip": True,
                        "identical_pixels_after_round_trip": restored,
                        "identical_native_state_after_round_trip": native_restored,
                        "system_backdrop_query_supported": all(
                            r["native"]["system_backdrop_type"] is not None for r in records)}
            (evidence / "captures.json").write_text(
                json.dumps(metadata, indent=2) + "\n", encoding="utf-8", newline="\n")
            print(f"{args.stage}: restored UI pixels: {restored}", flush=True)
            assert all(restored.values()), "Theme switching changed Hitty typography"
            assert all(native_restored.values()), "Theme switching changed native appearance state"
        finally:
            try:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=30)
            finally:
                cli("server", "stop", check=False)


if __name__ == "__main__":
    main()
