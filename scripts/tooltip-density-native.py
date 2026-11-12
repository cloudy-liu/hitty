"""Capture hover tooltips with an isolated Windows profile."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CAPTURE = ROOT / "scripts/capture-native-window.ps1"
wait_for = runpy.run_path(str(ROOT / "scripts/bootstrap-native-smoke.py"))["wait_for"]

WINDOW = (1100, 700)
TAB_NAME = "A tab name long enough that the sidebar truncates it"
# Logical points in that window. `hover` is where the synthetic pointer rests;
# `crop` is the x, y, width, height kept around the tooltip it raises.
NEUTRAL = (700, 420)  # terminal body, away from every hover target
# One launch per entry: the keybinding overrides its profile carries, then the
# scenes captured in it.
PASSES = [
    ({}, {
        "hide-sidebar": {"hover": (201, 15), "crop": (40, 0, 320, 116)},
        "new-tab": {"hover": (171, 15), "crop": (40, 0, 320, 116)},
        "settings": {"hover": (982, 15), "crop": (780, 0, 320, 116)},
        # A plain text tooltip: the control has no chord to show.
        "close-tab": {"hover": (198, 131), "crop": (40, 52, 320, 116)},
        # Anchored to the pointer rather than to a control. Only a tab the
        # sidebar has to truncate carries the card, and on such a tab the card
        # also opens over the row's own ×, so the rename waits for this scene.
        "tab-card": {"hover": (100, 131), "crop": (60, 100, 460, 130), "tab_name": TAB_NAME},
    }),
    # The same tile path as `hide-sidebar`, with its action left unbound.
    ({"ToggleSwitcher": ""}, {
        "switch-workspace-unbound": {"hover": (120, 60), "crop": (0, 30, 320, 130)},
    }),
]


def powershell(*command):
    result = subprocess.run(["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", *command],
                            check=True, capture_output=True, text=True, timeout=30)
    return result.stdout.strip()


def crop(source, target, box):
    x, y, width, height = box
    powershell("-Command", (
        "Add-Type -AssemblyName System.Drawing; "
        f"$image = [System.Drawing.Bitmap]::FromFile('{source}'); "
        f"$box = New-Object System.Drawing.Rectangle {x}, {y}, {width}, {height}; "
        "$part = $image.Clone($box, $image.PixelFormat); $image.Dispose(); "
        f"$part.Save('{target}', [System.Drawing.Imaging.ImageFormat]::Png); $part.Dispose()"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", choices=("before", "after"), required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--scale", type=float, required=True, help="display scale, e.g. 1.75")
    args = parser.parse_args()
    app = (args.bin_dir / "hitty-app.exe").resolve()
    executable = (args.bin_dir / "hitty.exe").resolve()
    output = ROOT / "docs/testing/images/tooltip-density" / args.stage
    output.mkdir(parents=True, exist_ok=True)
    config = {
        "theme_preset": "one_light", "theme_follow_system": False,
        "gui_language": "en", "font_size": 15, "ui_font_size": 16,
        "cursor_blink": False, "sidebar_grouping": "none", "tab_bar_position": "left",
        "check_for_updates": False, "auto_download_updates": False,
        "install_cli_on_path": False, "show_tray_icon": False,
        "shell": {"program": os.environ["ComSpec"], "args": []},
    }

    def physical(values):
        return [round(value * args.scale) for value in values]

    def capture_pass(keybindings, scenes):
        profile = Path(tempfile.mkdtemp(prefix="hitty-tooltip-density-"))
        (profile / "config.json").write_text(
            json.dumps({**config, "keybindings": keybindings}), encoding="utf-8")
        (profile / "window.json").write_text(
            json.dumps({"x": 80, "y": 80, "width": WINDOW[0], "height": WINDOW[1]}),
            encoding="utf-8")
        env = {k: v for k, v in os.environ.items() if k != "HITTY" and not k.startswith("HITTY_")}
        env.update(HITTY_CONFIG_DIR=str(profile), HITTY_DATA_DIR=str(profile / "data"))

        def cli(*command, check=True):
            result = subprocess.run([str(executable), *command], env=env, capture_output=True,
                                    text=True, encoding="utf-8", errors="replace", timeout=30,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
            if check and result.returncode:
                raise RuntimeError(result.stderr or result.stdout)
            return result

        with (profile / "app.log").open("wb") as log:
            process = subprocess.Popen([str(app), "--config-dir", str(profile)], env=env,
                                       cwd=os.environ["SystemRoot"], stdout=log, stderr=log,
                                       creationflags=subprocess.CREATE_NO_WINDOW)
            try:
                def panes():
                    if process.poll() is not None:
                        raise RuntimeError(f"Application exited: {process.returncode}")
                    result = cli("pane", "ls", "--json", check=False)
                    return json.loads(result.stdout)["panes"] if result.returncode == 0 else None

                pane = wait_for(panes, "CMD pane")[0]
                workspace = str(pane["workspace"])
                address = "%" + str(pane["pane"])
                cli("ws", "rename", workspace, "Tooltips")

                def tab():
                    return json.loads(cli("tab", "ls", workspace, "--json").stdout)["tabs"][0]

                cli("send", address, "cls", "--enter")
                wait_for(lambda: os.environ["SystemRoot"] in cli("capture", address, "--plain").stdout,
                         "CMD prompt")

                def shot(target, point, settle):
                    return powershell("-File", str(CAPTURE), "-ProcId", str(process.pid),
                                      "-Out", str(target),
                                      "-MoveTo", ",".join(map(str, physical(point))),
                                      "-SettleMs", str(settle))

                captures = []
                for name, scene in scenes.items():
                    if "tab_name" in scene:
                        cli("tab", "rename", "@" + str(tab()["ordinal"]), scene["tab_name"])
                        wait_for(lambda: tab()["label"] == scene["tab_name"], "tab name")
                    # Leave the last control long enough for its tooltip's grace
                    # period to lapse, so each one opens from rest instead of
                    # sliding over.
                    shot(profile / f"{name}-rest.png", NEUTRAL, 900)
                    window = output / f"{name}-window.png"
                    report = shot(window, scene["hover"], 1500)
                    crop(window, output / f"{name}.png", physical(scene["crop"]))
                    captures.append({"scene": name, "hover": scene["hover"],
                                     "keybindings": keybindings, "profile": str(profile),
                                     "capture": report})
                return captures
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=15)
                cli("server", "stop", check=False)

    captures = [capture for keybindings, scenes in PASSES
                for capture in capture_pass(keybindings, scenes)]
    metadata = {
        "stage": args.stage, "app": str(app),
        "binary_sha256": hashlib.sha256(app.read_bytes()).hexdigest(),
        "configuration": config, "tab_name": TAB_NAME,
        "logical_window": list(WINDOW), "display_scale": args.scale,
        "captures": captures,
    }
    (output / "captures.json").write_text(
        json.dumps(metadata, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(metadata), flush=True)


if __name__ == "__main__":
    main()
