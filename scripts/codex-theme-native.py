"""Check real Codex theme switches with a local model fixture (requires Pillow)."""

import argparse
import ctypes
from ctypes import wintypes
from collections import Counter
import hashlib
import http.server
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import threading

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
wait_for = runpy.run_path(str(ROOT / "scripts/bootstrap-native-smoke.py"))["wait_for"]
PROMPT = "/plan Write a short plan to compare background colors."
DRAFT = "Compare the background colors"
PLAN = "Inspect the input, transcript, and plan backgrounds."
THEMES = {"one_light": 0xFAFAFA, "one_dark_pro": 0x282C34}


def surfaces(path, canvas):
    """Measure the three full-width fills away from text and window chrome."""
    with Image.open(path) as source:
        image = source.convert("RGB")
    rows = [Counter(image.getpixel((x, y)) for x in range(
        int(image.width * .80), int(image.width * .95), 5
    )).most_common(1)[0][0] for y in range(image.height)]
    bands, start = [], 0
    for y in range(1, len(rows) + 1):
        if y < len(rows) and rows[y] == rows[start]:
            continue
        color = "#%02X%02X%02X" % rows[start]
        if y - start >= image.width / 60 and color != f"#{canvas:06X}":
            bands.append(dict(color=color, from_y=start, to_y=y - 1))
        start = y
    assert len(bands) == 3, (path, bands)
    return dict(zip(("prompt_history", "proposed_plan", "composer"), bands))


def fills(scene):
    return {name: value["color"] for name, value in scene["surfaces"].items()}


