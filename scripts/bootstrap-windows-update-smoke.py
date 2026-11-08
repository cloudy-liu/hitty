"""Verify a Windows portable update using two actual, unpublished/test packages."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import zipfile


def wait_for(probe, description, seconds=45):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = probe()
        if value:
            return value
        time.sleep(0.25)
    raise TimeoutError(description)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('old_zip', type=Path)
    parser.add_argument('new_zip', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--old-version', required=True)
    parser.add_argument('--new-version', required=True)
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('Windows packages require Windows')
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    install, stage, config = (root / name for name in ('installed', 'stage', 'config'))
    config.mkdir()
    stage.mkdir()
    with zipfile.ZipFile(args.old_zip) as archive:
        archive.extractall(install)
    updater = root / 'hitty-updater.exe'
    shutil.copyfile(install / 'hitty-updater.exe', updater)
    preferences = dict(check_for_updates=False, auto_download_updates=False,
                       install_cli_on_path=False, show_tray_icon=False, font_size=17,
                       shell=dict(program='powershell.exe', args=['-NoLogo', '-NoProfile']))
    (config / 'config.json').write_text(json.dumps(preferences), encoding='utf-8', newline='\n')
    (config / 'window.json').write_text(json.dumps(dict(x=40,y=40,width=1100,height=700)),
                                      encoding='utf-8', newline='\n')
    (install / 'user-note.txt').write_text('preserve this unmanaged file', encoding='utf-8')
    env = {k:v for k,v in os.environ.items() if k != 'HITTY' and not k.startswith('HITTY_')}
    env.update(HITTY_CONFIG_DIR=str(config), HITTY_DATA_DIR=str(config))

    def cli(*command, check=True):
        result = subprocess.run([str(install / 'hitty.exe'), *command], env=env,
                                capture_output=True, text=True, encoding='utf-8',
                                errors='replace', timeout=20)
        if check and result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return result

    def pane_list():
        result = cli('pane', 'ls', '--json', check=False)
        return [p for p in json.loads(result.stdout)['panes'] if p.get('live')] if result.returncode == 0 else None

    def output_contains(pane, marker):
        result = cli('capture', '%' + str(pane), '--plain', check=False)
        return result.stdout if result.returncode == 0 and marker in result.stdout else None

    def snapshot(pid, name):
        script = Path(__file__).resolve().parent / 'capture-native-window.ps1'
        result = subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass',
                                 '-File',str(script),'-ProcId',str(pid),'-Out',str(root/name),
                                 '-SettleMs','500'],capture_output=True,text=True,timeout=20)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)

    def installed_processes(stop=False):
        query = '''$p = Get-CimInstance Win32_Process -Filter "Name='hitty-app.exe'" |
Where-Object { $_.ExecutablePath -eq $env:HITTY_ACCEPTANCE_APP }
'''
        query += ('$p | ForEach-Object { Stop-Process -Id $_.ProcessId }' if stop else
                  '$p | Select-Object ProcessId,CommandLine | ConvertTo-Json -Compress')
        result = subprocess.run(['powershell','-NoProfile','-Command',query],
                                env=dict(env,HITTY_ACCEPTANCE_APP=str(install/'hitty-app.exe')),
                                capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=15)
        if result.returncode:
            raise RuntimeError(result.stderr)
        if stop or not result.stdout.strip():
            return []
        data = json.loads(result.stdout)
        return data if isinstance(data,list) else [data]

    app = subprocess.Popen([str(install/'hitty-app.exe'),'--config-dir',str(config)],
                           cwd=config,env=env,creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        assert cli('--version').stdout.strip() == 'hitty ' + args.old_version
        pane = wait_for(pane_list, 'old package initial pane')[0]
        wait_for(lambda: cli('capture','%'+str(pane['pane']),'--plain').stdout.strip(), 'shell prompt')
        cli('ws','rename',str(pane['workspace']),'Hitty update acceptance')
        cli('send','%'+str(pane['pane']),"Write-Output ('UPDATE_' + 'BEFORE_OK')",'--enter')
        wait_for(lambda: output_contains(pane['pane'],'UPDATE_BEFORE_OK'),'old package output')
        snapshot(app.pid,'before-update.png')
        checksum = hashlib.sha256(args.new_zip.read_bytes()).hexdigest()
        checksums=root/'checksums.txt'
        checksums.write_text(checksum+'  '+args.new_zip.name+'\n',encoding='ascii',newline='\n')
        invalid=root/'invalid-checksums.txt'
        invalid.write_text('0'*64+'  '+args.new_zip.name+'\n',encoding='ascii',newline='\n')
        bad = subprocess.run([str(updater),'verify-portable',str(args.new_zip.resolve()),str(invalid),
                              args.new_zip.name,args.new_version,str(root/'invalid-stage')],env=env,
                             capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=30)
        assert bad.returncode != 0, 'corrupt checksum was accepted'
        (root/'checksum-rejection.txt').write_text(bad.stdout+bad.stderr,encoding='utf-8',newline='\n')
        verify = subprocess.run([str(updater),'verify-portable',str(args.new_zip.resolve()),str(checksums),
                                 args.new_zip.name,args.new_version,str(stage)],env=env,timeout=45)
        assert verify.returncode == 0, 'valid package verification failed'
        outcome=root/'outcome.json'
        helper=subprocess.Popen([str(updater),'install-portable',str(app.pid),str(args.new_zip.resolve()),
                                  str(checksums),args.new_zip.name,str(install),args.new_version,
                                  str(root/'updater.log'),str(stage),'--config-dir',str(config),
                                  '--result-file',str(outcome)],env=env,creationflags=subprocess.CREATE_NO_WINDOW)
        app.terminate(); app.wait(timeout=15)
        assert helper.wait(timeout=90)==0,'portable update helper failed'
        result=json.loads(outcome.read_text(encoding='utf-8'))
        assert result['ok'] and result['version']==args.new_version,result
        assert cli('--version').stdout.strip() == 'hitty '+args.new_version
        restored=wait_for(pane_list,'updated package panes')
        match=next(p for p in restored if p['workspace']==pane['workspace'])
        wait_for(lambda: cli('capture','%'+str(match['pane']),'--plain',check=False).stdout.strip(), 'restored shell prompt')
        cli('send','%'+str(match['pane']),"Write-Output ('UPDATE_' + 'AFTER_OK')",'--enter')
        text=wait_for(lambda: output_contains(match['pane'],'UPDATE_AFTER_OK'),'updated package output')
        (root/'updated-terminal.txt').write_text(text,encoding='utf-8',newline='\n')
        assert json.loads((config/'config.json').read_text(encoding='utf-8'))['font_size']==17
        assert (install/'user-note.txt').read_text()=='preserve this unmanaged file'
        gui=wait_for(lambda: next((p for p in installed_processes() if '--daemon' not in p['CommandLine']),None),
                     'updater relaunched GUI')
        snapshot(gui['ProcessId'],'after-update.png')
        report=dict(old_version=args.old_version,new_version=args.new_version,package_sha256=checksum,
                    checksum_rejection='passed',update='passed',gui_relaunch='passed',
                    workspace_restoration='passed',custom_settings='passed',unmanaged_file='passed',outcome=result)
        (root/'result.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8',newline='\n')
        print(json.dumps(report),flush=True)
    finally:
        if app.poll() is None:
            app.terminate(); app.wait(timeout=15)
        cli('server','stop',check=False)
        installed_processes(stop=True)


if __name__ == '__main__':
    main()
