"""Run two fresh native fixtures with identical input and build metadata."""
import hashlib, json, pathlib, subprocess, sys, time

base = pathlib.Path('D:/gh-prj/ctty7/dist/markdown-performance')
source = pathlib.Path('D:/gh-prj/ctty7/dist/local-test/markdown-stress-test.md')
trial = int(sys.argv[1])
for revision in ('before', 'final'):
    name = f'{revision}-final-{trial}'
    exe = base / revision / 'tty7-app.exe'
    options = dict(source=source.as_posix(), width=1200, height=850, scale=1,
                   dark=True, hold_seconds=1, profile_output=(base / f'{name}.json').as_posix())
    options_path = base / f'{name}-options.json'
    options_path.write_text(json.dumps(options, indent=2), encoding='utf-8')
    metadata = dict(input_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                    input_bytes=source.stat().st_size,
                    binary_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),
                    started_unix=time.time(), options=options,
                    build='optimized dev: debug=0, incremental=false, opt-level=2; gpui and gpui-component opt-level=3')
    with (base / f'{name}-stdout.log').open('wb') as out, (base / f'{name}-stderr.log').open('wb') as err:
        process = subprocess.Popen([str(exe), '--config-dir', str(base / revision / f'profile-{trial}'),
                                    '--markdown-visual', str(options_path)], stdout=out, stderr=err)
        code = process.wait(timeout=55)
    metadata['exit_code'] = code
    (base / f'{name}-metadata.json').write_text(json.dumps(metadata, indent=2), encoding='utf-8')
    if code or not (base / f'{name}.json').exists():
        print((base / f'{name}-stderr.log').read_text(encoding='utf-8', errors='replace')[-4000:])
        raise SystemExit(f'{name} failed: {code}')
    result = json.loads((base / f'{name}.json').read_text(encoding='utf-8'))
    print(name, {key: result[key] for key in ('scroll_cpu_frame', 'expanded_drag_cpu_frame', 'all_content_ready_ms')}, flush=True)
