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
    parser.add_argument("--pane-engine", choices=("native", "herdr-core"))
    parser.add_argument("--hc-bin", type=Path, help="hc executable for the optional herdr-core smoke")
    args = parser.parse_args()
    if args.pane_engine == "herdr-core" and args.hc_bin is None:
        parser.error("--pane-engine herdr-core requires --hc-bin for verification and cleanup")
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
    if args.pane_engine:
        preferences["pane_engine"] = args.pane_engine
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
    environment.pop("HC_SESSION", None)
    environment.pop("HC_TERMINAL_ID", None)
    if args.hc_bin:
        environment[prefix + "_HC_BIN"] = str(args.hc_bin.resolve())

    # Same stable profile owner as daemon::hc; no user's terminals are selected.
    owner = 0xCBF29CE484222325
    for byte in str(config).encode("utf-8"):
        owner = ((owner ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF

    def hc(*command):
        result = subprocess.run(
            [str(args.hc_bin.resolve()), *command], env=environment,
            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
        )
        if result.returncode:
            raise RuntimeError(result.stderr or result.stdout)
        return json.loads(result.stdout)

    def owned_terminals():
        rows = hc("ls", "--all", "--label", f"hitty.owner={owner:016x}", "--json")["terminals"]
        # `hc ls` is a compact view; labels are in the full `hc get` record.
        terminals = [hc("get", row["address"], "--json") for row in rows]
        return [t for t in terminals if t.get("labels", {}).get("hitty.owner") == f"{owner:016x}"]

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
            cli("wait", target, "--until", "free", "--timeout", "10", "--json")
            hc_address = None
            if args.pane_engine == "herdr-core":
                terminals = owned_terminals()
                terminal = next((t for t in terminals if t["labels"].get("hitty.pane") == target), None)
                if terminal is None:
                    raise AssertionError(f"No hc terminal for {target}; owner={owner:016x}; records={terminals}")
                hc_address = terminal["address"]
                identity_command = (
                    "Write-Output ('ENGINE_ID=' + $env:HC_TERMINAL_ID + ';PANE_ID=' + $env:HITTY_PANE)"
                    if os.name == "nt" else
                    "printf 'ENGINE_ID=%s;PANE_ID=%s\\n' \"$HC_TERMINAL_ID\" \"$HITTY_PANE\""
                )
                cli("send", target, identity_command, "--enter")
                expected = f"ENGINE_ID={hc_address};PANE_ID={pane['pane']}"
                text = wait_for(lambda: (value if expected in value else None)
                    if (value := cli("capture", target, "--plain").stdout) else None, "engine identity")
                (output / f"{args.phase}-engine-identity.txt").write_text(text, encoding="utf-8", newline="\n")
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
                if hc_address and not any(t["address"] == hc_address for t in owned_terminals()):
                    raise AssertionError("GUI reconnect replaced the hc terminal")
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
                "pane_engine": args.pane_engine or "native", "hc_terminal": hc_address,
                "cli_wait_free": "passed",
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
            if args.pane_engine == "herdr-core":
                for terminal in owned_terminals():
                    hc("kill", terminal["address"], "--json")
                    hc("rm", terminal["address"], "--json")
                if owned_terminals():
                    raise AssertionError("smoke left a test-owned hc terminal")


if __name__ == "__main__":
    main()
