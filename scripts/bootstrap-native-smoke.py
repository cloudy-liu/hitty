"""Exercise a built Windows app and CLI with isolated data and capture its window."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


def wait_for(probe, description):
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        result = probe()
        if result:
            return result
        time.sleep(0.25)
    raise TimeoutError(description)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("cli", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--identity", default="hitty")
    parser.add_argument("--phase", choices=("before", "after"), required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    config = Path(tempfile.mkdtemp(prefix="hitty-bootstrap-smoke-"))
    preferences = {
        "check_for_updates": False,
        "auto_download_updates": False,
        "install_cli_on_path": False,
        "show_tray_icon": False,
        "shell": {"program": "powershell.exe", "args": ["-NoLogo", "-NoProfile"]},
    }
    (config / "config.json").write_text(json.dumps(preferences), encoding="utf-8", newline="\n")
    (config / "window.json").write_text(
        json.dumps({"x": 40, "y": 40, "width": 1100, "height": 700}), encoding="utf-8", newline="\n"
    )
    environment = os.environ.copy()
    prefix = args.identity.upper()
    for key in list(environment):
        if key == prefix or key.startswith(prefix + "_"):
            del environment[key]
    environment[prefix + "_CONFIG_DIR"] = str(config)
    environment[prefix + "_DATA_DIR"] = str(config)

    def cli(*command, check=True):
        result = subprocess.run(
            [str(args.cli.resolve()), *command], env=environment,
            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30,
        )
        if check and result.returncode:
            raise RuntimeError(result.stderr or result.stdout)
        return result

    with (output / f"{args.phase}-application.log").open("wb") as log:
        process = subprocess.Popen(
            [str(args.app.resolve()), "--config-dir", str(config)],
            env=environment, cwd=config, stdout=log, stderr=log,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        try:
            def panes():
                if process.poll() is not None:
                    raise RuntimeError(f"Application exited: {process.returncode}")
                result = cli("pane", "ls", "--json", check=False)
                return json.loads(result.stdout)["panes"] if result.returncode == 0 else None

            pane = wait_for(panes, "first terminal pane")[0]
            target = "%" + str(pane["pane"])
            wait_for(lambda: "PS " in cli("capture", target, "--plain").stdout, "shell prompt")
            cli("ws", "rename", str(pane["workspace"]), "Bootstrap verification")
            cli("send", target, "Write-Output ('BOOTSTRAP_' + 'OK')", "--enter")
            text = wait_for(
                lambda: (value if "BOOTSTRAP_OK" in value else None)
                if (value := cli("capture", target, "--plain").stdout) else None,
                "shell output through the public CLI",
            )
            (output / f"{args.phase}-terminal.txt").write_text(text, encoding="utf-8", newline="\n")
            capture = subprocess.run(
                ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                 str(root / "scripts/capture-native-window.ps1"), "-ProcId", str(process.pid),
                 "-Out", str(output / f"{args.phase}-application.png"), "-SettleMs", "1500"],
                check=True, capture_output=True, text=True, timeout=30,
            )
            metadata = {
                "phase": args.phase, "binary": str(args.app.resolve()),
                "window_logical_size": [1100, 700], "capture": capture.stdout.strip(),
                "cli_version": cli("--version").stdout.strip(),
                "terminal_input_output": "passed", "configuration": str(config),
                "test_preferences": preferences,
            }
            (output / f"{args.phase}-metadata.json").write_text(
                json.dumps(metadata, indent=2), encoding="utf-8", newline="\n"
            )
            print(json.dumps(metadata), flush=True)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=15)
            cli("server", "stop", check=False)


if __name__ == "__main__":
    main()
