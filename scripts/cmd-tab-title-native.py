"""Capture CMD's automatic tab title with an isolated Windows profile."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
wait_for = runpy.run_path(str(ROOT / "scripts/bootstrap-native-smoke.py"))["wait_for"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", choices=("before", "after"), required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    args = parser.parse_args()
    app = (args.bin_dir / "hitty-app.exe").resolve()
    executable = (args.bin_dir / "hitty.exe").resolve()
    output = ROOT / "docs/testing/images/cmd-tab-title" / args.stage
    output.mkdir(parents=True, exist_ok=True)
    profile = Path(tempfile.mkdtemp(prefix="hitty-cmd-title-"))
    config = {
        "theme_preset": "one_light", "theme_follow_system": False,
        "gui_language": "en", "font_size": 15, "ui_font_size": 14,
        "cursor_blink": False, "sidebar_grouping": "none",
        "check_for_updates": False, "auto_download_updates": False,
        "install_cli_on_path": False, "show_tray_icon": False,
        "shell": {"program": os.environ["ComSpec"], "args": []},
    }

    def save_config():
        temporary = profile / "config.tmp"
        temporary.write_text(json.dumps(config), encoding="utf-8")
        temporary.replace(profile / "config.json")

    save_config()
    (profile / "window.json").write_text(
        '{"x":80,"y":80,"width":1100,"height":700}', encoding="utf-8")
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
            cli("ws", "rename", workspace, "Shell titles")

            def tab():
                return json.loads(cli("tab", "ls", workspace, "--json").stdout)["tabs"][0]

            original = wait_for(lambda: (row if "cmd" in row["label"].lower() else None)
                                if (row := tab()) else None, "CMD startup title")
            cli("send", address, "cls", "--enter")
            wait_for(lambda: os.environ["SystemRoot"] in cli("capture", address, "--plain").stdout,
                     "CMD prompt")
            captures = []
            for layout in ("left", "top"):
                config["tab_bar_position"] = layout
                save_config()
                capture = subprocess.run([
                    "powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                    str(ROOT / "scripts/capture-native-window.ps1"), "-ProcId", str(process.pid),
                    "-Out", str(output / f"{layout}.png"), "-SettleMs", "1800",
                ], check=True, capture_output=True, text=True, timeout=30)
                captures.append({"layout": layout, "capture": capture.stdout.strip()})

            if args.stage == "after":
                assert original["label"] == "CMD", original
                for title, expected in (
                    ("cmd.exe", "CMD"),
                    ("C:/Windows/System32/CMD.EXE", "CMD"),
                    ("PowerShell", "PowerShell"),
                    ("pwsh.exe", "pwsh.exe"),
                    (r"C:\Windows\System32\cmd.exe - build", r"C:\Windows\System32\cmd.exe - build"),
                    (r"Editing C:\Windows\cmd.exe", r"Editing C:\Windows\cmd.exe"),
                ):
                    cli("send", address, "title " + title, "--enter")
                    wait_for(lambda: tab()["label"] == expected, "title: " + title)
            else:
                assert original["label"] != "CMD" and "cmd.exe" in original["label"].lower(), original

            cli("send", address, "title Build API", "--enter")
            wait_for(lambda: tab()["label"] == "Build API", "custom CMD title")
            cli("tab", "rename", "@" + str(original["ordinal"]), "My command prompt")
            wait_for(lambda: tab()["label"] == "My command prompt", "manual tab name")
            metadata = {
                "stage": args.stage, "app": str(app),
                "binary_sha256": hashlib.sha256(app.read_bytes()).hexdigest(),
                "profile": str(profile), "configuration": config,
                "logical_window": [1100, 700], "startup_tab": original,
                "captures": captures, "custom_title": "passed", "manual_name": "passed",
            }
            (output / "captures.json").write_text(
                json.dumps(metadata, indent=2) + "\n", encoding="utf-8", newline="\n")
            print(json.dumps(metadata), flush=True)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=15)
            cli("server", "stop", check=False)


if __name__ == "__main__":
    main()
