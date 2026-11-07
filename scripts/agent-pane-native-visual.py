"""Capture base/head agent pane chrome on Windows with identical fixture data.

Builds nothing. `prepare-before` overlays only the fixture entry on a separate
baseline checkout; `capture` runs a built binary scene by scene and renders the
window with scripts/capture-native-window.ps1 (PrintWindow, no pointer, no
overlap), posting a synthetic pointer move to a pane's top edge where a scene
asks for hover.
"""

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
CAPTURE = ROOT / "scripts/capture-native-window.ps1"
WHEEL = ROOT / "scripts/post-mouse-wheel.ps1"
SETTINGS_SCROLL = (1000, 500, 10)  # logical point in the content column, wheel notches down
ENTRY = b'''    #[cfg(feature = "markdown-visual-tests")]
    if let Some(index) = args.iter().position(|arg| arg == "--agent-pane-visual") {
        agent_pane_visual::run(args.get(index + 1).expect("agent pane fixture options path"));
        return;
    }

'''
# Logical window: 1200x700, 32px title bar, 240px sidebar, 2x2 split.
TITLE_BAR, SIDEBAR, W, H = 32, 240, 1200, 700
PANE_W = (W - SIDEBAR) / 2
PANE_H = (H - TITLE_BAR) / 2
TOP_EDGE = 6  # inside the reveal band, above the first terminal row
# A quarter of the way across: on the reveal band, clear of the drag grip's
# 80px handle in the middle, whose tooltip would otherwise cover the text.
EDGES = {  # pane index -> logical (x, y) on that pane's top edge
    0: (SIDEBAR + PANE_W / 4, TITLE_BAR + TOP_EDGE),
    3: (SIDEBAR + PANE_W * 1.25, TITLE_BAR + PANE_H + TOP_EDGE),
}
SCENES = [  # (name, titles, dark, hover pane, zoom, settings page, layout)
    ("default-dark", False, True, None, False, False, "grid"),
    ("hover-dark", False, True, 0, False, False, "grid"),
    ("titles-dark", True, True, None, False, False, "grid"),
    ("titles-hover-dark", True, True, 0, False, False, "grid"),
    ("titles-light", True, False, None, False, False, "grid"),
    ("titles-hover-light", True, False, 0, False, False, "grid"),
    ("ordinary-hover-dark", True, True, 3, False, False, "grid"),
    ("zoom-hover-dark", True, True, 0, True, False, "grid"),
    ("settings-light", True, False, None, False, True, "grid"),
    ("titles-column-dark", True, True, None, False, False, "column"),
]
SETTINGS_HEIGHT = 1400  # the Window & Tabs page is taller than the pane scenes


def replace_once(path, old, new):
    data = path.read_bytes()
    if new in data:
        return
    assert data.count(old) == 1, f"{path}: anchor not unique"
    path.write_bytes(data.replace(old, new, 1))


def prepare_before(destination):
    destination = destination.resolve(strict=True)
    assert destination != ROOT, "before must be a separate checkout"
    (destination / "src/agent_pane_visual.rs").write_bytes(
        (ROOT / "src/agent_pane_visual.rs").read_bytes())
    main = destination / "src/main.rs"
    replace_once(main, b'#[cfg(feature = "markdown-visual-tests")]\nmod agent_attention_visual;\n',
                 b'#[cfg(feature = "markdown-visual-tests")]\nmod agent_attention_visual;\n'
                 b'#[cfg(feature = "markdown-visual-tests")]\nmod agent_pane_visual;\n')
    replace_once(main, b"    apply_config_dir_arg(&args);\n\n", b"    apply_config_dir_arg(&args);\n\n" + ENTRY)


def capture_scene(binary, output, phase, scene, scale):
    name, titles, dark, hover, zoom, settings, layout = scene
    png = output / f"{phase}-{name}.png"
    log = output / f"{phase}-{name}.log"
    with tempfile.TemporaryDirectory(prefix="ctty7-agent-pane-visual-") as temporary:
        options = pathlib.Path(temporary) / "options.json"
        options.write_text(json.dumps({"output": str(png), "titles": titles, "dark": dark,
                                       "zoom": zoom, "settings": settings, "layout": layout, "hold_seconds": 90,
                                       "height": SETTINGS_HEIGHT if settings else None}), encoding="utf-8")
        with log.open("w", encoding="utf-8") as log_stream:
            process = subprocess.Popen([str(binary), "--config-dir", temporary,
                                        "--agent-pane-visual", str(options)],
                                       stdout=log_stream, stderr=log_stream,
                                       creationflags=subprocess.DETACHED_PROCESS)
            try:
                deadline = time.monotonic() + 60
                while "NATIVE_AGENT_PANES_READY" not in log.read_text(encoding="utf-8", errors="replace"):
                    assert process.poll() is None, f"fixture exited: {log.read_text(errors='replace')}"
                    assert time.monotonic() < deadline, "fixture timed out"
                    time.sleep(0.2)
                subprocess.run(["powershell", "-NoProfile", "-Command",
                                f"(New-Object -ComObject WScript.Shell).AppActivate({process.pid})"],
                               check=False, capture_output=True)
                time.sleep(0.8)
                if settings:
                    x, y, notches = SETTINGS_SCROLL
                    subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                                    str(WHEEL), "-ProcId", str(process.pid), "-X", str(int(x * scale)),
                                    "-Y", str(int(y * scale)), "-Notches", str(notches)],
                                   check=True, capture_output=True)
                    time.sleep(0.8)
                command = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                           str(CAPTURE), "-ProcId", str(process.pid), "-Out", str(png), "-SettleMs", "1200"]
                if hover is not None:
                    x, y = EDGES[hover]
                    command += ["-MoveTo", f"{int(x * scale)},{int(y * scale)}"]
                result = subprocess.run(command, check=True, capture_output=True, text=True)
                print(f"  {name}: {result.stdout.strip()}", flush=True)
            finally:
                png.with_suffix(".done").write_text("captured", encoding="utf-8")
                try:
                    process.wait(timeout=20)
                except subprocess.TimeoutExpired:
                    process.kill()
                png.with_suffix(".done").unlink(missing_ok=True)
    return png


def capture(binary, output, phase, repository, scale, only=None):
    output.mkdir(parents=True, exist_ok=True)
    for scene in SCENES:
        if only and scene[0] not in only:
            continue
        capture_scene(binary, output, phase, scene, scale)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repository, text=True).strip()
    (output / f"{phase}-metadata.json").write_text(json.dumps({
        "revision": revision, "platform": "Windows / GPUI / PrintWindow",
        "viewport": [W, H], "display_scale": scale,
        "source": "Production Tty7App render; deterministic daemon messages, no live agent.",
        "scenes": [s[0] for s in SCENES],
    }, indent=2), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    before = sub.add_parser("prepare-before")
    before.add_argument("destination", type=pathlib.Path)
    shot = sub.add_parser("capture")
    shot.add_argument("binary", type=pathlib.Path)
    shot.add_argument("output", type=pathlib.Path)
    shot.add_argument("--phase", choices=("before", "after"), required=True)
    shot.add_argument("--repository", type=pathlib.Path, default=ROOT)
    shot.add_argument("--scale", type=float, required=True, help="display scale, e.g. 1.75")
    shot.add_argument("--only", nargs="*", help="scene names to capture; default all")
    args = parser.parse_args()
    if args.command == "prepare-before":
        prepare_before(args.destination)
    else:
        capture(args.binary.resolve(strict=True), args.output.resolve(), args.phase,
                args.repository.resolve(strict=True), args.scale, args.only)


if __name__ == "__main__":
    sys.exit(main())
