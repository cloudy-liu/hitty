"""Exercise real Windows Codex resume with an isolated, local mock provider.

Run with --bin-dir <workspace debug/release directory>. Requires installed Codex.
--expect-missing records the released baseline failure without claiming a pass.
--abrupt kills the isolated daemon tree instead of asking it to stop.
"""
import argparse
import csv
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time

args = argparse.ArgumentParser(description=__doc__)
args.add_argument("--bin-dir", type=Path, required=True)
args.add_argument("--output", type=Path, required=True)
args.add_argument("--expect-missing", action="store_true")
args.add_argument("--abrupt", action="store_true")
args.add_argument("--shell", choices=("cmd", "powershell"), default="cmd")
args.add_argument("--scratch-dir", type=Path, default=Path(tempfile.gettempdir()))
options = args.parse_args()
repo = Path(__file__).resolve().parents[1]
binaries = options.bin_dir.resolve()
root = Path(tempfile.mkdtemp(prefix="hc-", dir=options.scratch_dir))
evidence = options.output.resolve()
evidence.mkdir(parents=True, exist_ok=True)
(evidence/"result.json").unlink(missing_ok=True)
profile=root/"hitty"
home=root/"home"
codex_home=root/"codex-home"
project=root/"project"
for p in (profile,home,codex_home,project): p.mkdir()
env={k:v for k,v in os.environ.items() if k!="HITTY" and not k.startswith("HITTY_") and k not in ("CODEX_THREAD_ID","CODEX_SESSION_ID","CODEX_DAEMON_SHUTDOWN_SOCKET")}
env.update(HITTY_CONFIG_DIR=str(profile),HITTY_DATA_DIR=str(profile),CODEX_HOME=str(codex_home),
           USERPROFILE=str(home),HOME=str(home),APPDATA=str(home/"roaming"),LOCALAPPDATA=str(home/"local"))
app_path=binaries/"hitty-app.exe"
cli_path=binaries/"hitty.exe"
codex_cmd=shutil.which("codex.cmd")
requests=[]
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_GET(self):
        body=json.dumps(dict(object="list",data=[dict(id="fixture",object="model",owned_by="fixture")])).encode()
        self.send_response(200);self.send_header("Content-Type","application/json");self.send_header("Content-Length",str(len(body)));self.end_headers();self.wfile.write(body)
    def do_POST(self):
        body=self.rfile.read(int(self.headers.get("Content-Length","0")))
        marker=next((x for x in ("FIXTURE_A","FIXTURE_B") if x.encode() in body),"FIXTURE_OTHER")
        requests.append(marker)
        message=dict(id="msg_"+marker,type="message",role="assistant",content=[dict(type="output_text",text="ACK_"+marker,annotations=[])],status="completed")
        events=[
            ("response.created",dict(type="response.created",response=dict(id="resp_"+marker,status="in_progress",output=[]))),
            ("response.output_item.added",dict(type="response.output_item.added",output_index=0,item=dict(id=message["id"],type="message",role="assistant",content=[],status="in_progress"))),
            ("response.content_part.added",dict(type="response.content_part.added",item_id=message["id"],output_index=0,content_index=0,part=dict(type="output_text",text="",annotations=[]))),
            ("response.output_text.delta",dict(type="response.output_text.delta",item_id=message["id"],output_index=0,content_index=0,delta="ACK_"+marker)),
            ("response.output_text.done",dict(type="response.output_text.done",item_id=message["id"],output_index=0,content_index=0,text="ACK_"+marker)),
            ("response.content_part.done",dict(type="response.content_part.done",item_id=message["id"],output_index=0,content_index=0,part=message["content"][0])),
            ("response.output_item.done",dict(type="response.output_item.done",output_index=0,item=message)),
            ("response.completed",dict(type="response.completed",response=dict(id="resp_"+marker,status="completed",output=[message],usage=dict(input_tokens=10,output_tokens=5,total_tokens=15)))),
        ]
        payload="".join("event: "+event+"\ndata: "+json.dumps(data)+"\n\n" for event,data in events).encode()
        self.send_response(200);self.send_header("Content-Type","text/event-stream");self.send_header("Content-Length",str(len(payload)));self.end_headers();self.wfile.write(payload)
server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Handler)
threading.Thread(target=server.serve_forever,daemon=True).start()
(codex_home/"config.toml").write_text(
    'model = "fixture"\nmodel_provider = "fixture"\napproval_policy = "never"\nsandbox_mode = "read-only"\ncheck_for_update_on_startup = false\n'
    '[model_providers.fixture]\nname = "Private mock"\nbase_url = "http://127.0.0.1:'+str(server.server_port)+'/v1"\nwire_api = "responses"\nrequires_openai_auth = false\n'
    '[projects.'+json.dumps(project.as_posix())+']\ntrust_level = "trusted"\n',encoding="utf-8")
