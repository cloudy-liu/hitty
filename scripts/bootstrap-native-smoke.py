"""Exercise an app and CLI with isolated data; optionally capture on Windows."""

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
    parser.add_argument("--no-capture", action="store_true")
    parser.add_argument("--reconnect", action="store_true", help="also restart the GUI and reuse its pane")
    args = parser.parse_args()
    if os.name != "nt" and not args.no_capture:
        parser.error("use --no-capture outside Windows; native visual CI has separate captures")
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    config = Path(tempfile.mkdtemp(prefix="hitty-bootstrap-smoke-"))
    preferences = {
        "check_for_updates": False,
        "auto_download_updates": False,
        "install_cli_on_path": False,
        "show_tray_icon": False,
        "shell": (
            {"program": "powershell.exe", "args": ["-NoLogo", "-NoProfile"]}
            if os.name == "nt" else
            {"program": "/bin/bash", "args": ["--noprofile", "--norc"]}
        ),
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

    def launch(log):
        return subprocess.Popen(
            [str(args.app.resolve()), "--config-dir", str(config)],
            env=environment, cwd=config, stdout=log, stderr=log,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
        )

    with (output / f"{args.phase}-application.log").open("wb") as log:
        process = launch(log)
        try:
            def panes():
                if process.poll() is not None:
                    raise RuntimeError(f"Application exited: {process.returncode}")
                result = cli("pane", "ls", "--json", check=False)
                return json.loads(result.stdout)["panes"] if result.returncode == 0 else None

            pane = wait_for(panes, "first terminal pane")[0]
            target = "%" + str(pane["pane"])
            wait_for(lambda: cli("capture", target, "--plain").stdout.strip(), "shell prompt")
            cli("ws", "rename", str(pane["workspace"]), "Bootstrap verification")
            command = (
                "Write-Output ('BOOTSTRAP_' + 'OK')" if os.name == "nt"
                else "printf '%s%s\\n' 'BOOTSTRAP_' 'OK'"
            )
            cli("send", target, command, "--enter")
            text = wait_for(
                lambda: (value if "BOOTSTRAP_OK" in value else None)
                if (value := cli("capture", target, "--plain").stdout) else None,
                "shell output through the public CLI",
            )
            (output / f"{args.phase}-terminal.txt").write_text(text, encoding="utf-8", newline="\n")
            if args.reconnect:
                process.terminate()
                process.wait(timeout=15)
                process = launch(log)
                restored = wait_for(panes, "pane after GUI restart")
                if not any(p["pane"] == pane["pane"] and p["workspace"] == pane["workspace"] for p in restored):
                    raise AssertionError("GUI restart did not retain the existing pane and workspace")
                cli("send", target, command.replace("BOOTSTRAP_", "RECONNECT_"), "--enter")
                text = wait_for(
                    lambda: (value if "RECONNECT_OK" in value else None)
                    if (value := cli("capture", target, "--plain").stdout) else None,
                    "shell output after GUI restart",
                )
                (output / f"{args.phase}-reconnected-terminal.txt").write_text(
                    text, encoding="utf-8", newline="\n"
                )
            capture_result = None
            if not args.no_capture:
                capture = subprocess.run(
                    ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                     str(root / "scripts/capture-native-window.ps1"), "-ProcId", str(process.pid),
                     "-Out", str(output / f"{args.phase}-application.png"), "-SettleMs", "1500"],
                    check=True, capture_output=True, text=True, timeout=30,
                )
                capture_result = capture.stdout.strip()
            metadata = {
                "phase": args.phase, "binary": str(args.app.resolve()),
                "window_logical_size": [1100, 700], "capture": capture_result,
                "cli_version": cli("--version").stdout.strip(),
                "terminal_input_output": "passed", "configuration": str(config),
                "session_after_gui_restart": "passed" if args.reconnect else "not exercised",
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
