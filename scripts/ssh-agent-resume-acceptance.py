"""Run the production app against an isolated loopback SSH/agent fixture.

Install paramiko into target/ssh-evidence-python, then pass --binary, --phase
and --output. Capture the resulting native window with LCU. Touch disconnect
or stop in the phase's output directory to disconnect SSH or finish the run.
No account credentials, external hosts, or real agent services are used.
"""

import argparse
import json
import os
import pathlib
import shlex
import socket
import subprocess
import sys
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "target/ssh-evidence-python"))
import paramiko

SESSION = "0199c3f2-1b0e-7c3a-9f21-6d4b8e2a5c17"
WORKSPACE = "ca9c78d3-86aa-4e88-9a7d-43bbf5df1e65"
TAB = "f7c78b51-fdb5-45c1-b90b-4241184e1bf2"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--phase", choices=["before", "after"], required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--port", type=int, default=22471)
    args = parser.parse_args()
    output = (args.output / args.phase).resolve()
    output.mkdir(parents=True, exist_ok=True)
    config = output / "config"
    config.mkdir(exist_ok=True)
    binaries = config / "bin"
    binaries.mkdir(exist_ok=True)
    if (config / "daemon.pid").exists():
        raise RuntimeError("Use a fresh output directory for each run")
    events = []
    channels = []
    event_lock = threading.Lock()

    def record(**event):
        with event_lock:
            events.append(event)
            (output / "ssh-events.json").write_text(json.dumps(events, indent=2), encoding="utf8")

    class Server(paramiko.ServerInterface):
        def check_auth_none(self, username):
            return paramiko.AUTH_SUCCESSFUL

        def get_allowed_auths(self, username):
            return "none"

        def check_channel_request(self, kind, channel_id):
            return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

        def check_channel_pty_request(self, channel, term, width, height, *rest):
            return True

        def check_channel_window_change_request(self, *args):
            return True

        def shell(self, channel, cwd):
            time.sleep(0.15)
            channels.append(channel)
            record(event="shell-ready", cwd=cwd)
            channel.sendall(
                f"\x1b]2;SSH agent restore acceptance\x07\x1b]7;file://fixture{cwd}\x07"
                f"SSH loopback fixture / shell ready\r\nDirectory: {cwd}\r\n"
                "\x1b]133;A\x07tester@fixture $ \x1b]133;B\x07"
            )
            while not channel.closed:
                data = channel.recv(32768)
                if not data:
                    break
                command = data.decode("utf8", errors="replace").strip()
                if command.startswith("codex resume ") or command.startswith("claude "):
                    record(event="agent-resume", cwd=cwd, command=command)
                    channel.sendall(
                        "\r\n\x1b[32mAGENT FIXTURE: exact conversation resumed\x1b[0m\r\n"
                        f"Session: {SESSION}\r\nCommand: {command}\r\nDirectory: {cwd}\r\n"
                        "\r\nWaiting for input (deterministic agent fixture).\r\n"
                    )

        def check_channel_shell_request(self, channel):
            threading.Thread(target=self.shell, args=(channel, "/home/tester"), daemon=True).start()
            return True

        def check_channel_exec_request(self, channel, command):
            script = command.decode("utf8")
            if script == "echo __tty7_shell; echo $SHELL":
                def probe():
                    time.sleep(0.02)
                    channel.sendall("__tty7_shell\n/bin/bash\n")
                    channel.send_exit_status(0)
                    channel.close()
                threading.Thread(target=probe, daemon=True).start()
            else:
                cwd = "/home/tester"
                first = script.splitlines()[0]
                if first.startswith("builtin cd -- "):
                    cwd = shlex.split(first)[3]
                threading.Thread(target=self.shell, args=(channel, cwd), daemon=True).start()
            return True

    listener = socket.socket()
    listener.bind(("127.0.0.1", args.port))
    listener.listen()
    key = paramiko.RSAKey.generate(2048)
    transports = []

    def accept():
        while True:
            try:
                connection, _ = listener.accept()
            except OSError:
                return
            transport = paramiko.Transport(connection)
            transports.append(transport)
            transport.add_server_key(key)
            transport.start_server(server=Server())
            def retain_channels(t=transport):
                while t.is_active():
                    channel = t.accept(1)
                    if channel is not None:
                        channels.append(channel)
            threading.Thread(target=retain_channels, daemon=True).start()
    threading.Thread(target=accept, daemon=True).start()

    spec = dict(host="127.0.0.1", port=args.port, user="tester", auth_mode="auto",
                verify_host_keys=False, shell_integration=True, display_name="SSH acceptance fixture")
    machine = dict(workspaces=[dict(id=WORKSPACE, name="SSH restore acceptance", tabs=[
        dict(id=TAB, root={"Leaf": dict(pane=42)})], active_tab=TAB)], panes=[
        dict(id=42, cwd="/work/project", ssh_spec=spec, live=False, agent=dict(
            agent="Codex", session_id=SESSION, launch_argv=["codex", "--yolo"], status="idle"))])
    geometry = dict(x=80, y=80, width=1200, height=700)
    files = {
        "machine.json": machine,
        "views.json": dict(active=WORKSPACE, views=[dict(id=WORKSPACE, open=True, window=geometry)]),
        "window.json": geometry,
        "config.json": dict(restore_session=True, restore_agent_sessions=True,
                            check_for_updates=False, install_cli_on_path=False,
                            show_tray_icon=False, font_size=15,
                            env={"PATH": str(binaries) + os.pathsep + os.environ["PATH"]},
                            theme="dark", gui_language="en", shell=dict(program="cmd.exe", args=["/k"])),
    }
    for name, value in files.items():
        (config / name).write_text(json.dumps(value), encoding="utf8")
    (binaries / "codex.cmd").write_text("@echo off\necho AGENT FIXTURE: command reached LOCAL shell\necho %*\n", encoding="utf8")
    env = dict(os.environ, PATH=str(binaries) + os.pathsep + os.environ["PATH"])
    flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    log = (output / "app-output.log").open("w", encoding="utf8")
    process = subprocess.Popen([str(args.binary.resolve()), "--config-dir", str(config)],
                               env=env, stdout=log, stderr=log, creationflags=flags)
    print(json.dumps(dict(app_pid=process.pid, config=str(config), phase=args.phase)), flush=True)
    try:
        while process.poll() is None and not (output / "stop").exists():
            if (output / "reopen").exists():
                (output / "reopen").unlink()
                process.terminate()
                process.wait(timeout=10)
                process = subprocess.Popen(
                    [str(args.binary.resolve()), "--config-dir", str(config)],
                    env=env, stdout=log, stderr=log, creationflags=flags,
                )
                record(event="gui-reopened", app_pid=process.pid)
            if (output / "disconnect").exists():
                (output / "disconnect").unlink()
                for channel in list(channels):
                    channel.close()
                record(event="disconnected")
            time.sleep(0.1)
    finally:
        subprocess.run([str(args.binary.resolve()), "--config-dir", str(config), "--stop-daemon"],
                       stdout=log, stderr=log, creationflags=flags, timeout=20)
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        for transport in transports:
            transport.close()
        listener.close()
        log.close()


if __name__ == "__main__":
    main()