(profile/"config.json").write_text(json.dumps(dict(
    check_for_updates=False,auto_download_updates=False,show_tray_icon=False,install_cli_on_path=False,
    restore_session=True,restore_agent_sessions=True,shell=dict(program=("cmd.exe" if options.shell=="cmd" else "powershell.exe"),args=(["/d","/k"] if options.shell=="cmd" else ["-NoLogo","-NoProfile"])),
    env={"CODEX_HOME":str(codex_home),"HOME":str(home),"USERPROFILE":str(home)})),encoding="utf-8")

(profile/"window.json").write_text(json.dumps(dict(x=80,y=80,width=1150,height=750)),encoding="utf-8")

def cli(*args,check=True):
    p=subprocess.run([str(cli_path),*args],env=env,capture_output=True,text=True,encoding="utf-8",errors="replace",
                     timeout=15,creationflags=subprocess.CREATE_NO_WINDOW)
    if check and p.returncode: raise RuntimeError((args,p.stdout,p.stderr))
    return p.stdout if p.returncode==0 else ""
def panes():
    return json.loads(cli("pane","ls","--json",check=False) or "{}").get("panes",[])
def capture(pane): return cli("capture","%"+str(pane),"--plain",check=False)
def wait(probe,label,seconds=35):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        r=probe()
        if r:return r
        time.sleep(.3)
    raise AssertionError(label)
def launch(): return subprocess.Popen([str(app_path),"--config-dir",str(profile)],env=env,cwd=project,creationflags=subprocess.CREATE_NO_WINDOW)
def facts():
    try:return json.loads((profile/"machine.json").read_text(encoding="utf-8"))["panes"]
    except (ValueError,FileNotFoundError):return []
print(json.dumps(dict(profile=str(root),codex_home=str(codex_home),mock_port=server.server_port)),flush=True)
# Use the installed version directly: a managed daemon can update itself while
# this test runs, which would make the before/after comparison change versions.
socket=codex_home/"app-server-control"/"app-server-control.sock"
socket.parent.mkdir(parents=True,exist_ok=True)
identity=subprocess.check_output(["whoami.exe","/user","/fo","csv","/nh"],text=True)
sid=next(csv.reader(identity.splitlines()))[1]
subprocess.run(["icacls.exe",str(socket.parent),"/inheritance:r","/grant:r","*"+sid+":(OI)(CI)F"],
               check=True,capture_output=True,creationflags=subprocess.CREATE_NO_WINDOW)
shared_log=(root/"shared-server.log").open("wb")
shared_server=subprocess.Popen([codex_cmd,"app-server","--listen","unix://"+socket.as_posix()],
    env=env,stdout=shared_log,stderr=shared_log,creationflags=subprocess.CREATE_NO_WINDOW)
