"""Capture base/head production tab chrome with identical daemon fixture data."""

import argparse
import json
import pathlib
import subprocess
import tempfile
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]
ENTRY = '''    #[cfg(feature = "markdown-visual-tests")]
    if let Some(index) = args.iter().position(|arg| arg == "--ssh-tab-visual") {
        ssh_tab_visual::run(args.get(index + 1).expect("SSH tab fixture options path"));
        return;
    }

'''


def prepare_before(destination):
    destination = destination.resolve(strict=True)
    assert destination != ROOT, "before must be a separate checkout"
    # Overlay only the fixture entry and quiet transport helpers. Production
    # tab selectors/renderers stay exactly as they were at the base revision.
    (destination / "src/ssh_tab_visual.rs").write_bytes(
        (ROOT / "src/ssh_tab_visual.rs").read_bytes())
    main = destination / "src/main.rs"
    text = main.read_text(encoding="utf-8")
    if "mod ssh_tab_visual;" not in text:
        assert "mod markdown_visual;\n" in text
        text = text.replace("mod markdown_visual;\n", "mod markdown_visual;\n"
                            '#[cfg(feature = "markdown-visual-tests")]\nmod ssh_tab_visual;\n', 1)
    if 'arg == "--ssh-tab-visual"' not in text:
        assert "    apply_config_dir_arg(&args);\n\n" in text
        text = text.replace("    apply_config_dir_arg(&args);\n\n",
                            "    apply_config_dir_arg(&args);\n\n" + ENTRY, 1)
    main.write_text(text, encoding="utf-8")
    view = destination / "src/terminal/view.rs"
    text = view.read_text(encoding="utf-8")
    for helper in ("quiet_test_pane", "quiet_test_shell_parts"):
        old = f"#[cfg(test)]\npub(crate) fn {helper}("
        replacement = ('#[cfg(any(test, feature = "markdown-visual-tests"))]\n'
                       f"pub(crate) fn {helper}(")
        if old in text:
            assert text.count(old) == 1, f"ambiguous base helper {helper}"
            text = text.replace(old, replacement, 1)
        else:
            assert text.count(replacement) == 1, f"missing base helper {helper}"
    view.write_text(text, encoding="utf-8")


def capture(binary, output, phase, repository):
    output.mkdir(parents=True, exist_ok=True)
    direct_actions = "titlebar-settings" in (repository / "src/ui/tab_strip.rs").read_text(encoding="utf-8")
    titlebar_action = "settings" if direct_actions else "menu"
    desktop_log = (output / f"{phase}-desktop.log").open("w", encoding="utf-8")
    manager = subprocess.Popen(["openbox"], stdout=desktop_log, stderr=desktop_log)
    try:
        deadline = time.monotonic() + 10
        while "window id #" not in subprocess.check_output(
                ["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"], text=True):
            assert manager.poll() is None, "Openbox exited before desktop initialization"
            assert time.monotonic() < deadline, "Openbox initialization timed out"
            time.sleep(0.1)
        for surface in ("top", "sidebar"):
            screenshot = output / f"{phase}-{surface}.png"
            log = output / f"{phase}-{surface}.log"
            with tempfile.TemporaryDirectory(prefix="ctty7-ssh-visual-") as temporary:
                options = pathlib.Path(temporary) / "options.json"
                options.write_text(json.dumps({"sidebar": surface == "sidebar",
                                               "output": str(screenshot)}), encoding="utf-8")
                with log.open("w", encoding="utf-8") as log_stream:
                    process = subprocess.Popen([str(binary), "--config-dir", temporary,
                                                "--ssh-tab-visual", str(options)],
                                               stdout=log_stream, stderr=log_stream)
                    try:
                        deadline = time.monotonic() + 45
                        while "NATIVE_SSH_TABS_READY" not in log.read_text(encoding="utf-8"):
                            assert process.poll() is None, f"fixture exited: {log.read_text()}"
                            assert time.monotonic() < deadline, f"fixture timed out: {log.read_text()}"
                            time.sleep(0.1)
                        windows = subprocess.check_output([
                            "xdotool", "search", "--name", "^SSH tab visual acceptance$"
                        ], text=True).splitlines()
                        assert len(windows) == 1, f"expected one fixture window: {windows}"
                        subprocess.run(["import", "-window", windows[0], str(screenshot)], check=True)
                        colors = int(subprocess.check_output([
                            "identify", "-format", "%k", str(screenshot)], text=True))
                        assert colors > 20 and screenshot.stat().st_size > 5000, "blank screenshot"
                        # The fixture is 1200x660. The trailing titlebar tile
                        # opens Settings with direct actions, or the app menu
                        # on an older baseline. Keep both captures accurately named.
                        subprocess.run(["xdotool", "windowactivate", "--sync", windows[0]],
                                       check=True)
                        subprocess.run(["xdotool", "mousemove", "--sync", "--window",
                                        windows[0], "1077", "20", "click", "1",
                                        "mousemove", "--window", windows[0], "600", "300"],
                                       check=True)
                        time.sleep(0.5)
                        action_screenshot = output / f"{phase}-{surface}-{titlebar_action}.png"
                        subprocess.run(["import", "-window", windows[0], str(action_screenshot)],
                                       check=True)
                        assert action_screenshot.read_bytes() != screenshot.read_bytes(), \
                            f"opening {titlebar_action} did not change the captured window"
                        process.wait(timeout=20)
                        assert process.returncode == 0, log.read_text()
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            process.wait(timeout=10)
            print(f"Captured {phase} {surface}: {screenshot}", flush=True)
        revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repository,
                                           text=True).strip()
        (output / f"{phase}-metadata.json").write_text(json.dumps({
            "revision": revision, "platform": "Linux / X11 / GPUI",
            "viewport": [1200, 660], "theme": "light",
            "titlebar_action": titlebar_action,
            "titlebar_action_trigger": "Opened through the production trailing tile at window coordinates (1077, 20).",
            "source": "Production Tty7App render; deterministic daemon messages, no live SSH.",
            "cases": ["local shell", "local Codex", "local Claude Code", "native SSH shell",
                      "terminal SSH with detected Codex", "native SSH with detected Claude Code"],
        }, indent=2), encoding="utf-8")
    finally:
        manager.terminate()
        manager.wait(timeout=10)
        desktop_log.close()


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
    args = parser.parse_args()
    if args.command == "prepare-before":
        prepare_before(args.destination)
    else:
        capture(args.binary.resolve(strict=True), args.output.resolve(), args.phase,
                args.repository.resolve(strict=True))


if __name__ == "__main__":
    main()
