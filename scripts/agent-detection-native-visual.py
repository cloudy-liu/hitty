"""Capture Qoder CLI and CodeBuddy detection in a live ctty7 window on Windows.

Builds nothing. Runs an already built tty7-app against a throwaway config
directory, so the machine's own server, settings and hooks stay untouched, and
renders the window with scripts/capture-native-window.ps1 (PrintWindow: no
pointer, no overlapping windows). Debug builds also skip the hook refresh and
the PATH install at launch.

The sidebar scene opens three tabs: the startup shell, a stand-in process named
codebuddy.exe (a copy of timeout.exe, so CodeBuddy itself is not needed), and a
real `qodercli` started in --qoder-cwd. Pick a directory Qoder already trusts,
or its trust dialog replaces the composer. No prompt is ever sent to Qoder.
The settings scene opens Settings -> Agents and scrolls to its last hook rows,
which read the machine's real agent configs without writing to them.

    python scripts/agent-detection-native-visual.py target/debug \\
        docs/testing/images/qoder-codebuddy-agents --phase after \\
        --qoder-cwd D:/work/repo --scale 1.75
"""

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
CAPTURE = ROOT / "scripts/capture-native-window.ps1"
WHEEL = ROOT / "scripts/post-mouse-wheel.ps1"
# Logical points in the default 1440x900 window.
NEUTRAL = {  # where the synthetic pointer rests, away from any hover target
    "sidebar": (914, 628),  # terminal body
    "settings": (150, 800),  # empty settings navigation, below its last entry
}
SETTINGS_GEAR = (1322, 14)
SETTINGS_AGENTS = (66, 277)
SETTINGS_SCROLL = (857, 571, 15)  # point in the content column, wheel notches down
SCENES = [  # (name, theme, kind)
    ("sidebar-dark", "dark", "sidebar"),
    ("sidebar-light", "light", "sidebar"),
    ("settings-dark", "dark", "settings"),
]


class Instance:
    """One tty7-app, its own server, and a CLI pointed at that server."""

    def __init__(self, target, theme, cwd, log):
        self.cli_path = target / "tty7.exe"
        self.config = pathlib.Path(tempfile.mkdtemp(prefix="ctty7-agent-detection-visual-"))
        (self.config / "config.json").write_text(
            json.dumps({"theme": theme, "theme_preset": theme}), encoding="utf-8")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("TTY7")}
        self.env["TTY7_CONFIG_DIR"] = str(self.config)
        self.app = subprocess.Popen([str(target / "tty7-app.exe"), "--config-dir", str(self.config)],
                                    env=self.env, cwd=cwd, stdout=log, stderr=log,
                                    creationflags=subprocess.DETACHED_PROCESS)

    def cli(self, *args, check=True):
        result = subprocess.run([str(self.cli_path), *args], env=self.env, capture_output=True,
                                text=True, encoding="utf-8", errors="replace", timeout=30)
        if check and result.returncode:
            raise RuntimeError(f"tty7 {' '.join(args)}: {result.stdout}{result.stderr}")
        return result.stdout

    def panes(self):
        return json.loads(self.cli("pane", "ls", "--json"))["panes"]

    def new_tab(self, workspace, cwd):
        known = {p["pane"] for p in self.panes()}
        self.cli("tab", "new", workspace, "--cwd", str(cwd))
        pane = wait_for(lambda: [p for p in self.panes() if p["pane"] not in known], "new tab")[0]
        time.sleep(2)  # let the shell print its prompt before typing
        return pane["pane"]

    def close(self):
        # The window restarts a server that vanishes under it, so it goes first.
        self.app.kill()
        self.app.wait(timeout=20)
        self.cli("server", "stop", check=False)
        shutil.rmtree(self.config, ignore_errors=True)


def wait_for(probe, what, seconds=60):
    deadline = time.monotonic() + seconds
    while True:
        try:
            value = probe()
            if value:
                return value
        except Exception:
            pass
        if time.monotonic() > deadline:
            raise TimeoutError(what)
        time.sleep(0.5)


