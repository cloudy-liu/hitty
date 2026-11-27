"""Capture the Windows zero-session sidebar before and after upgrading."""

import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
wait_for = runpy.run_path(str(ROOT / "scripts/bootstrap-native-smoke.py"))["wait_for"]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--verify", action="store_true", help="verify interactive Windows clicks at 175%% display scaling")
parser.add_argument("--profile", type=Path, help="isolated profile printed by the before run")
parser.add_argument("--cli", required=True, type=Path)
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
cli = args.cli.resolve(strict=True)

fixtures = Path(tempfile.gettempdir()) / "hitty-unified-folder-fixture-92c1"
pinned = fixtures / "Pinned folder"
ordinary = fixtures / "Ordinary folder"
for folder in (pinned, ordinary):
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "example.txt").write_text("isolated folder fixture\n", encoding="utf-8")
profile = args.profile or Path(tempfile.mkdtemp(prefix="hitty-unified-folder-"))
marker = profile / "sidebar-empty-directories.fixture"
if args.profile is None:
    marker.touch()
elif not marker.is_file():
    parser.error("--profile must come from an earlier run of this script")
config = {
    "theme_preset": "one_light", "theme_follow_system": False, "gui_language": "en",
    "sidebar_grouping": "repo-or-directory", "tab_bar_position": "left",
    "sidebar_pinned_groups": [str(pinned)], "cursor_blink": False,
    "check_for_updates": False, "auto_download_updates": False,
    "install_cli_on_path": False, "show_tray_icon": False,
    "shell": {"program": os.environ["ComSpec"], "args": []},
}
if args.profile is None:
    (profile / "config.json").write_text(json.dumps(config), encoding="utf-8")
    (profile / "window.json").write_text('{"x":80,"y":80,"width":1100,"height":700}', encoding="utf-8")
env = {key: value for key, value in os.environ.items() if key != "HITTY" and not key.startswith("HITTY_")}
env.update(HITTY_CONFIG_DIR=str(profile), HITTY_DATA_DIR=str(profile / "data"))
env["PATH"] = os.pathsep.join([str(Path(os.environ["SystemRoot"]) / "System32"),
                              os.environ["SystemRoot"], str(Path(shutil.which("git.exe")).parent)])


def command(*arguments, check=True):
    return subprocess.run([str(cli), *arguments], env=env, check=check, text=True,
                          capture_output=True, timeout=20, encoding="utf-8", errors="replace",
                          creationflags=subprocess.CREATE_NO_WINDOW)


def panes():
    result = command("pane", "ls", "--json", check=False)
    return json.loads(result.stdout)["panes"] if result.returncode == 0 else None


def wait_for_prompt(folder):
    pane = wait_for(panes, f"terminal in {folder.name}")[0]
    address = "%" + str(pane["pane"])
    wait_for(lambda: str(folder) + ">" in command("capture", address, "--plain").stdout,
             f"prompt in {folder.name}")
    return str(pane["workspace"])


def tabs(workspace):
    return json.loads(command("tab", "ls", workspace, "--json").stdout)["tabs"]


def close_tab(workspace):
    tab = wait_for(lambda: tabs(workspace), "one terminal tab")[0]
    command("tab", "close", "@" + str(tab["ordinal"]))
    wait_for(lambda: tabs(workspace) == [] and panes() == [], "zero tabs and panes")


def capture(process, name, *options):
    result = subprocess.run([
        "powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
        str(ROOT / "scripts/capture-native-window.ps1"), "-ProcId", str(process.pid),
        "-Out", str(output / (name + ".png")), "-SettleMs", "900", *options,
    ], check=True, capture_output=True, text=True, timeout=30)
    print(result.stdout.strip(), flush=True)
    if args.verify and "client=1925x1225" not in result.stdout:
        raise RuntimeError("interactive verification requires a 1100×700 client at 175% scaling")


def start(log, *options):
    return subprocess.Popen([str(binary), "--config-dir", str(profile), *options], env=env,
                            cwd=ordinary, stdout=log, stderr=log, creationflags=subprocess.CREATE_NO_WINDOW)


with (profile / "app.log").open("wb") as log:
    process = start(log, "--open-path", str(ordinary))
    try:
        workspace = wait_for_prompt(ordinary)
        command("ws", "rename", workspace, "Folder retention")
        close_tab(workspace)
        capture(process, "zero-sessions")
        if args.verify:
            capture(process, "open-pinned", "-Click", "-MoveTo", "142,280")
            assert wait_for_prompt(pinned) == workspace
            close_tab(workspace)
            capture(process, "open-ordinary", "-Click", "-MoveTo", "142,405")
            assert wait_for_prompt(ordinary) == workspace
            close_tab(workspace)
            # Close the actual window before stopping the isolated daemon. The
            # cold launch must restore the saved workspace even with no tabs.
            subprocess.run(["powershell.exe", "-NoProfile", "-Command",
                            f"if (-not (Get-Process -Id {process.pid}).CloseMainWindow()) {{ exit 1 }}"],
                           check=True, timeout=10)
            process.wait(timeout=15)
            view = next(row for row in json.loads((profile / "views.json").read_text(encoding="utf-8"))["views"]
                        if row["id"] == workspace)
            assert set(map(Path, view["sidebar_folders"])) == {pinned, ordinary}
            assert not view["open"], view
            command("server", "stop")
            process = start(log)
            capture(process, "restarted")
            assert panes() == [] and tabs(workspace) == []
            restored = json.loads((profile / "views.json").read_text(encoding="utf-8"))
            assert restored["active"] == workspace
            assert next(row for row in restored["views"] if row["id"] == workspace)["open"]
            print("Both placeholders launched the configured terminal in their folders; cold restart restored zero sessions", flush=True)
        assert all((folder / "example.txt").is_file() for folder in (pinned, ordinary))
        print(f"Isolated profile: {profile}", flush=True)
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=15)
        command("server", "stop", check=False)
