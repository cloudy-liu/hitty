"""Verify updater replacement and consecutive restore with isolated agent fixtures.

Usage: python scripts/agent-resume-update-acceptance.py BIN_DIR BASELINE_ZIP OUTPUT_DIR
The archive supplies bundled Windows runtime assets; the selected build supplies
all executables. No installed application or real agent data is changed.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile

base = Path(tempfile.gettempdir())
binary_dir = Path(sys.argv[1]).resolve()
baseline = Path(sys.argv[2]).resolve()
output = Path(sys.argv[3]).resolve()
output.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix="acceptance-", dir=base))
install, stage, config, home, fake_bin, project = (root/n for n in ("install","stage","profile","home","agent-bin","project"))
for p in (install,stage,config,home,fake_bin,project):
    p.mkdir()
with zipfile.ZipFile(baseline) as z:
    z.extractall(install)
for name in ("hitty-app.exe","hitty.exe","hitty-updater.exe"):
    shutil.copyfile(binary_dir/name, install/name)
# This is a private test package of the fixed build, with the same version.
# It exercises the installer after update selection and package preparation.
archive=stage/"hitty-0.3.1-windows-x86_64.zip"
with zipfile.ZipFile(archive,"w",zipfile.ZIP_DEFLATED,compresslevel=1) as z:
    for f in install.rglob("*"):
        if f.is_file():
            z.write(f, f.relative_to(install))
checksum=hashlib.sha256(archive.read_bytes()).hexdigest()
manifest=stage/"checksums.txt"
manifest.write_text(checksum+"  "+archive.name+"\n",encoding="ascii")
helper=root/"hitty-updater.exe"
shutil.copyfile(install/"hitty-updater.exe",helper)
app_path=install/"hitty-app.exe"
cli_path=install/"hitty.exe"
env={k:v for k,v in os.environ.items() if k!="HITTY" and not k.startswith("HITTY_")}
env.update(HITTY_CONFIG_DIR=str(config), HITTY_DATA_DIR=str(config), HOME=str(home),USERPROFILE=str(home),
           APPDATA=str(home/"roaming"),LOCALAPPDATA=str(home/"local"),
           CODEX_HOME=str(home/".codex"),CLAUDE_CONFIG_DIR=str(home/".claude"))
env["PATH"]=str(fake_bin)+os.pathsep+env["PATH"]
(config/"config.json").write_text(json.dumps(dict(
    check_for_updates=False,auto_download_updates=False,install_cli_on_path=False,show_tray_icon=False,
    restore_session=True,restore_agent_sessions=True,gui_language="en",env={"PATH":env["PATH"]},
    shell=dict(program="cmd.exe",args=["/d","/k"]))),encoding="utf-8")
(config/"window.json").write_text('{"x":80,"y":80,"width":1150,"height":750}',encoding="ascii")
cases={
    "codex-first":("codex","0199c3f2-1b0e-7c3a-9f21-6d4b8e2a5c17"),
    "codex-second":("codex","c2ef37e9-62b8-4ee8-a031-4e9c596544c0"),
    "claude":("claude","cf074a5a-8f41-4442-bb8e-baf7b3400113"),
}
for agent in ("codex","claude"):
    body=[
        "@echo off",
        f'>>"{root / (agent+"-%HITTY_PANE%.args")}" echo [%*]',
         'echo %* | findstr /c:"resume " >nul && goto wait',
        'if "%~1"=="-c" goto initial',
        'if not "%~1"=="" goto wait',
        ':initial',
        "echo \x1b]777;notify;hitty://cli-agent;"+json.dumps(dict(agent=agent,event="prompt-submit"))+"\x07",
        ":wait_hook",
        "ping -n 2 127.0.0.1 > nul",
        f'if not exist "{root/"hook-%HITTY_PANE%.json"}" goto wait_hook',
        f'"{app_path}" agent-hook {agent} session-start < "{root/"hook-%HITTY_PANE%.json"}"',
        f'"{app_path}" agent-hook {agent} prompt-submit < "{root/"hook-%HITTY_PANE%.json"}"',
        f'>"{root/"hook-%HITTY_PANE%.sent"}" echo delivered',
        ":wait",
        "ping -n 240 127.0.0.1 > nul",
    ]
    (fake_bin/(agent+".cmd")).write_text("\n".join(body)+"\n",encoding="ascii",newline="\r\n")

def cli(*args,check=True):
    r=subprocess.run([str(cli_path),*args],env=env,capture_output=True,text=True,encoding="utf-8",errors="replace",
                     timeout=15,creationflags=subprocess.CREATE_NO_WINDOW)
    if check and r.returncode:
        raise RuntimeError((args,r.returncode,r.stdout,r.stderr))
    return r.stdout if r.returncode==0 else ""

def panes():
    return json.loads(cli("pane","ls","--json",check=False) or "{}").get("panes",[])

def facts():
    try: return json.loads((config/"machine.json").read_text(encoding="utf-8"))["panes"]
    except (FileNotFoundError,json.JSONDecodeError): return []

def wait_for(probe,label,seconds=25):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        r=probe()
        if r: return r
        time.sleep(.25)
    raise AssertionError(label)

def launch():
    return subprocess.Popen([str(app_path),"--config-dir",str(config)],env=env,cwd=project,
                            creationflags=subprocess.CREATE_NO_WINDOW)

def app_pids():
    query="""Get-CimInstance Win32_Process -Filter "Name='hitty-app.exe'" | Where-Object {$_.ExecutablePath -eq $env:HITTY_FIXTURE_EXE} | ForEach-Object {$_.ProcessId}"""
    r=subprocess.run(["powershell.exe","-NoProfile","-Command",query],env=dict(env,HITTY_FIXTURE_EXE=str(app_path)),
                     capture_output=True,text=True,timeout=15)
    return [int(x) for x in r.stdout.splitlines() if x.isdigit()]

def stop():
    for pid in app_pids():
        subprocess.run(["taskkill.exe","/pid",str(pid),"/f"],capture_output=True,timeout=10)
    cli("server","stop",check=False)

def calls():
    return [f.read_text().strip() for f in root.glob("*.args")]

def ids_saved():
    saved={(p.get("agent") or {}).get("session_id") for p in facts() if p.get("live")}
    return all(sid in saved for _,sid in cases.values())

def assert_restored(expected):
    live=wait_for(lambda: [p for p in panes() if p.get("live")] if len(panes())==5 else None,"five live panes")
    wait_for(ids_saved,"all original session IDs restored")
    for agent,sid in cases.values():
        needle=("resume " if agent=="codex" else "--resume ")+sid
        wait_for(lambda: sum(needle in c for c in calls())==expected, f"one resume per agent per restoration: {sid}")
    return live

report=dict(profile=str(root),version="0.3.1",package_sha256=checksum,fixture="two Codex and one Claude, same directory",
            package="private fixed debug build, same-version replacement")
print(json.dumps(report),flush=True)
app=launch()
try:
    first=wait_for(panes,"startup pane")[0]
    targets={}
    for label,(agent,sid) in cases.items():
        known={p["pane"] for p in panes()}
        cli("tab","new",str(first["workspace"]),"--cwd",str(project))
        pane=wait_for(lambda: next((p["pane"] for p in panes() if p["pane"] not in known),None),"new pane")
        targets[label]=pane
        wait_for(lambda: cli("capture",f"%{pane}","--plain",check=False).strip(),"shell prompt")
        cli("send",f"%{pane}",agent,"--enter")
        wait_for(lambda: (root/f"{agent}-{pane}.args").exists(),"fixture launch")
    cli("tab","new",str(first["workspace"]),"--cwd",str(project))
    wait_for(lambda: all(any(p["id"]==targets[label] and (p.get("agent") or {}).get("agent")=="Codex" for p in facts()) for label in ("codex-first","codex-second")),"both Codex detected before hooks")
    for label,pane in targets.items():
        (root/f"hook-{pane}.json").write_text(json.dumps(dict(session_id=cases[label][1],
            cwd=project.as_posix(),source="startup",prompt="isolated acceptance")),encoding="ascii")
    wait_for(ids_saved,"each pane saved its exact ID")
    (root/"machine-before.json").write_bytes((config/"machine.json").read_bytes())
    print("PASS pre-install identity capture",flush=True)
    outcome=root/"outcome.json"
    updater=subprocess.Popen([str(helper),"install-portable",str(app.pid),str(archive),str(manifest),
        archive.name,str(install),"0.3.1",str(root/"update.log"),str(stage),"--config-dir",str(config),
        "--result-file",str(outcome)],env=env,creationflags=subprocess.CREATE_NO_WINDOW)
    app.terminate(); app.wait(timeout=15)
    assert updater.wait(timeout=100)==0,(root/"update.log").read_text(encoding="utf-8")
    result=json.loads(outcome.read_text(encoding="utf-8"))
    assert result["ok"],result
    assert_restored(1)
    (root/"machine-after-update.json").write_bytes((config/"machine.json").read_bytes())
    print("PASS portable replacement restored all three exact conversations",flush=True)
    stop()
    app=launch()
    assert_restored(2)
    (root/"machine-after-second-restart.json").write_bytes((config/"machine.json").read_bytes())
    print("PASS second restart before next agent hook retained all three identities",flush=True)
    report.update(identity_capture="passed",portable_update="passed",five_panes_restored="passed",
                  codex_same_directory="passed",claude_restore="passed",second_restart="passed",
                  agent_calls=calls(),outcome=result)
    (output/"result.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    for name in ("machine-before.json","machine-after-update.json","machine-after-second-restart.json","update.log"):
        shutil.copyfile(root/name,output/name)
    print(json.dumps(report),flush=True)
finally:
    if app.poll() is None:
        app.terminate();app.wait(timeout=10)
    stop()

