"""Capture the production Markdown view on a native macOS or X11 desktop."""

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile
import time


def capture(binary, output, dark, width, scale, anchor):
    name = f"{'dark' if dark else 'light'}-{width}-{scale:g}-{anchor or 'top'}"
    screenshot = output / f"{name}.png"
    log = output / f"{name}.log"
    with tempfile.TemporaryDirectory(prefix="ctty7-markdown-visual-") as temporary:
        options = pathlib.Path(temporary) / "options.json"
        options.write_text(json.dumps({
            "source": str(pathlib.Path("docs/examples/markdown-github-theme.md").resolve()),
            "width": width, "height": 900, "scale": scale, "dark": dark,
            "anchor": anchor, "output": str(screenshot),
        }), encoding="utf-8")
        log_stream = log.open("w", encoding="utf-8")
        process = subprocess.Popen(
            [str(binary), "--config-dir", temporary, "--markdown-visual", str(options)],
            stdout=log_stream, stderr=log_stream,
        )
        try:
            deadline = time.monotonic() + 45
            while time.monotonic() < deadline:
                if "NATIVE_MARKDOWN_READY" in log.read_text(encoding="utf-8"):
                    break
                if process.poll() is not None:
                    if sys.platform == "darwin" and process.returncode < 0:
                        trace = output / f"{name}-backtrace.log"
                        with trace.open("w", encoding="utf-8") as trace_stream:
                            try:
                                subprocess.run([
                                    "/usr/bin/lldb", "--batch", "-o", "run",
                                    "-k", "thread backtrace all", "--", str(binary),
                                    "--config-dir", temporary, "--markdown-visual", str(options),
                                ], stdout=trace_stream, stderr=trace_stream, timeout=45)
                            except subprocess.TimeoutExpired:
                                trace_stream.write("Debugger timed out\n")
                    raise RuntimeError(f"{name}: fixture exited before drawing with code "
                                       f"{process.returncode}: {log.read_text()}")
                time.sleep(0.1)
            else:
                raise TimeoutError(f"{name}: no native ready marker")
            if sys.platform != "darwin":
                window = subprocess.check_output([
                    "xdotool", "search", "--name", "^Markdown visual acceptance$"
                ], text=True).splitlines()
                if len(window) != 1:
                    raise RuntimeError(f"{name}: expected one native window, got {window}")
                subprocess.run(["import", "-window", window[0], str(screenshot)], check=True)
                colors = int(subprocess.check_output(
                    ["identify", "-format", "%k", str(screenshot)], text=True))
                if colors < 20:
                    raise RuntimeError(f"{name}: blank or unrendered screenshot ({colors} colors)")
            if not screenshot.is_file() or screenshot.stat().st_size < 5000:
                raise RuntimeError(f"{name}: native screenshot missing or empty")
            process.wait(timeout=25)
            if process.returncode:
                raise RuntimeError(f"{name}: fixture failed: {log.read_text()}")
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
            log_stream.close()
    print(f"Captured native {name}", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    # Use a managed X11 desktop rather than a bare X server.
    manager = None
    manager_log = None
    if sys.platform != "darwin":
        manager_log = (output / "desktop.log").open("w", encoding="utf-8")
        manager = subprocess.Popen(["/usr/bin/openbox"], stdout=manager_log,
                                   stderr=manager_log)
    try:
        if manager is not None:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                property_value = subprocess.check_output(
                    ["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"], text=True)
                if "window id #" in property_value:
                    break
                if manager.poll() is not None:
                    raise RuntimeError("Openbox exited before initializing the desktop: "
                                       + (output / "desktop.log").read_text())
                time.sleep(0.1)
            else:
                raise TimeoutError("Openbox did not initialize the desktop")
        for dark in (False, True):
            for width in (1076, 500):
                for anchor in ("", "lists-and-tasks", "code-and-diff"):
                    capture(binary, output, dark, width, 1, anchor)
    finally:
        if manager is not None:
            manager.terminate()
            manager.wait(timeout=10)
            manager_log.close()


if __name__ == "__main__":
    main()
