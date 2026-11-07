#!/usr/bin/env python3
"""Check real Windows tooltip occlusion, dismissal and project breadcrumbs."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    if sys.platform != "win32":
        raise SystemExit("This acceptance check requires Windows and WebView2.")
    binary = Path(sys.argv[1]).resolve(strict=True)
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="ctty7-tooltip-", ignore_cleanup_errors=True) as scratch:
        for label, disabled in [("unclipped", True), ("fixed", False)]:
            result = output / f"{label}.json"
            result.unlink(missing_ok=True)
            options = {
                "root": str(root),
                "files": ["tests/fixtures/html-preview/report.html"],
                "output": str(result),
                "width": 800,
                "height": 480,
                "right_panel": True,
                "preview": True,
                "check_native_tooltip": True,
                "disable_native_tooltip_clip": disabled,
                "assert_native_restored": not disabled,
                "assert_breadcrumb": str(Path(root.name) / "tests/fixtures/html-preview/report.html"),
                "hold_seconds": 0,
            }
            settings = Path(scratch) / f"{label}.json"
            settings.write_text(json.dumps(options), encoding="utf-8")
            log = output / f"{label}.log"
            with log.open("w", encoding="utf-8") as stream:
                process = subprocess.Popen(
                    [str(binary), "--config-dir", str(Path(scratch) / label),
                     "--document-visual", str(settings)],
                    stdout=stream, stderr=stream, creationflags=subprocess.CREATE_NO_WINDOW,
                )
                try:
                    code = process.wait(timeout=45)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                    raise RuntimeError(f"{label}: native acceptance timed out; see {log}")
            text = log.read_text(encoding="utf-8", errors="replace")
            if disabled:
                assert code != 0 and "native HTML child covers the tooltip" in text, \
                    f"The unclipped native child must reproduce the overlap; see {log}"
            else:
                assert code == 0 and result.is_file(), f"Native acceptance failed; see {log}"
                assert "NATIVE_TOOLTIP_REGION_RESTORED" in text, \
                    "Dismissing the tooltip must restore the native page region"
            print(f"PASS: {label}", flush=True)


if __name__ == "__main__":
    main()
