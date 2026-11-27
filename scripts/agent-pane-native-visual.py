"""Capture base/head agent pane chrome on Windows with identical fixture data.

Builds nothing. `prepare-before` overlays only the fixture entry on a separate
baseline checkout; `capture` runs a built binary scene by scene and renders the
window with scripts/capture-native-window.ps1 (PrintWindow, no pointer, no
overlap), posting a synthetic pointer move to a pane's top edge where a scene
asks for hover.
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
TWO_DIGIT_MINUTES = "session-activity-two-digit-minutes-light"
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
    ("task-titles-off-dark", True, True, None, False, False, "grid"),
    ("task-titles-dark", True, True, None, False, False, "grid"),
    ("task-titles-light", True, False, None, False, False, "grid"),
    ("task-titles-hover-dark", True, True, 0, False, False, "grid"),
    ("task-titles-column-dark", True, True, None, False, False, "column"),
    ("task-titles-settings-light", True, False, None, False, True, "grid"),
    ("session-activity-light", False, False, None, False, False, "sidebar-activity"),
    ("session-activity-hover-light", False, False, None, False, False, "sidebar-activity"),
    ("session-activity-hover-dark", False, True, None, False, False, "sidebar-activity"),
    ("session-activity-short-hover-light", False, False, None, False, False, "sidebar-activity"),
    (TWO_DIGIT_MINUTES, False, False, None, False, False, "sidebar-activity"),
    ("session-activity-wrap-light", False, False, None, False, False, "sidebar-activity"),
    ("session-activity-wrap-dark", False, True, None, False, False, "sidebar-activity"),
    ("session-activity-wrap-large", False, False, None, False, False, "sidebar-activity"),
    ("session-activity-wrap-hover-large", False, False, None, False, False, "sidebar-activity"),
    ("sidebar-product-icons-light", False, False, None, False, False, "sidebar-activity"),
    ("sidebar-product-icons-dark", False, True, None, False, False, "sidebar-activity"),
    ("sidebar-product-icons-settings", False, False, None, False, True, "grid"),
    ("sidebar-icon-choices-light", False, False, None, False, False, "sidebar-activity"),
    ("sidebar-icon-choices-dark", False, True, None, False, False, "sidebar-activity"),
    ("sidebar-icon-choices-settings", False, False, None, False, True, "grid"),
    ("sidebar-icon-choices-expanded", False, False, None, False, True, "grid"),
    ("sidebar-icon-choices-clawd-menu", False, False, None, False, True, "grid"),
    ("agent-hooks-settings", False, False, None, False, True, "grid"),
    ("pinned-empty-sidebar-light", False, False, None, False, False, "pinned-empty"),
    ("pinned-empty-sidebar-dark", False, True, None, False, False, "pinned-empty"),
    ("dir-header-plus-menu-light", False, False, None, False, False, "pinned-grouped"),
    ("dir-header-plus-menu-dark", False, True, None, False, False, "pinned-grouped"),
    ("agents-quick-launch-settings-light", False, False, None, False, True, "pinned-empty"),
    ("agents-quick-launch-settings-dark", False, True, None, False, True, "pinned-empty"),
]
# Scenes whose agent panes title themselves with a task, and whether the
# task-title setting is on: scene name -> (tasks, task_titles).
TASKS = {
    "task-titles-off-dark": (True, False),
    "task-titles-dark": (True, True),
    "task-titles-light": (True, True),
    "task-titles-hover-dark": (True, True),
    "task-titles-column-dark": (True, True),
    "task-titles-settings-light": (False, True),
    "session-activity-light": (True, False),
    "session-activity-hover-light": (True, False),
    "session-activity-hover-dark": (True, False),
    "session-activity-short-hover-light": (True, False),
    TWO_DIGIT_MINUTES: (True, False),
    "session-activity-wrap-light": (True, False),
    "session-activity-wrap-dark": (True, False),
    "session-activity-wrap-large": (True, False),
    "session-activity-wrap-hover-large": (True, False),
    "sidebar-product-icons-light": (True, False),
    "sidebar-product-icons-dark": (True, False),
    "sidebar-icon-choices-light": (True, False),
    "sidebar-icon-choices-dark": (True, False),
}
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
    if b'"--agent-pane-visual"' not in main.read_bytes():
        replace_once(main, b'#[cfg(feature = "markdown-visual-tests")]\nmod agent_attention_visual;\n',
                     b'#[cfg(feature = "markdown-visual-tests")]\nmod agent_attention_visual;\n'
                     b'#[cfg(feature = "markdown-visual-tests")]\nmod agent_pane_visual;\n')
        replace_once(main, b"    apply_config_dir_arg(&args);\n\n", b"    apply_config_dir_arg(&args);\n\n" + ENTRY)


def capture_scene(binary, output, phase, scene, scale, activity_reference_secs=None):
    name, titles, dark, hover, zoom, settings, layout = scene
    tasks, task_titles = TASKS.get(name, (False, False))
    activity_ages = None
    if name.startswith("session-activity-wrap-"):
        activity_ages = [46 * 60, 12 * 3600, 8 * 86400]
    elif name == TWO_DIGIT_MINUTES:
        activity_ages = [60, 28 * 60, 6 * 60]
        # Seed against the fixture's clock even when the before capture is old.
        activity_reference_secs = None
    sidebar_width = 180 if name.endswith("-large") else 206
    ui_font_size = 24 if name.endswith("-large") else 16
    png = output / f"{phase}-{name}.png"
    log = output / f"{phase}-{name}.log"
    with tempfile.TemporaryDirectory(
        prefix="hitty-agent-pane-visual-",
        ignore_cleanup_errors=True,
    ) as temporary:
        temp_root = pathlib.Path(temporary)
        options = temp_root / "options.json"
        pinned_dirs = []
        proc_env = None
        is_pinned_launcher_scene = name.startswith((
            "pinned-empty-sidebar-",
            "dir-header-plus-menu-",
            "agents-quick-launch-settings-",
        ))
        if is_pinned_launcher_scene:
            projects_root = temp_root / "projects"
            for folder in ("hitty", "my-app"):
                target_dir = projects_root / folder
                target_dir.mkdir(parents=True, exist_ok=True)
                pinned_dirs.append(str(target_dir))
            fake_bin = temp_root / "fake-bin"
            fake_bin.mkdir(parents=True, exist_ok=True)
            for shim in ("claude.exe", "codex.cmd", "agy.cmd", "qodercli.exe", "codebuddy.cmd"):
                (fake_bin / shim).write_text("", encoding="utf-8")
            proc_env = os.environ.copy()
            sys_root = os.environ.get("SystemRoot", r"C:\Windows")
            proc_env["PATH"] = f"{fake_bin};{sys_root}\\System32"
        if layout == "sidebar-activity":
            options.with_name("config.json").write_text(json.dumps({
                "sidebar_width": sidebar_width, "sidebar_grouping": "none",
                "ui_font_size": ui_font_size,
                "theme_follow_system": False, "theme_preset": "dark" if dark else "light",
                "install_cli_on_path": False,
            }), encoding="utf-8")
        is_agents_quick_launch = name.startswith("agents-quick-launch-settings-")
        scene_height = 820 if is_agents_quick_launch else (SETTINGS_HEIGHT if settings else None)
        options.write_text(json.dumps({"output": str(png), "titles": titles, "dark": dark,
                                       "tasks": tasks, "task_titles": task_titles,
                                       "sidebar_product_icons": name.startswith(("sidebar-product-icons", "sidebar-icon-choices")),
                                       "sidebar_icon_fixture": name.startswith("sidebar-icon-choices"),
                                       "activity_ages": activity_ages,
                                       "activity_reference_secs": activity_reference_secs,
                                       "short_title": "short" in name,
                                       "sidebar_width": sidebar_width if layout == "sidebar-activity" else None,
                                       "ui_font_size": ui_font_size,
                                       "zoom": zoom, "settings": settings, "layout": layout, "hold_seconds": 90,
                                       "pinned_dirs": pinned_dirs,
                                       "settings_section": "agents" if is_agents_quick_launch else None,
                                       "agent_launch_commands": {"codex": "codex --yolo"} if is_agents_quick_launch else {},
                                       "height": scene_height}), encoding="utf-8")
        with log.open("w", encoding="utf-8") as log_stream:
            process = subprocess.Popen([str(binary), "--config-dir", temporary,
                                        "--agent-pane-visual", str(options)],
                                       stdout=log_stream, stderr=log_stream,
                                       env=proc_env,
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
                if settings and name != "agent-hooks-settings" and not is_agents_quick_launch:
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
                elif name.startswith("dir-header-plus-menu-"):
                    # Hover the first pinned group header (`hitty`) to reveal [Pin] [+] [...],
                    # then click the `+` button to open the bottom-start launch menu.
                    subprocess.run([
                        "powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                        str(CAPTURE), "-ProcId", str(process.pid),
                        "-Out", str(temp_root / "hover.png"),
                        "-MoveTo", f"{int(120 * scale)},{int(130 * scale)}", "-SettleMs", "500",
                    ], check=True, capture_output=True)
                    command += ["-MoveTo", f"{int(194 * scale)},{int(130 * scale)}", "-Click"]
                elif layout == "sidebar-activity":
                    x, y = (100, 152 if name.endswith("-large") else 132) if "hover" in name else (700, 80)
                    command += ["-MoveTo", f"{int(x * scale)},{int(y * scale)}"]
                    if "hover" in name:
                        subprocess.run([
                            "powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                            str(CAPTURE), "-ProcId", str(process.pid),
                            "-Out", str(png.with_suffix(".away.png")),
                            "-MoveTo", f"{int(700 * scale)},{int(400 * scale)}", "-SettleMs", "600",
                        ], check=True, capture_output=True)
                elif name == "sidebar-icon-choices-settings":
                    command += ["-MoveTo", "400,345"]
                elif name in ("sidebar-icon-choices-expanded", "sidebar-icon-choices-clawd-menu"):
                    command += ["-MoveTo", f"{int(978 * scale)},{int(316 * scale)}", "-Click"]
                    if name.endswith("clawd-menu"):
                        subprocess.run(command, check=True, capture_output=True)
                        command[-2] = f"{int(822 * scale)},{int(416 * scale)}"
                elif name == "agent-hooks-settings":
                    command += ["-MoveTo", f"{int(66 * scale)},{int(277 * scale)}", "-Click"]
                result = subprocess.run(command, check=True, capture_output=True, text=True)
                print(f"  {name}: {result.stdout.strip()}", flush=True)
                if layout == "sidebar-activity" and "hover" in name:
                    subprocess.run([
                        "powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                        str(CAPTURE), "-ProcId", str(process.pid),
                        "-Out", str(png.with_suffix(".dismissed.png")),
                        "-MoveTo", f"{int(700 * scale)},{int(400 * scale)}", "-SettleMs", "1200",
                    ], check=True, capture_output=True)
            finally:
                png.with_suffix(".done").write_text("captured", encoding="utf-8")
                try:
                    process.wait(timeout=20)
                except subprocess.TimeoutExpired:
                    process.kill()
                png.with_suffix(".done").unlink(missing_ok=True)
    return png


def capture_sidebar_activity(binary, output, phase, scene, scale, activity_reference_secs):
    with tempfile.TemporaryDirectory(prefix="hitty-sidebar-activity-") as temporary:
        fixture = pathlib.Path(temporary)
        for index in range(4):
            repo = fixture / f"fixture-repo-{index}"
            repo.mkdir()
            branch = "feature/sidebar-activity-and-recover-after-restart" if index == 0 else "main"
            git = ["git", "-C", str(repo), "-c", "core.autocrlf=false"]
            subprocess.run([*git, "init", "-q", "-b", branch], check=True)
            if index == 0:
                notes = repo / "notes.txt"
                notes.write_text("Keep\n" + "Remove\n" * 4, encoding="utf-8")
                subprocess.run([*git, "add", "notes.txt"], check=True)
                subprocess.run([*git, "-c", "user.name=Hitty Fixture", "-c", "user.email=fixture@example.invalid",
                                "-c", "commit.gpgsign=false", "commit", "-qm", "Initial notes"], check=True)
                notes.write_text("Keep\n" + "Add\n" * 18, encoding="utf-8")
        png = capture_scene(binary, fixture, phase, scene, scale, activity_reference_secs)
        for evidence in (png, png.with_suffix(".log"), png.with_suffix(".layout.json"),
                         png.with_suffix(".dismissed.png")):
            if not evidence.exists():
                continue
            shutil.copyfile(evidence, output / evidence.name)
        if scene[0].startswith("session-activity-"):
            layout = json.loads(png.with_suffix(".layout.json").read_text(encoding="utf-8"))
            assert abs(layout["display_scale"] - scale) < 0.01, "display scale differs from capture settings"
            before, after = layout["without_activity"], layout["with_activity"]
            assert len(before) == len(after) == 4, "missing sidebar row measurements"
            wrapped = [index for index, (a, b) in enumerate(zip(before, after)) if b > a + 1]
            print(f"  single-line activity: {'FAIL' if wrapped else 'PASS'}; "
                  f"row heights {before} -> {after}", flush=True)
            if phase == "after":
                assert not wrapped, f"activity wrapped and increased row heights: {wrapped}"


def capture(binary, output, phase, repository, scale, only=None):
    output.mkdir(parents=True, exist_ok=True)
    before_metadata = output / "before-metadata.json"
    activity_reference_secs = int(time.time())
    if phase == "after" and before_metadata.exists():
        activity_reference_secs = json.loads(before_metadata.read_text(encoding="utf-8")).get(
            "activity_reference_secs") or activity_reference_secs
    for scene in SCENES:
        if only and scene[0] not in only:
            continue
        if scene[-1] == "sidebar-activity":
            capture_sidebar_activity(binary, output, phase, scene, scale, activity_reference_secs)
        else:
            capture_scene(binary, output, phase, scene, scale)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repository, text=True).strip()
    (output / f"{phase}-metadata.json").write_text(json.dumps({
        "revision": revision, "platform": "Windows / GPUI / PrintWindow",
        "viewport": [W, H], "display_scale": scale,
        "activity_reference_secs": None if only == [TWO_DIGIT_MINUTES] else activity_reference_secs,
        "source": "Production HittyApp render; deterministic daemon messages, no live agent.",
        "scenes": [s[0] for s in SCENES if not only or s[0] in only],
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