app=None
targets={}
try:
    wait(lambda: socket.exists(),"private shared Codex server",60)
    app=launch()
    first=wait(panes,"initial pane")[0]
    for marker in ("FIXTURE_A","FIXTURE_B"):
        known={p["pane"] for p in panes()}
        cli("tab","new",str(first["workspace"]),"--cwd",str(project))
        pane=wait(lambda: next((p["pane"] for p in panes() if p["pane"] not in known),None),"new pane")
        targets[marker]=pane
        wait(lambda: capture(pane).strip(),"shell")
        command = "codex" if options.expect_missing or marker=="FIXTURE_A" else "codex.exe"
        cli("send","%"+str(pane),command+" --no-alt-screen","--enter")
        wait(lambda: "fixture" in capture(pane).lower() and "Ask Codex" in capture(pane),"Codex composer",60)
        time.sleep(1)
        cli("send","%"+str(pane),"Reply with "+marker+" only.")
        time.sleep(.3)  # Let Codex finish its paste burst before pressing Enter.
        cli("send","%"+str(pane),"--enter")
        wait(lambda: "ACK_"+marker in capture(pane),"mock assistant response",60)
        (root/(marker+"-before.txt")).write_text(capture(pane),encoding="utf-8")
    (root/"machine-before.json").write_bytes((profile/"machine.json").read_bytes())
    print("real Codex responses",requests,"target identities",[(p["id"],(p.get("agent") or {}).get("session_id")) for p in facts() if p["id"] in targets.values()],flush=True)
    def saved_ids():
        return {p["id"]:(p.get("agent") or {}).get("session_id") for p in facts() if p["id"] in targets.values()}
    if options.expect_missing:
        time.sleep(5)
    else:
        wait(lambda: len(saved_ids())==2 and all(saved_ids().values()),"both live conversation IDs saved",30)
    saved=saved_ids()
    expected=set(saved.values())
    (evidence/"machine-before.json").write_bytes((profile/"machine.json").read_bytes())
    print("live IDs",saved,flush=True)
    def stop():
        global app
        if app.poll() is None:
            app.terminate();app.wait(timeout=15)
        if options.abrupt:
            pid=int((profile/"daemon.pid").read_text())
            subprocess.run(["taskkill.exe","/pid",str(pid),"/t","/f"],check=True,capture_output=True,creationflags=subprocess.CREATE_NO_WINDOW)
        else:
            cli("server","stop")
    def live_ids():
        running={p["pane"] for p in panes() if p.get("live")}
        return {(p.get("agent") or {}).get("session_id") for p in facts() if p["id"] in running and p.get("agent")}
    stop()
    (evidence/"machine-after-stop.json").write_bytes((profile/"machine.json").read_bytes())
    app=launch()
    wait(lambda: len(panes())==3,"three restored panes")
    if options.expect_missing:
        time.sleep(3)
        assert not all(saved.values()),"baseline unexpectedly has both IDs"
        assert not any(live_ids()),"baseline unexpectedly restored agents"
    else:
        wait(lambda: expected <= live_ids(),"original IDs after first restart")
        stop()
        app=launch()
        wait(lambda: expected <= live_ids(),"original IDs after consecutive restart")
        for marker in targets:
            sid=saved[targets[marker]]
            pane=wait(lambda: next((p["id"] for p in facts() if p.get("live") and (p.get("agent") or {}).get("session_id")==sid),None),"restored pane ID")
            def has_client():
                processes=json.loads(cli("procs","%"+str(pane),"--json",check=False) or "{}").get("procs",[])
                return sum(p["name"].lower()=="codex.exe" for p in processes)>=2
            wait(has_client,"restored native client below its launcher",60)
            wait(lambda: "Ask Codex" in capture(pane),"restored composer",60)
            time.sleep(1)
            count=requests.count(marker)
            cli("send","%"+str(pane),"Verify resumed conversation "+marker+".")
            time.sleep(.3)
            cli("send","%"+str(pane),"--enter")
            wait(lambda: requests.count(marker)>count,"new model request in resumed conversation",60)
            wait(lambda: "ACK_"+marker in capture(pane),"restored response")
            assert (next(p for p in facts() if p["id"]==pane)["agent"]["session_id"])==sid
        histories={path.stem[-36:] for path in codex_home.glob("sessions/**/*.jsonl")}
        assert histories==expected,("a restored client created a different conversation",histories,expected)
    (evidence/"machine-after-restart.json").write_bytes((profile/"machine.json").read_bytes())
    subprocess.run(["powershell.exe","-NoProfile","-ExecutionPolicy","Bypass","-File",str(repo/"scripts/capture-native-window.ps1"),"-ProcId",str(app.pid),"-Out",str(evidence/"restored.png")],check=True,capture_output=True,creationflags=subprocess.CREATE_NO_WINDOW)
    report=dict(profile=str(root),codex_cli=subprocess.check_output([codex_cmd,"--version"],env=env,text=True).strip(),
        mode="abrupt" if options.abrupt else "stop",shell=options.shell,original_ids=saved,
        result="baseline failure reproduced" if options.expect_missing else "passed: two exact conversations across two restarts and new turns")
    (evidence/"result.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(report),flush=True)
finally:
    if app is not None and not (evidence/"result.json").exists():
        try:
            for pane in panes():
                (evidence/("failure-pane-"+str(pane["pane"])+".txt")).write_text(capture(pane["pane"]),encoding="utf-8")
        except subprocess.TimeoutExpired:
            pass
    if app is not None and app.poll() is None:
        subprocess.run(["taskkill.exe","/pid",str(app.pid),"/t","/f"],capture_output=True,timeout=20,creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        cli("server","stop",check=False)
    except subprocess.TimeoutExpired:
        if (profile/"daemon.pid").exists():
            pid=int((profile/"daemon.pid").read_text())
            subprocess.run(["taskkill.exe","/pid",str(pid),"/t","/f"],capture_output=True,timeout=20,creationflags=subprocess.CREATE_NO_WINDOW)
    if shared_server.poll() is None:
        subprocess.run(["taskkill.exe","/pid",str(shared_server.pid),"/t","/f"],capture_output=True,timeout=20,creationflags=subprocess.CREATE_NO_WINDOW)
    shared_log.close()
    server.shutdown()

