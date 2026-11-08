"""Compare the original renderer, previous canvas, and GitHub-style scroll view."""
import hashlib
import json
import pathlib
import subprocess
import sys
import time

base = pathlib.Path('D:/gh-prj/ctty7/dist/markdown-performance')
bundle = base.parent / 'local-test-20261014-github-mermaid-controls'
source = pathlib.Path('C:/Users/cloudy/.codex/worktrees/compose-open-prs/ctty7/tests/fixtures/markdown/large-navigation.md')
trial = int(sys.argv[1])
for revision, exe in (
    ('original', base / 'before/tty7-app.exe'),
    ('canvas', base / 'final/tty7-app.exe'),
    ('github', bundle / 'tty7-app.exe'),
):
    name = f'github-comparison-{revision}-{trial}'
    options = dict(source=source.as_posix(), width=1200, height=850, scale=1,
                   dark=True, hold_seconds=1, profile_output=(base / f'{name}.json').as_posix())
    options_path = base / f'{name}-options.json'
    options_path.write_text(json.dumps(options, indent=2), encoding='utf-8')
    metadata = dict(input_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                    input_bytes=source.stat().st_size,
                    binary_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),
                    started_unix=time.time(), options=options,
                    window_visibility='hidden; native CPU drawing is explicitly driven and verified',
                    build='optimized dev: debug=0, incremental=false, opt-level=2; gpui and gpui-component opt-level=3')
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    with (base / f'{name}-stdout.log').open('wb') as out, (base / f'{name}-stderr.log').open('wb') as err:
        process = subprocess.Popen([str(exe), '--config-dir', str(base / f'{name}-profile'),
                                    '--markdown-visual', str(options_path)], stdout=out, stderr=err,
                                   creationflags=subprocess.CREATE_NO_WINDOW, startupinfo=startup)
        code = process.wait(timeout=90)
    metadata['exit_code'] = code
    (base / f'{name}-metadata.json').write_text(json.dumps(metadata, indent=2), encoding='utf-8')
    if code or not (base / f'{name}.json').exists():
        print((base / f'{name}-stderr.log').read_text(encoding='utf-8', errors='replace')[-4000:])
        raise SystemExit(f'{name} failed: {code}')
    result = json.loads((base / f'{name}.json').read_text(encoding='utf-8'))
    print(name, {key: result[key] for key in ('scroll_cpu_frame', 'expanded_drag_cpu_frame', 'all_content_ready_ms')}, flush=True)
