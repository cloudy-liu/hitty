"""Copy installed Cmder into two isolated fixtures; optimize only the second.

Preserves Git selection, PATH rules, alias migrations, Clink and user scripts.
The original installation is read-only. Run measure-pane-startup on options.json
in each fixture. Neither fixture is an installed replacement for Cmder.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def replace_once(path, old, new):
    source = path.read_bytes()
    old, new = old.replace("\n", "\r\n").encode(), new.replace("\n", "\r\n").encode()
    if source.count(old) != 1:
        raise ValueError(f"Unsupported Cmder source in {path.name}; expected one matching block")
    path.write_bytes(source.replace(old, new))


def optimize(root):
    vendor = root / "vendor"
    replace_once(vendor / "lib/lib_profile.cmd",
        '  pushd "%~1"\n  for /f "usebackq" %%x in ( `dir /b *.bat *.cmd 2^>nul` ) do (',
        '  if not exist "%~1\\*.bat" if not exist "%~1\\*.cmd" exit /b 0\n'
        '  pushd "%~1"\n  for /f "usebackq" %%x in ( `dir /b *.bat *.cmd 2^>nul` ) do (')

    # These no-argument includes only refresh the following variables. Keep
    # refreshing them on each call, without re-entering three batch files.
    base = 'set lib_base=call "%~dp0lib_base.cmd"'
    console = '\n'.join((
        'set lib_console=call "%~dp0lib_console.cmd"',
        'set print_debug=if %debug_output% gtr 0 %lib_console% debug_output',
        'set print_verbose=if %verbose_output% gtr 0 %lib_console% verbose_output',
        'set print_error=%lib_console% show_error',
    ))
    dispatch = '\n'.join((
        'if "%~1" == "/h" (', '%lib_base% help "%~0"',
        ') else if "%1" neq "" (', 'call :%*', ')', 'exit /b',
    ))
    # Fail closed if an installed library adds work to its no-argument
    # include. Inlining that include must not discard user modifications.
    for name, expected in (
        ("lib_base.cmd", '@echo off\n' + base + '\n' + dispatch),
        ("lib_console.cmd", '@echo off\ncall "%~dp0lib_base.cmd"\n' + console +
         '\nif "%fast_init%" == "1" exit /b\n' + dispatch),
    ):
        prelude = (vendor / "lib" / name).read_text().split('\nexit /b\n', 1)[0] + '\nexit /b'
        commands = [line.strip() for line in prelude.splitlines()
                    if line.strip() and not line.lstrip().startswith('::')]
        if commands != expected.splitlines():
            raise ValueError(f"Unsupported no-argument Cmder include in {name}")
    for library, include in (("lib_git.cmd", "lib_console.cmd"),
                             ("lib_path.cmd", "lib_console"),
                             ("lib_profile.cmd", "lib_console")):
        replace_once(vendor / "lib" / library,
                     'call "%~dp0lib_base.cmd"\n' + f'call "%%~dp0{include}"',
                     base + '\n' + console)
    replace_once(vendor / "lib/lib_console.cmd", 'call "%~dp0lib_base.cmd"', base)
    for old, new in (("%lib_git% read_version USER", "call :read_version USER"),
                     ("%lib_git% validate_version USER", "call :validate_version USER"),
                     ("!lib_git! compare_versions USER VENDORED", "call :compare_versions USER VENDORED")):
        replace_once(vendor / "lib/lib_git.cmd", old, new)
    replace_once(vendor / "lib/lib_base.cmd",
        '    echo %comspec% | %WINDIR%\\System32\\find /i "\\cmd.exe" > nul && set "CMDER_SHELL=cmd"\n'
        '    echo %comspec% | %WINDIR%\\System32\\find /i "\\tcc.exe" > nul && set "CMDER_SHELL=tcc"\n'
        '    echo %comspec% | %WINDIR%\\System32\\find /i "\\tccle" > nul && set "CMDER_SHELL=tccle"',
        '    if /i not "%comspec:\\cmd.exe=%" == "%comspec%" set "CMDER_SHELL=cmd"\n'
        '    if /i not "%comspec:\\tcc.exe=%" == "%comspec%" set "CMDER_SHELL=tcc"\n'
        '    if /i not "%comspec:\\tccle=%" == "%comspec%" set "CMDER_SHELL=tccle"')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cmder", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cwd", type=Path, default=Path.cwd())
    parser.add_argument("--samples", type=int, default=20)
    parser.add_argument("--budget-ms", type=float)
    args = parser.parse_args()
    source, output = args.cmder.resolve(), args.output.resolve()
    if os.name != "nt":
        parser.error("Windows is required")
    git = source / "vendor/git-for-windows"
    if (git / "post-install.bat").exists():
        raise ValueError("Finish the installed Git setup before creating a read-only fixture")
    if source == output or source in output.parents:
        raise ValueError("Create the fixtures outside the installed Cmder directory")
    output.mkdir(parents=True, exist_ok=False)
    for variant in ("baseline", "optimized"):
        root = output / variant
        vendor = root / "vendor"
        vendor.mkdir(parents=True)
        for name in ("bin", "lib", "clink", "clink-completions"):
            shutil.copytree(source / "vendor" / name, vendor / name)
        for file in (source / "vendor").iterdir():
            if file.is_file():
                shutil.copy2(file, vendor / file.name)
        shutil.copytree(source / "config", root / "config",
                        ignore=shutil.ignore_patterns("*history*", "*.log"))
        shutil.copytree(source / "bin", root / "bin")
        for file in source.glob("LICENSE*"):
            shutil.copy2(file, root / file.name)
        if git.is_dir():
            # A directory junction avoids copying the large, unmodified Git runtime.
            # Pass paths as environment data, never PowerShell source.
            command = "$null = New-Item -ItemType Junction -Path $env:HITTY_FIXTURE_LINK -Target $env:HITTY_FIXTURE_GIT"
            env = os.environ | {"HITTY_FIXTURE_LINK": str(vendor / "git-for-windows"),
                                "HITTY_FIXTURE_GIT": str(git)}
            subprocess.run(["powershell.exe", "-NoProfile", "-Command", command],
                           env=env, check=True, capture_output=True, creationflags=subprocess.CREATE_NO_WINDOW)
        with (root / "config/user_aliases.cmd").open("ab") as aliases:
            aliases.write(b"\r\nhitty-startup-fixture=set /a 73000+421\r\n")
        if variant == "optimized":
            optimize(root)
        profile = root / "hitty-profile"
        profile.mkdir()
        config = {"restore_agent_sessions": True, "pane_engine": "native",
                  "env": {"CMDER_ROOT": str(root), "CMDER_USER_CONFIG": str(root / "config")}}
        (profile / "config.json").write_text(json.dumps(config), encoding="utf-8")
        options = {"config_dir": str(profile), "cwd": str(args.cwd.resolve()),
                   "shell": {"program": os.environ["ComSpec"],
                             "args": ["/D", "/K", str(vendor / "init.bat")],
                             "args_are_hitty_defaults": False},
                   "samples": args.samples, "output": str(root / "measurements.json"),
                   "budget_ms": args.budget_ms}
        (root / "options.json").write_text(json.dumps(options), encoding="utf-8")
    records = {}
    for name in ("init.bat", "lib/lib_base.cmd", "lib/lib_console.cmd", "lib/lib_git.cmd", "lib/lib_path.cmd", "lib/lib_profile.cmd"):
        records[name] = {v: hashlib.sha256((output / v / "vendor" / name).read_bytes()).hexdigest()
                         for v in ("baseline", "optimized")}
    (output / "source-hashes.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()
