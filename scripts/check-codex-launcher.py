"""Verify native Windows Codex argument forwarding and batch caller return.

Usage: python scripts/check-codex-launcher.py PATH_TO_HITTY_EXE
Requires rustc, cmd.exe and Windows PowerShell; uses only temporary files.
"""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


with tempfile.TemporaryDirectory(prefix="hitty-native-launcher-") as directory:
    root = Path(directory)
    source = root / "argv.rs"
    source.write_text(
        'fn main() { for arg in std::env::args().skip(1) { '
        'for byte in arg.as_bytes() { print!("{byte:02x}"); } '
        'println!(); } std::process::exit(17); }',
        encoding="utf-8",
    )
    original = root / "original agent.exe"
    subprocess.run(["rustc", str(source), "-o", str(original)], check=True)
    bin_dir = root / "hitty-agent-bin"
    bin_dir.mkdir()
    launcher = bin_dir / "codex.exe"
    shutil.copyfile(sys.argv[1], launcher)
    env = dict(os.environ, HITTY_PANE="42", HITTY_CODEX_PROGRAM=str(original))
    arguments = ["resume", "exact-id", "a model", "literal %TEMP% & $x; 'quoted' 中文"]
    expected = ["-c", "shell_environment_policy.set.HITTY_PANE='42'", *arguments]

    def verify(command):
        result = subprocess.run(command, env=env, capture_output=True, check=False)
        assert result.returncode == 17, result
        actual = [bytes.fromhex(line).decode("utf-8") for line in result.stdout.decode().splitlines()]
        assert actual == expected, (actual, expected)

    verify([str(launcher), *arguments])
    quote = lambda value: "'" + value.replace("'", "''") + "'"
    verify(["powershell.exe", "-NoProfile", "-Command",
            "& " + " ".join(map(quote, [str(launcher), *arguments])) + "; exit $LASTEXITCODE"])
    # npm's PowerShell entry point accepts here-strings. Keep that route rather
    # than converting it to cmd.exe, whose native argument API rejects newlines.
    powershell_script = root / "original agent.ps1"
    powershell_script.write_text('& $env:HITTY_ARGV_FIXTURE @args\nexit $LASTEXITCODE\n', encoding="utf-8")
    env.update(HITTY_CODEX_PROGRAM=str(powershell_script), HITTY_ARGV_FIXTURE=str(original))
    arguments.append("line one\nline two")
    expected.append(arguments[-1])
    verify([str(launcher), *arguments])
    env["HITTY_CODEX_PROGRAM"] = str(original)
    caller = root / "caller.cmd"
    caller.write_text(f'@echo off\r\n"{launcher}" resume exact-id\r\necho CALLER_RETURNED\r\n', encoding="utf-8")
    result = subprocess.run(["cmd.exe", "/d", "/c", str(caller)], env=env, capture_output=True)
    assert b"CALLER_RETURNED" in result.stdout, result
    print("PASS native argv, PowerShell literal arguments, exit status and batch caller return")