def powershell(script, *args):
    result = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                             str(script), *map(str, args)], check=True, capture_output=True, text=True)
    return result.stdout.strip()


def point(xy, scale):
    return f"{int(xy[0] * scale)},{int(xy[1] * scale)}"


def sidebar(instance, standin, qoder_cwd):
    first = wait_for(instance.panes, "startup pane")[0]
    workspace = first["workspace"]
    instance.cli("ws", "rename", workspace, "agents")
    buddy = instance.new_tab(workspace, standin.parent)
    instance.cli("send", f"%{buddy}", f"{standin} /t 3600", "--enter")
    qoder = instance.new_tab(workspace, qoder_cwd)
    instance.cli("send", f"%{qoder}", "qodercli", "--enter")
    # Qoder draws its composer within a few seconds; the window probes the
    # process tree and reads the screen on its own cadence after that.
    time.sleep(15)
    print(instance.cli("agents", check=False).strip(), flush=True)
    return qoder


def capture_scene(target, output, phase, scene, scale, standin, qoder_cwd):
    name, theme, kind = scene
    png = output / f"{phase}-{name}.png"
    with (output / f"{phase}-{name}.log").open("w", encoding="utf-8") as log:
        instance = Instance(target, theme, qoder_cwd, log)
        qoder = None
        try:
            pid = instance.app.pid
            if kind == "sidebar":
                qoder = sidebar(instance, standin, qoder_cwd)
            else:
                wait_for(instance.panes, "startup pane")
                time.sleep(3)
            subprocess.run(["powershell", "-NoProfile", "-Command",
                            f"(New-Object -ComObject WScript.Shell).AppActivate({pid})"],
                           check=False, capture_output=True)
            time.sleep(1)
            if kind == "settings":
                for xy in (SETTINGS_GEAR, SETTINGS_AGENTS):
                    powershell(CAPTURE, "-ProcId", pid, "-Out", png, "-MoveTo", point(xy, scale),
                               "-Click", "-SettleMs", 1500)
                x, y, notches = SETTINGS_SCROLL
                powershell(WHEEL, "-ProcId", pid, "-X", int(x * scale), "-Y", int(y * scale),
                           "-Notches", notches)
                time.sleep(1)
            print(f"  {name}: " + powershell(CAPTURE, "-ProcId", pid, "-Out", png, "-MoveTo",
                                             point(NEUTRAL[kind], scale), "-SettleMs", 1500), flush=True)
        finally:
            if qoder is not None:
                instance.cli("send", f"%{qoder}", "--key", "C-c", check=False)
            instance.close()
    return png


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("target", type=pathlib.Path, help="directory holding tty7-app.exe and tty7.exe")
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--phase", choices=("before", "after"), required=True)
    parser.add_argument("--qoder-cwd", type=pathlib.Path, required=True,
                        help="a directory Qoder already trusts")
    parser.add_argument("--scale", type=float, required=True, help="display scale, e.g. 1.75")
    parser.add_argument("--only", nargs="*", help="scene names to capture; default all")
    args = parser.parse_args()
    target = args.target.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    # A fixed directory, so the stand-in's sidebar row reads the same in both
    # phases. Its path must not contain spaces: it is typed into the shell.
    standin = pathlib.Path(tempfile.gettempdir()) / "ctty7-agent-detection" / "codebuddy.exe"
    standin.parent.mkdir(exist_ok=True)
    shutil.copyfile(pathlib.Path(os.environ["SystemRoot"]) / "System32/timeout.exe", standin)
    for scene in SCENES:
        if not args.only or scene[0] in args.only:
            capture_scene(target, output, args.phase, scene, args.scale, standin,
                          args.qoder_cwd.resolve(strict=True))


if __name__ == "__main__":
    sys.exit(main())
