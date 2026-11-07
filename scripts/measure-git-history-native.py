"""Measure the production History renderer using a prepared native fixture."""

import argparse
import json
from pathlib import Path
import subprocess
import time


def measure(executable: Path, case: Path, label: str, runs: int) -> None:
    base = json.loads(case.read_text(encoding="utf-8-sig"))
    config = case.parent / (case.stem + "-config")
    results = []
    for index in range(1, runs + 1):
        output = case.parent / f"{label}-run-{index}.json"
        if output.exists():
            raise SystemExit(f"Refusing to overwrite an existing measurement: {output}")
        options = dict(base, output=str(output.resolve()), hold_seconds=5)
        options_path = case.parent / f"{label}-options-{index}.json"
        options_path.write_text(json.dumps(options, indent=2), encoding="utf-8")
        log = case.parent / f"{label}-run-{index}.log"
        with log.open("wb") as stream:
            process = subprocess.Popen(
                [str(executable.resolve()), "--config-dir", str(config.resolve()),
                 "--document-visual", str(options_path.resolve())],
                stdout=stream, stderr=stream,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            try:
                deadline = time.monotonic() + 40
                while not output.exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError(log.read_text(encoding="utf-8", errors="replace"))
                    time.sleep(0.1)
                result = json.loads(output.read_text(encoding="utf-8"))
                if result["history_commits"] != 5000:
                    raise RuntimeError("The performance fixture must load 5,000 commits")
                for key in ("width", "height", "dark", "history_width", "history_height"):
                    if result[key] != base[key]:
                        raise RuntimeError(f"Fixture state changed: {key}={result[key]!r}, expected {base[key]!r}")
                memory = subprocess.check_output(
                    ["powershell", "-NoProfile", "-Command",
                     f"Get-Process -Id {process.pid} | "
                     "Select-Object PrivateMemorySize64,WorkingSet64 | ConvertTo-Json"],
                )
                result["process_memory"] = json.loads(memory.decode("utf-8-sig"))
                result["run"] = index
                result["label"] = label
                output.write_text(json.dumps(result, indent=2), encoding="utf-8")
                results.append(result)
                print(json.dumps({
                    "run": index, "label": label, **result["history_profile"],
                    **result["process_memory"],
                }), flush=True)
                process.wait(timeout=10)
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)
    summary = case.parent / f"{label}-summary.json"
    summary.write_text(json.dumps(results, indent=2), encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("case", type=Path)
    parser.add_argument("--label", required=True)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    measure(args.executable, args.case, args.label, args.runs)
