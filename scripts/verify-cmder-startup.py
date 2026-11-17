"""Compare patched Cmder library behavior against the copied source baseline."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixtures", type=Path)
    args = parser.parse_args()
    roots = [args.fixtures.resolve() / v for v in ("baseline", "optimized")]
    with tempfile.TemporaryDirectory(prefix="hitty-cmder-equivalence-") as tmp:
        temporary = Path(tmp)
        commands = temporary / "commands with spaces"
        commands.mkdir()
        (commands / "fixture.cmd").write_text("@echo off\n", encoding="ascii")
        checks = 0

        def run(root, body):
            script = temporary / "check.cmd"
            script.write_text('@echo off\nset "debug_output=0"\nset "verbose_output=0"\n'
                              'set "fast_init=0"\n' + body, encoding="utf-8")
            env = os.environ | {"CMDER_ROOT": str(root)}
            result = subprocess.run([os.environ["ComSpec"], "/D", "/C", str(script)],
                                    env=env, capture_output=True, timeout=15,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
            if result.returncode:
                raise RuntimeError(f"fixture failed with exit {result.returncode}")
            return [line.replace(str(root).encode(), b"<ROOT>")
                    for line in result.stdout.splitlines() if line.startswith(b"RESULT=")]

        system = str(Path(os.environ["SystemRoot"]) / "System32")
        for placement in ("", "append"):
            for entries in ([system], [str(commands), system], [system, str(commands)],
                            [system, str(commands), system], [str(commands).upper(), system],
                            [system, str(commands) + "-other"]):
                body = ('call "%CMDER_ROOT%\\vendor\\lib\\lib_path.cmd"\n'
                        f'set "PATH={";".join(entries)}"\n'
                        f'%lib_path% enhance_path "{commands}" {placement}\n'
                        'echo RESULT=%PATH%\n')
                before, after = [run(root, body) for root in roots]
                assert before and before == after, (placement, entries, before, after)
                checks += 1
        # COMSPEC also starts CMD's pipeline workers, so fake nonexistent paths
        # cannot test detection. Rename copies of the real interpreter instead.
        shells = temporary / "shells with spaces"
        shells.mkdir()
        comspecs = [os.environ["ComSpec"], os.environ["ComSpec"].upper()]
        for name in ("cmd.exe", "tcc.exe", "tccle.exe", "custom.exe"):
            path = shells / name
            shutil.copy2(os.environ["ComSpec"], path)
            comspecs.append(str(path))
        for comspec in comspecs:
            body = (f'set "ComSpec={comspec}"\n'
                    'call "%CMDER_ROOT%\\vendor\\lib\\lib_base.cmd" cmder_shell\n'
                    'echo RESULT=%CMDER_SHELL%;%CMDER_CLINK%;%CMDER_ALIASES%\n')
            before, after = [run(root, body) for root in roots]
            assert before and before == after, (before, after)
            checks += 1
        for content in (b";= Add aliases below here\r\nfixture=echo hello\r\n",
                        b"fixture=echo legacy\r\n", b"fixture=echo Add\r\n"):
            outcomes = []
            for index, root in enumerate(roots):
                aliases = temporary / f"aliases-{index}.cmd"
                aliases.write_bytes(content)
                backup = Path(str(aliases) + ".old_format")
                backup.unlink(missing_ok=True)
                run(root, f'set "user_aliases={aliases}"\n'
                    'call "%CMDER_ROOT%\\vendor\\lib\\lib_base.cmd" update_legacy_aliases\n')
                outcomes.append((aliases.read_bytes(), backup.read_bytes() if backup.exists() else None))
            assert outcomes[0] == outcomes[1], "legacy alias migration changed"
            checks += 1
        for populated in (False, True):
            profiles = temporary / ("populated" if populated else "empty")
            profiles.mkdir()
            if populated:
                (profiles / "one.cmd").write_text('@echo off\nset "HITTY_PROFILE_FIXTURE=ran"\n')
            body = ('call "%CMDER_ROOT%\\vendor\\lib\\lib_profile.cmd"\n'
                    f'%lib_profile% run_profile_d "{profiles}"\n'
                    'echo RESULT=%HITTY_PROFILE_FIXTURE%;%CD%;%ERRORLEVEL%\n')
            before, after = [run(root, body) for root in roots]
            assert before and before == after, "profile script execution changed"
            checks += 1

        # Directory checks must retain DIR's filtering and its match
        # semantics, including executable-looking directories and backups.
        for name, filename, kind in (
            ("empty", None, None), ("exe", "tool.exe", None),
            ("backup", "tool.EXE.bak", None), ("directory", "tool.cmd", "directory"),
            ("hidden", "tool.exe", "hidden"), ("system", "tool.cmd", "system"),
            ("hidden-directory", "tool.exe", "hidden-directory"),
            ("system-directory", "tool.exe", "system-directory"),
            ("empty.COM", None, None), ("spaces", "command with spaces.PS1", None),
        ):
            directory = temporary / ("path-" + name)
            directory.mkdir()
            if filename:
                entry = directory / filename
                if kind and "directory" in kind:
                    entry.mkdir()
                else:
                    entry.touch()
                if kind and (kind.startswith("hidden") or kind.startswith("system")):
                    subprocess.run(["attrib", "+h" if kind.startswith("hidden") else "+s", str(entry)],
                                   check=True, capture_output=True, creationflags=subprocess.CREATE_NO_WINDOW)
            for fast in (0, 1):
                for mode in ("", "/A", "/A:D"):
                    body = (f'set "fast_init={fast}"\nset "DIRCMD={mode}"\n'
                            'call "%CMDER_ROOT%\\vendor\\lib\\lib_path.cmd"\n'
                            f'%lib_path% enhance_path "{directory}"\n'
                            'echo RESULT=%add_to_path%;%PATH%;%ERRORLEVEL%\n')
                    before, after = [run(root, body) for root in roots]
                    assert before and before == after, (name, fast, mode, before, after)
                    checks += 1
        for library in ("lib_git", "lib_path", "lib_profile", "lib_console"):
            for debug in (0, 1):
                body = (f'set "debug_output={debug}"\nset "verbose_output={debug}"\n'
                        f'call "%CMDER_ROOT%\\vendor\\lib\\{library}.cmd"\n'
                        'echo RESULT=%lib_base%;%lib_console%;%print_debug%;%print_verbose%;%print_error%\n')
                before, after = [run(root, body) for root in roots]
                assert before and before == after, (library, debug, before, after)
                checks += 1
        for user, vendored in (("1.2.3.4", "1.2.3.4"), ("2.0.0.0", "1.9.9.9"),
                               ("1.2.3.4", "1.2.3.5"), ("1.2.4.0", "1.2.3.9")):
            body = ('call "%CMDER_ROOT%\\vendor\\lib\\lib_git.cmd"\n'
                    f'%lib_git% validate_version USER {user}\n'
                    f'%lib_git% validate_version VENDORED {vendored}\n'
                    '%lib_git% compare_versions USER VENDORED\n'
                    'echo RESULT=%ERRORLEVEL%;%USER_MAJOR%;%USER_MINOR%;%USER_PATCH%;%USER_BUILD%\n')
            before, after = [run(root, body) for root in roots]
            assert before and before == after, (user, vendored, before, after)
            checks += 1
        print(f"{checks} baseline/optimized behavior comparisons passed")


if __name__ == "__main__":
    main()
