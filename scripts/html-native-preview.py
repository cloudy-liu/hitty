#!/usr/bin/env python3
"""Run the real document-panel HTML fixture and retain native evidence."""

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


def main():
    binary = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="ctty7-html-") as temporary:
        scratch = Path(temporary)
        # A literal # in the filename must stay distinct from URL fragments.
        fixture = scratch / "HTML 页面 with spaces"
        shutil.copytree(Path(__file__).resolve().parents[1] / "tests/fixtures/html-preview", fixture)
        pages = fixture / "pages"
        pages.mkdir()
        source = pages / "report #1.HTML"
        # Keep the reference page's content, with resources in a parent folder.
        for original, target in [(fixture / "report.html", source),
                                 (fixture / "next.htm", pages / "next.htm")]:
            target.write_text(original.read_text(encoding="utf-8").replace("assets/", "../assets/"),
                              encoding="utf-8")
        options = scratch / "options.json"
        options.write_text(json.dumps({"source": str(source), "output": str(output / "result.json"),
                                       "hold_seconds": int(sys.argv[3]) if len(sys.argv) > 3 else 10}), encoding="utf-8")
        log_path = output / "native.log"
        with log_path.open("w", encoding="utf-8") as log:
            process = subprocess.Popen([str(binary), "--config-dir", str(scratch / "config"),
                                        "--html-preview-visual", str(options)], stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 90
                captured = False
                source_sampled = False
                while process.poll() is None:
                    text = log_path.read_text(encoding="utf-8", errors="replace")
                    if not captured and "NATIVE_HTML_READY_FOR_CAPTURE" in text:
                        if sys.platform == "darwin":
                            # GPUI's GPU screenshot excludes native WKWebView children.
                            subprocess.run(["screencapture", "-x", str(output / "desktop.png")], check=True)
                        elif sys.platform == "win32":
                            measure_memory(process.pid, "active-preview", output)
                        captured = True
                    if not source_sampled and "NATIVE_HTML_SOURCE_RESTORED" in text:
                        if sys.platform == "win32":
                            measure_memory(process.pid, "source-restored", output)
                        source_sampled = True
                    if time.monotonic() > deadline:
                        raise TimeoutError("Native HTML acceptance timed out")
                    time.sleep(0.1)
                if process.returncode != 0 or "NATIVE_HTML_PASSED" not in log_path.read_text(encoding="utf-8", errors="replace"):
                    raise RuntimeError(f"Native HTML acceptance failed; see {log_path}")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
        print((output / "result.json").read_text(encoding="utf-8"))


def measure_memory(pid, label, output):
    script = Path(__file__).resolve().parent / "bench/html-preview-memory.ps1"
    subprocess.run(["powershell.exe", "-NoProfile", "-File", str(script),
                    "-RootProcessId", str(pid), "-Label", label,
                    "-Output", str(output / f"{label}-memory.json"), "-Samples", "3"],
                   check=True, creationflags=subprocess.CREATE_NO_WINDOW)


if __name__ == "__main__":
    main()