class Responses(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def respond(self, content_type, payload):
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        models = {"object": "list", "data": [{"id": "fixture", "object": "model", "owned_by": "local"}]}
        self.respond("application/json", json.dumps(models).encode())

    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        output = f"<proposed_plan>\n# Compare Backgrounds\n\n- {PLAN}\n</proposed_plan>\n"
        part = dict(type="output_text", text=output, annotations=[])
        item = dict(id="msg_theme", type="message", role="assistant", content=[part], status="completed")
        response = dict(id="resp_theme", status="completed", output=[item],
                        usage=dict(input_tokens=10, output_tokens=10, total_tokens=20))
        events = [
            ("response.created", dict(response=dict(id="resp_theme", status="in_progress", output=[]))),
            ("response.output_item.added", dict(item=dict(item, content=[], status="in_progress"), output_index=0)),
            ("response.content_part.added", dict(item_id=item["id"], output_index=0, content_index=0, part=dict(part, text=""))),
            ("response.output_text.delta", dict(item_id=item["id"], output_index=0, content_index=0, delta=output)),
            ("response.output_item.done", dict(item=item, output_index=0)),
            ("response.completed", dict(response=response)),
        ]
        payload = "".join(
            f"event: {event}\ndata: {json.dumps(dict(data, type=event))}\n\n" for event, data in events
        ).encode()
        self.respond("text/event-stream", payload)


def capture_scenes(binary, codex, output, scratch, port):
    profile, project, codex_home, temp = (scratch / name for name in ("hitty", "project", "codex", "temp"))
    for path in (profile, project, codex_home, temp):
        path.mkdir()
    (codex_home / "config.toml").write_text(
        'model = "fixture"\nmodel_provider = "fixture"\ncheck_for_update_on_startup = false\n'
        'cli_auth_credentials_store = "file"\n[model_providers.fixture]\nname = "Local fixture"\n'
        f'base_url = "http://127.0.0.1:{port}/v1"\nwire_api = "responses"\n'
        f'[projects.{json.dumps(project.as_posix())}]\ntrust_level = "trusted"\n', encoding="utf-8",
    )
    (profile / "window.json").write_text('{"x":80,"y":80,"width":1200,"height":800}', encoding="utf-8")
    pane_env = dict(CODEX_HOME=str(codex_home), TEMP=str(temp), TMP=str(temp))
    env = {k: v for k, v in os.environ.items() if not k.startswith(
        ("HITTY", "CODEX_", "OPENAI_", "HC_", "CLAUDE", "ANTHROPIC_")
    )}
    for key in ("NO_COLOR", "FORCE_COLOR", "COLORFGBG", "TERM", "COLORTERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION"):
        env.pop(key, None)
    env.update(HITTY_CONFIG_DIR=str(profile), HITTY_DATA_DIR=str(profile / "data"), **pane_env)
    config = dict(theme_follow_system=False, theme_legible_palette=True, gui_language="en",
                  check_for_updates=False, auto_download_updates=False, show_tray_icon=False,
                  install_cli_on_path=False, restore_session=False, restore_agent_sessions=False,
                  cursor_blink=False, dim_inactive_panes=False,
                  font_family="Hack", font_size=15, line_height=1.4, ui_font_size=14,
                  shell=dict(program="cmd.exe", args=["/d", "/k"]), env=pane_env)

    def current_background():
        try:
            return json.loads((profile / "data/appearance.json").read_text())["terminal_colors"][1]
        except (OSError, ValueError, KeyError):
            return None

    def set_theme(theme, wait=True):
        pending = profile / "config.tmp"
        pending.write_text(json.dumps(dict(config, theme_preset=theme)), encoding="utf-8")
        pending.replace(profile / "config.json")
        if wait:
            wait_for(lambda: current_background() == THEMES[theme], f"{theme} applied")

    records = {}

    def cli(*args, check=True):
        result = subprocess.run([str(binary / "hitty.exe"), *args], check=check, env=env,
                                capture_output=True, encoding="utf-8", errors="replace",
                                timeout=30, creationflags=subprocess.CREATE_NO_WINDOW)
        return result.stdout if result.returncode == 0 else ""

    def panes():
        return json.loads(cli("pane", "ls", "--json", check=False) or "{}").get("panes", [])

    def screen(pane):
        return cli("capture", pane, "--plain", "--scrollback")

    def open_codex(pane):
        wait_for(lambda: screen(pane).strip(), "shell prompt")
        # The fixture only talks to the loopback server and issues no tool calls.
        cli("send", pane, f'"{codex}" --no-daemon --no-alt-screen --sandbox danger-full-access -a never', "--enter")
        wait_for(lambda: all(x in screen(pane) for x in ("OpenAI Codex", "Ask Codex", "fixture")), "Codex composer")
        cli("send", pane, PROMPT)
        wait_for(lambda: PROMPT in screen(pane), "plan prompt")
        cli("send", pane, "--enter")
        wait_for(lambda: "Implement this plan?" in screen(pane), "plan confirmation")
        cli("send", pane, "--key", "escape")
        cli("send", pane, DRAFT)
        wait_for(lambda: DRAFT in screen(pane), "unsent draft")

    def shot(app, pane, scene):
        content = screen(pane)
        assert all(text in content for text in ("Proposed Plan", PLAN, DRAFT)), scene
        command = ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                   "-File", str(ROOT / "scripts/capture-native-window.ps1"), "-ProcId", str(app.pid),
                   "-Out", str(output / f"{scene}.png"), "-SettleMs", "1500"]
        result = subprocess.run(command, check=True, capture_output=True, text=True, timeout=35,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        path = output / f"{scene}.png"
        records[scene] = dict(surfaces=surfaces(path, current_background()),
                              png_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                              capture=result.stdout.strip())
        print(scene, fills(records[scene]), flush=True)

    set_theme("one_light", wait=False)
    user32 = ctypes.windll.user32
    user32.GetForegroundWindow.restype = wintypes.HWND
    user32.GetWindowThreadProcessId.argtypes = (wintypes.HWND, ctypes.POINTER(wintypes.DWORD))
    user32.SetForegroundWindow.argtypes = (wintypes.HWND,)
    previous = user32.GetForegroundWindow()
    with (scratch / "hitty.log").open("wb") as log:
        app = subprocess.Popen([str(binary / "hitty-app.exe"), "--config-dir", str(profile)], cwd=project,
                               env=env, stdout=log, stderr=log, creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            initial = wait_for(panes, "initial shell")[0]
            foreground_pid = wintypes.DWORD()
            user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), ctypes.byref(foreground_pid))
            if previous and foreground_pid.value == app.pid:
                user32.SetForegroundWindow(previous)
            pane = f"%{initial['pane']}"
            cli("ws", "rename", str(initial["workspace"]), "Theme test")
            wait_for(lambda: current_background() == THEMES["one_light"], "initial theme applied")
            open_codex(pane)
            shot(app, pane, "open-light")
            set_theme("one_dark_pro")
            shot(app, pane, "switched-dark")
            set_theme("one_light")
            shot(app, pane, "roundtrip-light")
            set_theme("one_dark_pro")
            cli("tab", "new", str(initial["workspace"]), "--cwd", str(project))
            fresh = wait_for(lambda: [p for p in panes() if p["pane"] != initial["pane"]], "fresh tab")[0]
            pane = f"%{fresh['pane']}"
            open_codex(pane)
            shot(app, pane, "fresh-dark")
            set_theme("one_light")
            shot(app, pane, "switched-light")
            set_theme("one_dark_pro")
            shot(app, pane, "roundtrip-dark")
        finally:
            if app.poll() is None:
                app.terminate()
                app.wait(timeout=20)
            cli("server", "stop", check=False)
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True, help="matching Hitty, CLI and bundled ConPTY binaries")
    parser.add_argument("--codex-bin", type=Path, required=True, help="path to the Codex 0.169.0 executable")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--scratch-dir", type=Path, default=Path(tempfile.gettempdir()))
    parser.add_argument("--assert-colors", action="store_true")
    parser.add_argument("--baseline-dir", type=Path, help="also preserve all fresh light and dark fills")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Responses) as server:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            with tempfile.TemporaryDirectory(prefix="hitty-theme-", dir=args.scratch_dir) as scratch:
                records = capture_scenes(args.bin_dir.resolve(strict=True), args.codex_bin.resolve(strict=True),
                                         args.output.resolve(), Path(scratch), server.server_port)
        finally:
            server.shutdown()
    for first, returned in [("open-light", "roundtrip-light"), ("fresh-dark", "roundtrip-dark")]:
        assert records[first]["png_sha256"] == records[returned]["png_sha256"], (first, returned)
    dark_matched = fills(records["switched-dark"]) == fills(records["fresh-dark"])
    light_matched = fills(records["switched-light"]) == fills(records["open-light"])
    native_preserved = None
    if args.baseline_dir:
        baseline = json.loads((args.baseline_dir / "captures.json").read_text(encoding="utf-8"))
        native_preserved = all(fills(records[scene]) == fills(baseline["captures"][scene])
                               for scene in ("open-light", "fresh-dark"))
    report = dict(binary_sha256=hashlib.sha256((args.bin_dir / "hitty-app.exe").read_bytes()).hexdigest(),
                  codex=subprocess.check_output([str(args.codex_bin), "--version"], text=True).strip(),
                  themes=list(THEMES), logical_viewport=[1200, 800], forced_color=False,
                  dark_matches_new_session=dark_matched, light_matches_new_session=light_matched,
                  native_fills_preserved=native_preserved,
                  roundtrip_pixels_unchanged=True, captures=records)
    (args.output / "captures.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if args.baseline_dir:
        assert native_preserved, "fresh Codex fills differ from the native light/dark baseline"
        print("PASS: all fresh light and dark fills preserve the native baseline", flush=True)
    if args.assert_colors:
        assert dark_matched, "switched Codex dark fills differ from a new session"
        assert light_matched, "switched Codex light fills differ from a new session"
    print(f"Dark matches new session: {dark_matched}; light matches new session: {light_matched}; "
          "both round trips preserve every pixel", flush=True)


if __name__ == "__main__":
    main()
