"""Compare patched Cmder library behavior against the copied source baseline."""
import argparse
import os
from pathlib import Path
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
            return [line for line in result.stdout.splitlines() if line.startswith(b"RESULT=")]

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
        for comspec in (os.environ["ComSpec"], os.environ["ComSpec"].upper()):
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
        print(f"{checks} baseline/optimized behavior comparisons passed")


if __name__ == "__main__":
    main()
