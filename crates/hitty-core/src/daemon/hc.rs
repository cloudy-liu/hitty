//! herdr-core (`hc`) client — the spike engine that hosts a pane's PTY in a
//! Herdr core instead of opening a pty here (cloudy-liu/hitty#23).
//!
//! The core is reached through the `hc connect` child process: one JSON-RPC
//! line to say `hello {framed:true}`, then a binary frame stream with a
//! 9-byte header — `u32` LE channel, `u8` frame type, `u32` LE payload length.
//! Channel 0 carries JSON-RPC; every other channel is a terminal stream.
//! Frame types: `DATA` (terminal bytes, both directions), `CREDIT` (flow
//! control, `u32` LE byte count), `CLOSE` (the stream ended; payload may name
//! the reason and exit code), `RESIZE` (two `u16` LE), `FOCUS` (one byte),
//! `FACTS` (a display document, client → core).
//!
//! Written against the shapes herdr-app's `src/hc.rs` (Apache-2.0) uses, and
//! against `hc schema`. Lifecycle events support recovery; Hitty keeps its
//! existing hooks and screen-based agent detection.
//! Reference revision and adaptation scope: docs/provenance/herdr-pane-engine.md.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io::{self, BufRead as _, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child as StdChild, Command, Stdio};
#[cfg(test)]
use std::sync::atomic::AtomicU16;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::daemon::protocol::WinSize;
use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
use serde::Deserialize;
use serde_json::{Value, json};

/// Protocol generation this build speaks. A core that answers `hello` with a
/// newer generation is refused — mirroring herdr-app's `GENERATION` check —
/// and the pane falls back to the native engine.
pub(crate) const GENERATION: u64 = 1;

/// Oldest core exercised by this client and its integration tests.
const MIN_VERSION: &str = "0.1.15";

/// Methods this client requires from `hello`. A missing one means the core is
/// older than what the spike was written against.
const REQUIRED_METHODS: &[&str] = &[
    "terminal.run",
    "terminal.attach",
    "terminal.get",
    "terminal.list",
    "terminal.kill",
    "terminal.rm",
    "events.subscribe",
];

const HEADER: usize = 9;
pub(crate) const DATA: u8 = 0;
pub(crate) const CREDIT: u8 = 1;
pub(crate) const CLOSE: u8 = 2;
const RESIZE: u8 = 3;
const FOCUS: u8 = 4;
const FACTS: u8 = 5;

/// Sanity cap on a single frame; real DATA frames stream in chunks far below
/// this. A larger length means the stream is out of sync.
const MAX_PAYLOAD: usize = 16 << 20;

/// Credit granted to a freshly attached terminal. Replenished as the reader
/// consumes bytes, so the window stays this wide.
const ATTACH_CREDIT: u32 = 1 << 20;

/// How many frames a not-yet-registered channel may queue. First paint can
/// arrive before the `terminal.attach` reply is routed home.
const EARLY_FRAME_CAP: usize = 256;
const EARLY_BYTES_CAP: usize = 4 << 20;

/// How long a pane keeps trying to come back after the connection dropped —
/// `hc update` restarts the core while its terminals keep running, and the
/// pane should outlive that.
const REATTACH_WINDOW: Duration = Duration::from_secs(60);

/// Label naming the hitty pane a terminal belongs to, and the label naming the
/// profile that owns it. Together they are how a pane adopts the terminal it
/// was running before its daemon restarted.
pub(crate) const LABEL_PANE: &str = "hitty.pane";
pub(crate) const LABEL_OWNER: &str = "hitty.owner";

/// The subset of a core `Terminal` this client needs.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Terminal {
    pub address: String,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub foreground: Option<Foreground>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Foreground {
    #[cfg_attr(windows, allow(dead_code))]
    pub pid: u32,
    #[serde(default)]
    pub cwd: String,
}

#[derive(Default)]
struct Mirror {
    seq: u64,
    terminals: HashMap<String, Terminal>,
    revision: u64,
}

/// What a registered terminal channel delivers to its reader.
#[derive(Debug)]
pub(crate) enum StreamEvent {
    Data(Vec<u8>),
    /// The stream ended. The payload is the core's close document, if any.
    Close(Vec<u8>),
    /// The connection to the core dropped. Reattach is possible.
    Lost,
}

#[derive(Clone)]
enum Sink {
    Stream(Sender<StreamEvent>),
    Events(Sender<Value>),
}

struct Conn {
    child: Option<StdChild>,
    writer: Box<dyn Write + Send>,
}

/// One framed connection to the core, multiplexing every pane's terminal.
///
/// A lost connection is not fatal: `ensure_connected` dials a fresh
/// `hc connect` child, and each pane's reader reattaches its own terminal by
/// address (`terminal.attach` again), which is exactly the sequence the core
/// expects after a restart.
pub(crate) struct Core {
    conn: Mutex<Option<Conn>>,
    reconnect: Mutex<()>,
    pending: Mutex<HashMap<u64, Sender<Result<Value, String>>>>,
    channels: Mutex<HashMap<u32, Sink>>,
    early: Mutex<HashMap<u32, VecDeque<(u8, Vec<u8>)>>>,
    /// Leftover bytes per events channel: DATA frame boundaries may fall
    /// anywhere inside an event line.
    event_lines: Mutex<HashMap<u32, Vec<u8>>>,
    next_id: AtomicU64,
    alive: AtomicBool,
    epoch: AtomicU64,
    watching: AtomicBool,
    mirror: Mutex<Mirror>,
    changed: Condvar,
    #[cfg(test)]
    connector: Mutex<Option<TestConnector>>,
}

/// The process-wide core connection. `pane_engine = "herdr-core"` panes share
/// it; a pane's reader may reconnect it underneath.
static CORE: Mutex<Option<Arc<Core>>> = Mutex::new(None);

#[cfg(test)]
static TEST_CORE: Mutex<Option<Arc<Core>>> = Mutex::new(None);

#[cfg(test)]
pub(crate) type TestConnector = Box<
    dyn FnMut() -> anyhow::Result<(
            Box<dyn Read + Send>,
            Box<dyn Write + Send>,
            Option<StdChild>,
        )> + Send,
>;

#[cfg(test)]
pub(crate) fn set_test_core(core: Option<Arc<Core>>) {
    *TEST_CORE.lock().unwrap() = core;
}

/// The shared core, connecting lazily. `Err` means the pane engine is not
/// available (hc missing, refused handshake); callers fall back to native.
pub(crate) fn core() -> anyhow::Result<Arc<Core>> {
    #[cfg(test)]
    if let Some(core) = TEST_CORE.lock().unwrap().as_ref() {
        let core = core.clone();
        if !core.watching.load(Ordering::SeqCst) {
            core.start_watch()?;
        }
        return Ok(core);
    }
    let mut slot = CORE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(core) = slot.as_ref() {
        core.ensure_connected()?;
        return Ok(core.clone());
    }
    let core = Core::connect()?;
    core.start_watch()?;
    *slot = Some(core.clone());
    Ok(core)
}

/// The label value identifying this hitty profile, so two profiles on one
/// machine never adopt each other's terminals.
pub(crate) fn owner_key() -> String {
    let seed = crate::core::config::config_dir_path()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{:016x}", crate::host::fnv1a64(seed.as_bytes()))
}

/// The labels every hitty-hosted terminal carries.
pub(crate) fn pane_labels(pane: u64, owner: &str, workspace: Option<&str>) -> Value {
    let mut labels = serde_json::Map::new();
    labels.insert(LABEL_PANE.to_string(), Value::String(format!("%{pane}")));
    labels.insert(LABEL_OWNER.to_string(), Value::String(owner.to_string()));
    if let Some(workspace) = workspace {
        labels.insert("hitty.ws".to_string(), Value::String(workspace.to_string()));
    }
    Value::Object(labels)
}

impl Core {
    /// Spawn `hc connect` and finish the `hello` handshake.
    pub(crate) fn connect() -> anyhow::Result<Arc<Core>> {
        let (read, write, child) = spawn_connect_child()?;
        Self::from_transport(read, write, child)
    }

    /// The handshake and reader thread around an established transport. Tests
    /// hand in pipe pairs; production hands in the child's stdio.
    pub(crate) fn from_transport(
        read: Box<dyn Read + Send>,
        write: Box<dyn Write + Send>,
        child: Option<StdChild>,
    ) -> anyhow::Result<Arc<Core>> {
        let core = Arc::new(Core {
            conn: Mutex::new(None),
            reconnect: Mutex::new(()),
            pending: Mutex::new(HashMap::new()),
            channels: Mutex::new(HashMap::new()),
            early: Mutex::new(HashMap::new()),
            event_lines: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            watching: AtomicBool::new(false),
            mirror: Mutex::new(Mirror::default()),
            changed: Condvar::new(),
            #[cfg(test)]
            connector: Mutex::new(None),
        });
        match core.establish(read, write, child) {
            Ok(()) => Ok(core),
            Err(err) => Err(err),
        }
    }

    /// Write `hello`, check the reply, switch to framed reading. On failure
    /// the fresh child is killed so a refused core never lingers.
    fn establish(
        self: &Arc<Self>,
        read: Box<dyn Read + Send>,
        write: Box<dyn Write + Send>,
        child: Option<StdChild>,
    ) -> anyhow::Result<()> {
        let mut reader = BufReader::new(read);
        let mut writer = write;
        let hello = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "hello",
            "params": {"framed": true, "generation": GENERATION},
        });
        let fail = |child: Option<StdChild>, err: anyhow::Error| -> anyhow::Result<()> {
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
            Err(err)
        };
        if let Err(err) = write_json_line(&mut writer, &hello) {
            return fail(child, err.into());
        }
        // Bound both silence and a missing newline. The only process we kill
        // on timeout is our own `hc connect` transport, never the core.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = Vec::new();
            let result = (&mut reader).take(64 * 1024).read_until(b'\n', &mut line);
            let _ = tx.send((reader, line, result));
        });
        let (reader, line, result) = match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(reply) => reply,
            Err(_) => return fail(child, anyhow::anyhow!("hc hello timed out after 5 seconds")),
        };
        if let Err(e) = result {
            return fail(child, e.into());
        }
        if !line.ends_with(b"\n") {
            return fail(child, anyhow::anyhow!("hc hello missing or oversized"));
        }
        let reply: Value = match serde_json::from_slice(&line) {
            Ok(reply) => reply,
            Err(e) => return fail(child, anyhow::anyhow!("hc hello is not JSON: {e}")),
        };
        if let Err(problem) = check_hello(&reply) {
            return fail(child, anyhow::anyhow!("incompatible herdr-core: {problem}"));
        }
        *self.conn.lock().unwrap() = Some(Conn {
            child,
            writer: Box::new(writer),
        });
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.alive.store(true, Ordering::SeqCst);
        spawn_frame_reader(self.clone(), reader, epoch);
        Ok(())
    }

    /// Reconnect after a loss, serialized so two panes race into one dial.
    pub(crate) fn ensure_connected(self: &Arc<Self>) -> anyhow::Result<()> {
        let _guard = self.reconnect.lock().unwrap_or_else(|e| e.into_inner());
        if self.conn.lock().unwrap().is_some() {
            return Ok(());
        }
        #[cfg(test)]
        let transport = self
            .connector
            .lock()
            .unwrap()
            .as_mut()
            .map(|connect| connect());
        #[cfg(not(test))]
        let transport: Option<
            anyhow::Result<(
                Box<dyn Read + Send>,
                Box<dyn Write + Send>,
                Option<StdChild>,
            )>,
        > = None;
        let (read, write, child) = transport.unwrap_or_else(spawn_connect_child)?;
        self.establish(read, write, child)?;
        if self.watching.load(Ordering::SeqCst) {
            self.start_watch()?;
        }
        Ok(())
    }

    // ---- RPC -----------------------------------------------------------

    pub(crate) fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.call_timeout(method, params, Duration::from_secs(15))
    }

    pub(crate) fn call_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(err) = self.write_json(&message) {
            self.pending.lock().unwrap().remove(&id);
            return Err(err);
        }
        match rx.recv_timeout(timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(message)) => anyhow::bail!("{method}: {message}"),
            Err(RecvTimeoutError::Timeout) => {
                self.pending.lock().unwrap().remove(&id);
                anyhow::bail!("{method}: no reply within {:?}", timeout)
            }
            Err(RecvTimeoutError::Disconnected) => anyhow::bail!("{method}: lost the core"),
        }
    }

    /// Start a terminal. `params` are `terminal.run`'s, built by the caller.
    pub(crate) fn run(&self, params: Value) -> anyhow::Result<Terminal> {
        let result = self.call_timeout("terminal.run", params, Duration::from_secs(30))?;
        parse_terminal(&result)
    }

    pub(crate) fn get(&self, address: &str) -> anyhow::Result<Terminal> {
        let result = self.call_timeout(
            "terminal.get",
            json!({"terminal": address}),
            Duration::from_secs(10),
        )?;
        parse_terminal(&result)
    }

    /// Terminals carrying exactly these labels. Used to adopt a pane's own
    /// terminal after a daemon restart.
    pub(crate) fn find_by_pane_labels(
        &self,
        pane: u64,
        owner: &str,
    ) -> anyhow::Result<Option<Terminal>> {
        if self.watching.load(Ordering::SeqCst) {
            return Ok(self.owned_terminals().into_iter().find(|terminal| {
                terminal.labels.get(LABEL_PANE) == Some(&format!("%{pane}"))
                    && terminal.labels.get(LABEL_OWNER).map(String::as_str) == Some(owner)
            }));
        }
        let result = self.call(
            "terminal.list",
            json!({"labels": {LABEL_PANE: format!("%{pane}"), LABEL_OWNER: owner}}),
        )?;
        let terminals = result
            .get("terminals")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for terminal in terminals {
            if let Ok(terminal) = parse_terminal(&terminal) {
                // Guard against a core that ignored the label filter: only the
                // terminal carrying this pane's own label may be adopted.
                let pane_label = format!("%{pane}");
                if terminal.labels.get(LABEL_PANE).map(String::as_str) == Some(pane_label.as_str())
                    && terminal.labels.get(LABEL_OWNER).map(String::as_str) == Some(owner)
                {
                    return Ok(Some(terminal));
                }
            }
        }
        Ok(None)
    }

    pub(crate) fn kill(&self, address: &str) -> anyhow::Result<()> {
        self.call("terminal.kill", json!({"terminal": address}))?;
        Ok(())
    }

    pub(crate) fn remove(&self, address: &str) -> anyhow::Result<()> {
        self.call("terminal.rm", json!({"terminal": address}))?;
        Ok(())
    }

    /// Attach a viewer to a terminal. Registers the stream channel before the
    /// reply is returned, draining anything that arrived early, then grants
    /// the initial credit window.
    #[cfg(test)]
    pub(crate) fn attach(
        &self,
        address: &str,
        cols: u16,
        rows: u16,
        history: u32,
        resumed: bool,
    ) -> anyhow::Result<(u32, Terminal, Receiver<StreamEvent>)> {
        self.attach_display(address, cols, rows, history, resumed, json!({}))
    }

    fn attach_display(
        &self,
        address: &str,
        cols: u16,
        rows: u16,
        history: u32,
        resumed: bool,
        display: Value,
    ) -> anyhow::Result<(u32, Terminal, Receiver<StreamEvent>)> {
        let result = self.call(
            "terminal.attach",
            json!({
                "terminal": address,
                "cols": cols,
                "rows": rows,
                "history": history,
                "resumed": resumed,
                "observe": false,
                "display": display,
            }),
        )?;
        let channel = result
            .get("channel")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("terminal.attach replied without a channel"))?
            as u32;
        let terminal = parse_terminal(result.get("terminal").unwrap_or(&result))?;
        let rx = self.register_stream(channel);
        self.write_frame(channel, CREDIT, &ATTACH_CREDIT.to_le_bytes())?;
        Ok((channel, terminal, rx))
    }

    /// Subscribe before spawning or adopting terminals so lifecycle changes
    /// cannot fall into a gap between a list and the event stream.
    pub(crate) fn subscribe_events(
        &self,
        labels: Value,
    ) -> anyhow::Result<(u64, Vec<Terminal>, Receiver<Value>)> {
        let result = self.call("events.subscribe", json!({"labels": labels}))?;
        let terminals = result
            .get("terminals")
            .and_then(Value::as_array)
            .map(|terminals| {
                terminals
                    .iter()
                    .filter_map(|terminal| parse_terminal(terminal).ok())
                    .collect()
            })
            .unwrap_or_default();
        let channel = result
            .get("channel")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("events.subscribe replied without a channel"))?
            as u32;
        let rx = self.register_events(channel);
        self.send_credit(channel, ATTACH_CREDIT)?;
        Ok((result["seq"].as_u64().unwrap_or(0), terminals, rx))
    }

    fn start_watch(self: &Arc<Self>) -> anyhow::Result<()> {
        let epoch = self.epoch.load(Ordering::SeqCst);
        let (seq, terminals, rx) = self.subscribe_events(json!({LABEL_OWNER: owner_key()}))?;
        {
            let mut mirror = self.mirror.lock().unwrap();
            mirror.seq = seq;
            mirror.terminals = terminals
                .into_iter()
                .map(|t| (t.address.clone(), t))
                .collect();
            mirror.revision += 1;
        }
        self.watching.store(true, Ordering::SeqCst);
        self.changed.notify_all();
        let core = self.clone();
        std::thread::Builder::new()
            .name("hitty-hc-events".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    if core.epoch.load(Ordering::SeqCst) != epoch {
                        break;
                    }
                    core.apply_event(event, epoch);
                }
            })?;
        Ok(())
    }

    pub(crate) fn owned_terminals(&self) -> Vec<Terminal> {
        let owner = owner_key();
        self.mirror
            .lock()
            .unwrap()
            .terminals
            .values()
            .filter(|t| !t.address.contains('/') && t.labels.get(LABEL_OWNER) == Some(&owner))
            .cloned()
            .collect()
    }

    fn apply_event(&self, event: Value, epoch: u64) {
        let p = &event["params"];
        let mut mirror = self.mirror.lock().unwrap();
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        let seq = p["seq"].as_u64().unwrap_or(0);
        if event["method"] == "missed" && seq >= mirror.seq {
            mirror.terminals = p["terminals"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| parse_terminal(t).ok())
                .map(|t| (t.address.clone(), t))
                .collect();
            mirror.seq = seq;
        } else if event["method"] == "event" && seq > mirror.seq {
            mirror.seq = seq;
            let address = p["terminal"].as_str().unwrap_or_default();
            let data = &p["data"];
            match p["type"].as_str() {
                Some("created" | "restarted") => {
                    if let Ok(terminal) = parse_terminal(data) {
                        mirror.terminals.insert(address.to_owned(), terminal);
                    }
                }
                Some("removed") => {
                    mirror.terminals.remove(address);
                }
                Some("finished" | "lost") => {
                    if let Some(t) = mirror.terminals.get_mut(address) {
                        if p["type"] == "finished" {
                            t.state = "finished".into();
                        }
                        t.exit_code = data["exit_code"].as_i64().map(|v| v as i32);
                        t.pid = None;
                        t.foreground = None;
                    }
                }
                Some("foreground") => {
                    if let Some(t) = mirror.terminals.get_mut(address) {
                        t.foreground = serde_json::from_value(data.clone()).ok();
                    }
                }
                Some("labels") => {
                    if let Ok(terminal) = parse_terminal(&data["record"]) {
                        mirror.terminals.insert(address.to_owned(), terminal);
                    }
                    if let Some(t) = mirror.terminals.get_mut(address) {
                        if let Ok(labels) = serde_json::from_value(data["labels"].clone()) {
                            t.labels = labels;
                        }
                        if t.labels.get(LABEL_OWNER) != Some(&owner_key()) {
                            mirror.terminals.remove(address);
                        }
                    }
                }
                Some("size") => {
                    if let Some(t) = mirror.terminals.get_mut(address) {
                        t.cols = data["cols"].as_u64().map(|n| n as u16);
                        t.rows = data["rows"].as_u64().map(|n| n as u16);
                    }
                }
                _ => {}
            }
        } else {
            return;
        }
        mirror.revision += 1;
        self.changed.notify_all();
    }

    fn terminal_fact(&self, address: &str) -> Option<Terminal> {
        self.mirror.lock().unwrap().terminals.get(address).cloned()
    }

    fn detach(&self, channel: u32, epoch: u64) {
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        self.channels.lock().unwrap().remove(&channel);
        self.early.lock().unwrap().remove(&channel);
        let _ = self.write_bound(epoch, channel, CLOSE, &[]);
    }

    fn write_bound(&self, epoch: u64, channel: u32, kind: u8, bytes: &[u8]) -> anyhow::Result<()> {
        let mut connection = self.conn.lock().unwrap();
        anyhow::ensure!(
            self.epoch.load(Ordering::SeqCst) == epoch,
            "hc channel belongs to a disconnected core"
        );
        let conn = connection
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("lost the core"))?;
        write_frame(&mut conn.writer, channel, kind, bytes)
    }

    // ---- stream plumbing -------------------------------------------------

    fn register_stream(&self, channel: u32) -> Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel();
        let mut channels = self.channels.lock().unwrap();
        channels.insert(channel, Sink::Stream(tx.clone()));
        let early = self.early.lock().unwrap().remove(&channel);
        if let Some(early) = early {
            for (ty, payload) in early {
                if let Some(event) = stream_event(ty, payload) {
                    let _ = tx.send(event);
                }
            }
        }
        rx
    }

    fn register_events(&self, channel: u32) -> Receiver<Value> {
        let (tx, rx) = mpsc::channel();
        let mut channels = self.channels.lock().unwrap();
        channels.insert(channel, Sink::Events(tx.clone()));
        let early = self.early.lock().unwrap().remove(&channel);
        if let Some(early) = early {
            for (ty, payload) in early {
                if ty == DATA {
                    self.route_event_bytes(channel, &payload, &tx);
                }
            }
        }
        rx
    }

    #[cfg(test)]
    pub(crate) fn input(&self, channel: u32, bytes: &[u8]) -> anyhow::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.write_frame(channel, DATA, bytes)
    }

    pub(crate) fn send_credit(&self, channel: u32, bytes: u32) -> anyhow::Result<()> {
        self.write_frame(channel, CREDIT, &bytes.to_le_bytes())
    }

    #[cfg(test)]
    pub(crate) fn send_resize(&self, channel: u32, cols: u16, rows: u16) -> anyhow::Result<()> {
        let mut payload = Vec::with_capacity(4);
        payload.extend_from_slice(&cols.to_le_bytes());
        payload.extend_from_slice(&rows.to_le_bytes());
        self.write_frame(channel, RESIZE, &payload)
    }

    #[allow(dead_code)]
    pub(crate) fn send_focus(&self, channel: u32, focused: bool) -> anyhow::Result<()> {
        self.write_frame(channel, FOCUS, &[focused as u8])
    }

    #[allow(dead_code)]
    pub(crate) fn send_facts(&self, channel: u32, facts: &Value) -> anyhow::Result<()> {
        self.write_frame(channel, FACTS, facts.to_string().as_bytes())
    }

    fn write_json(&self, message: &Value) -> anyhow::Result<()> {
        // Framed mode carries *everything* in frames, channel 0 included;
        // only the `hello` line written before the switch is raw.
        let text = serde_json::to_string(message)?;
        self.write_frame(0, DATA, text.as_bytes())
    }

    fn write_frame(&self, channel: u32, ty: u8, payload: &[u8]) -> anyhow::Result<()> {
        let mut guard = self.conn.lock().unwrap();
        let conn = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("lost the core"))?;
        write_frame(&mut conn.writer, channel, ty, payload)
    }

    fn route_rpc(&self, payload: &[u8]) {
        let text = String::from_utf8_lossy(payload);
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                log::warn!("hc: unparseable channel-0 message: {line:?}");
                continue;
            };
            let Some(id) = value.get("id").and_then(Value::as_u64) else {
                self.route_notification(value);
                continue;
            };
            let Some(sender) = self.pending.lock().unwrap().remove(&id) else {
                continue;
            };
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("rpc error");
                let kind = error
                    .get("data")
                    .and_then(|data| data.get("kind"))
                    .and_then(Value::as_str);
                let message = match kind {
                    Some(kind) => format!("{message} ({kind})"),
                    None => message.to_string(),
                };
                let _ = sender.send(Err(message));
            } else {
                let result = value.get("result").cloned().unwrap_or(Value::Null);
                let _ = sender.send(Ok(result));
            }
        }
    }

    fn route_notification(&self, value: Value) {
        let senders: Vec<Sender<Value>> = self
            .channels
            .lock()
            .unwrap()
            .values()
            .filter_map(|sink| match sink {
                Sink::Events(tx) => Some(tx.clone()),
                Sink::Stream(_) => None,
            })
            .collect();
        for sender in senders {
            let _ = sender.send(value.clone());
        }
    }

    fn route_stream(&self, channel: u32, ty: u8, payload: Vec<u8>) {
        let channels = self.channels.lock().unwrap();
        let sink = channels.get(&channel).cloned();
        match sink {
            Some(Sink::Stream(tx)) => {
                drop(channels);
                if let Some(event) = stream_event(ty, payload) {
                    let _ = tx.send(event);
                }
            }
            Some(Sink::Events(tx)) => {
                drop(channels);
                if ty == DATA {
                    self.route_event_bytes(channel, &payload, &tx);
                }
            }
            None => {
                let mut early = self.early.lock().unwrap();
                let bytes: usize = early
                    .values()
                    .flat_map(|q| q.iter())
                    .map(|(_, p)| p.len())
                    .sum();
                if bytes.saturating_add(payload.len()) > EARLY_BYTES_CAP
                    || (early.len() >= EARLY_FRAME_CAP && !early.contains_key(&channel))
                {
                    log::warn!("hc early channel buffer is full");
                    return;
                }
                let queue = early.entry(channel).or_default();
                if queue.len() < EARLY_FRAME_CAP {
                    queue.push_back((ty, payload));
                } else {
                    log::warn!("hc: dropping early frames for channel {channel}");
                }
            }
        }
    }

    fn route_event_bytes(&self, channel: u32, payload: &[u8], tx: &Sender<Value>) {
        let lines: Vec<Vec<u8>> = {
            let mut buffers = self.event_lines.lock().unwrap();
            let buffer = buffers.entry(channel).or_default();
            if buffer.len().saturating_add(payload.len()) > MAX_PAYLOAD {
                buffer.clear();
                log::warn!("hc event line exceeds the protocol frame limit");
            }
            buffer.extend_from_slice(payload);
            let mut lines = Vec::new();
            while let Some(index) = buffer.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=index).collect();
                lines.push(line);
            }
            lines
        };
        let _ = self.send_credit(channel, payload.len() as u32);
        for line in lines {
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(text) {
                Ok(value) => {
                    let _ = tx.send(value);
                }
                Err(err) => log::warn!("hc: unparseable event line ({err}): {text:?}"),
            }
        }
    }

    /// The connection ended: fail pending calls, tell every stream, drop the
    /// child. Readers decide between reattaching and reporting the loss.
    fn on_connection_lost(&self, epoch: u64) {
        let _reconnect = self.reconnect.lock().unwrap();
        let mut connection = self.conn.lock().unwrap();
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        self.alive.store(false, Ordering::SeqCst);
        if let Some(mut conn) = connection.take() {
            if let Some(mut child) = conn.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        drop(connection);
        let pending: Vec<_> = self.pending.lock().unwrap().drain().collect();
        for (_, sender) in pending {
            let _ = sender.send(Err("lost the core".to_string()));
        }
        let sinks: Vec<Sink> = self
            .channels
            .lock()
            .unwrap()
            .drain()
            .map(|(_, s)| s)
            .collect();
        for sink in sinks {
            if let Sink::Stream(tx) = sink {
                let _ = tx.send(StreamEvent::Lost);
            }
        }
        self.early.lock().unwrap().clear();
        self.event_lines.lock().unwrap().clear();
        self.changed.notify_all();
    }
}

fn stream_event(ty: u8, payload: Vec<u8>) -> Option<StreamEvent> {
    match ty {
        DATA => Some(StreamEvent::Data(payload)),
        CLOSE => Some(StreamEvent::Close(payload)),
        other => {
            log::debug!("hc: ignoring stream frame type {other}");
            None
        }
    }
}

fn spawn_frame_reader(core: Arc<Core>, mut reader: BufReader<Box<dyn Read + Send>>, epoch: u64) {
    std::thread::Builder::new()
        .name("hitty-daemon-hc-reader".to_string())
        .spawn(move || {
            loop {
                let mut header = [0u8; HEADER];
                if reader.read_exact(&mut header).is_err() {
                    break;
                }
                let channel = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
                let ty = header[4];
                let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
                if len > MAX_PAYLOAD {
                    log::warn!("hc: frame of {len} bytes exceeds the cap; dropping the link");
                    break;
                }
                let mut payload = vec![0u8; len];
                if reader.read_exact(&mut payload).is_err() {
                    break;
                }
                if channel == 0 {
                    core.route_rpc(&payload);
                } else {
                    core.route_stream(channel, ty, payload);
                }
            }
            core.on_connection_lost(epoch);
        })
        .expect("spawn hc frame reader thread");
}

// ---- transport -----------------------------------------------------------

fn spawn_connect_child() -> anyhow::Result<(
    Box<dyn Read + Send>,
    Box<dyn Write + Send>,
    Option<StdChild>,
)> {
    let binary = locate()?;
    let mut command = Command::new(&binary);
    command
        .arg("connect")
        .env_remove("HC_SESSION")
        .env_remove("HC_TERMINAL_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // The daemon runs without a console; a visible console window for the
        // hc child would flash on the user's desktop.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|err| anyhow::anyhow!("cannot run `{} connect`: {err}", binary.display()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("hc connect has no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("hc connect has no stdout"))?;
    Ok((Box::new(stdout), Box::new(stdin), Some(child)))
}

/// Find the `hc` binary. `HITTY_HC_BIN` wins (tests, unusual installs); then
/// PATH; then the standard install locations.
pub(crate) fn locate() -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("HITTY_HC_BIN") {
        let path = PathBuf::from(path);
        validate_hc(&path)?;
        return Ok(path);
    }
    let mut candidates: Vec<PathBuf> = vec![PathBuf::from("hc")];
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let dir = PathBuf::from(local).join("Programs").join("herdr-core");
            candidates.push(dir.join("hc.exe"));
            candidates.push(dir.join("herdr-core.exe"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(&home).join(".local/bin/hc"));
            candidates.push(PathBuf::from(&home).join(".cargo/bin/hc"));
        }
        candidates.push(PathBuf::from("/usr/local/bin/hc"));
        candidates.push(PathBuf::from("/opt/homebrew/bin/hc"));
    }
    let mut tried = Vec::new();
    for candidate in &candidates {
        if validate_hc(candidate).is_ok() {
            return Ok(candidate.clone());
        }
        tried.push(candidate.display().to_string());
    }
    anyhow::bail!("herdr-core (`hc`) not found (tried: {})", tried.join(", "))
}

fn validate_hc(path: &PathBuf) -> anyhow::Result<()> {
    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait()? {
            let mut stdout = String::new();
            child
                .stdout
                .take()
                .unwrap()
                .take(1024)
                .read_to_string(&mut stdout)?;
            anyhow::ensure!(
                status.success() && stdout.starts_with("hc "),
                "{} is not herdr-core",
                path.display()
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("{} --version timed out", path.display());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn write_json_line(writer: &mut Box<dyn Write + Send>, message: &Value) -> anyhow::Result<()> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    writer.write_all(line.as_bytes())?;
    writer.flush()?;
    Ok(())
}

fn write_frame(
    writer: &mut Box<dyn Write + Send>,
    channel: u32,
    ty: u8,
    payload: &[u8],
) -> anyhow::Result<()> {
    let mut header = [0u8; HEADER];
    header[0..4].copy_from_slice(&channel.to_le_bytes());
    header[4] = ty;
    header[5..9].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    writer.write_all(&header)?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

// ---- hello compatibility -------------------------------------------------

/// `Ok` when the core can serve this build: generation not newer, version at
/// least [`MIN_VERSION`], required methods present.
pub(crate) fn check_hello(reply: &Value) -> Result<(), String> {
    let result = reply.get("result").unwrap_or(reply);
    let generation = result
        .get("generation")
        .and_then(Value::as_u64)
        .ok_or_else(|| "hello reply has no generation".to_string())?;
    if generation != GENERATION {
        return Err(format!(
            "core speaks protocol generation {generation}; this build speaks {GENERATION}"
        ));
    }
    if result["framed"] != true {
        return Err("core refused framed mode".into());
    }
    let version = result
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| "hello reply has no version".to_string())?;
    if version_lt(version, MIN_VERSION) {
        return Err(format!("core {version} is older than {MIN_VERSION}"));
    }
    let methods: Vec<&str> = result
        .get("methods")
        .and_then(Value::as_array)
        .map(|methods| methods.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let missing: Vec<&str> = REQUIRED_METHODS
        .iter()
        .filter(|method| !methods.contains(*method))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(format!("core is missing methods: {}", missing.join(", ")));
    }
    Ok(())
}

fn version_lt(have: &str, want: &str) -> bool {
    fn parts(text: &str) -> Option<(u64, u64, u64)> {
        let mut numbers = text.split(['.', '-', '+']).take(3);
        let major = numbers.next()?.parse().ok()?;
        let minor = numbers.next().unwrap_or("0").parse().ok()?;
        let patch = numbers.next().unwrap_or("0").parse().ok()?;
        Some((major, minor, patch))
    }
    match (parts(have), parts(want)) {
        (Some(have), Some(want)) => have < want,
        // Unparseable versions refuse rather than guess.
        _ => true,
    }
}

fn parse_terminal(value: &Value) -> anyhow::Result<Terminal> {
    serde_json::from_value(value.clone())
        .map_err(|err| anyhow::anyhow!("unexpected terminal document ({err}): {value}"))
}

/// Exit code and reason out of a `CLOSE` payload, as leniently as possible:
/// the document shape is the core's, and a missing exit code falls back to
/// `terminal.get` afterwards.
fn parse_close(payload: &[u8]) -> (Option<i32>, Option<String>) {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return (None, None);
    };
    let code = value
        .get("exit_code")
        .or_else(|| value.get("code"))
        .and_then(Value::as_i64)
        .map(|code| code as i32);
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    (code, reason)
}

fn reason_is_loss(reason: Option<&str>) -> bool {
    matches!(
        reason,
        Some("holder_lost" | "server_stop" | "stopped_by_restart" | "lost")
    )
}

/// A core error that means the terminal is not coming back.
fn gone(err: &anyhow::Error) -> bool {
    let text = format!("{err:#}");
    text.contains("not_found") || text.contains("finished")
}

// ---- pane-facing handles -------------------------------------------------

/// Shared exit record between a pane's reader and its `Child`.
pub(crate) struct ChildExit {
    status: Mutex<Option<ExitStatus>>,
    cv: Condvar,
}

impl ChildExit {
    pub(crate) fn new() -> Self {
        Self {
            status: Mutex::new(None),
            cv: Condvar::new(),
        }
    }

    pub(crate) fn set(&self, code: u32) {
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if status.is_none() {
            *status = Some(ExitStatus::with_exit_code(code));
            self.cv.notify_all();
        }
    }

    fn get(&self) -> Option<ExitStatus> {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn wait(&self, cap: Duration) -> ExitStatus {
        let deadline = Instant::now() + cap;
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(status) = status.as_ref() {
                return status.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                log::warn!("hc: pane exit never reported; reporting code 1");
                return ExitStatus::with_exit_code(1);
            }
            let (guard, _) = self
                .cv
                .wait_timeout(status, left)
                .unwrap_or_else(|e| e.into_inner());
            status = guard;
        }
    }
}

/// Reader, writer and resize always follow the same reattached channel.
pub(crate) struct Session {
    core: Arc<Core>,
    address: String,
    bound: Mutex<Option<(u64, u32)>>,
    available: Condvar,
    size: Mutex<WinSize>,
    exit: Arc<ChildExit>,
    detached: AtomicBool,
    preserved: AtomicBool,
}

impl Session {
    pub(crate) fn attach(
        core: Arc<Core>,
        terminal: &Terminal,
        size: WinSize,
        history: u32,
    ) -> anyhow::Result<(Arc<Self>, HcReader)> {
        let epoch = core.epoch.load(Ordering::SeqCst);
        let (channel, _, rx) = core.attach_display(
            &terminal.address,
            size.cols,
            size.rows,
            history,
            false,
            display(size),
        )?;
        let session = Arc::new(Self {
            core,
            address: terminal.address.clone(),
            bound: Mutex::new(Some((epoch, channel))),
            available: Condvar::new(),
            size: Mutex::new(size),
            exit: Arc::new(ChildExit::new()),
            detached: AtomicBool::new(false),
            preserved: AtomicBool::new(false),
        });
        Ok((
            session.clone(),
            HcReader {
                session,
                rx,
                buffer: Vec::new(),
                offset: 0,
            },
        ))
    }
    fn send(&self, kind: u8, bytes: &[u8]) -> anyhow::Result<()> {
        let bound = self.bound.lock().unwrap();
        let (bound, _) = self
            .available
            .wait_timeout_while(bound, Duration::from_secs(5), |b| {
                b.is_none() && !self.detached.load(Ordering::SeqCst) && self.exit.get().is_none()
            })
            .unwrap();
        anyhow::ensure!(!self.detached.load(Ordering::SeqCst), "hc pane is detached");
        let (epoch, channel) =
            (*bound).ok_or_else(|| anyhow::anyhow!("hc pane is reconnecting"))?;
        self.core.write_bound(epoch, channel, kind, bytes)
    }
    pub(crate) fn preserve(&self) {
        self.preserved.store(true, Ordering::SeqCst);
        self.detach();
    }
    pub(crate) fn is_preserved(&self) -> bool {
        self.preserved.load(Ordering::SeqCst)
    }
    pub(crate) fn detach(&self) {
        self.detached.store(true, Ordering::SeqCst);
        if let Some((epoch, channel)) = self.bound.lock().unwrap().take() {
            self.core.detach(channel, epoch);
        }
        self.available.notify_all();
    }
    pub(crate) fn close(&self) -> io::Result<()> {
        if self.detached.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let result = self
            .core
            .ensure_connected()
            .and_then(|_| self.core.kill(&self.address))
            .and_then(|_| self.core.remove(&self.address));
        self.detach();
        self.exit.set(1);
        result.map_err(|e| io::Error::other(e.to_string()))
    }
    pub(crate) fn focus(&self, focused: bool) {
        let _ = self.send(FOCUS, &[focused as u8]);
    }
    #[cfg(unix)]
    pub(crate) fn foreground_pid(&self) -> Option<u32> {
        self.core
            .terminal_fact(&self.address)?
            .foreground
            .map(|p| p.pid)
    }
    pub(crate) fn foreground_cwd(&self) -> Option<PathBuf> {
        self.core
            .terminal_fact(&self.address)?
            .foreground
            .filter(|p| !p.cwd.is_empty())
            .map(|p| p.cwd.into())
    }
}

fn display(size: WinSize) -> Value {
    let appearance = crate::core::machine::appearance();
    let mut value = json!({"cell": [size.cell_w, size.cell_h]});
    if let Some([fg, bg, cursor]) = appearance.terminal_colors {
        value["fg"] = format!("#{fg:06x}").into();
        value["bg"] = format!("#{bg:06x}").into();
        value["cursor"] = format!("#{cursor:06x}").into();
    }
    value
}

pub(crate) struct HcMaster(pub(crate) Arc<Session>);
impl MasterPty for HcMaster {
    fn resize(&self, size: PtySize) -> anyhow::Result<()> {
        let size = WinSize {
            cols: size.cols,
            rows: size.rows,
            cell_w: size.pixel_width.checked_div(size.cols).unwrap_or(0),
            cell_h: size.pixel_height.checked_div(size.rows).unwrap_or(0),
        };
        *self.0.size.lock().unwrap() = size;
        self.0.send(FACTS, display(size).to_string().as_bytes())?;
        let mut bytes = size.cols.to_le_bytes().to_vec();
        bytes.extend(size.rows.to_le_bytes());
        self.0.send(RESIZE, &bytes)
    }
    fn get_size(&self) -> anyhow::Result<PtySize> {
        let size = *self.0.size.lock().unwrap();
        Ok(PtySize {
            rows: size.rows,
            cols: size.cols,
            pixel_width: size.cell_w.saturating_mul(size.cols),
            pixel_height: size.cell_h.saturating_mul(size.rows),
        })
    }
    fn try_clone_reader(&self) -> anyhow::Result<Box<dyn Read + Send>> {
        anyhow::bail!("reader supplied at spawn")
    }
    fn take_writer(&self) -> anyhow::Result<Box<dyn Write + Send>> {
        anyhow::bail!("writer supplied at spawn")
    }
    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<libc::pid_t> {
        self.0
            .foreground_pid()
            .and_then(|pid| i32::try_from(pid).ok())
    }
    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        None
    }
}

pub(crate) struct HcReader {
    session: Arc<Session>,
    rx: Receiver<StreamEvent>,
    buffer: Vec<u8>,
    offset: usize,
}
impl HcReader {
    #[cfg(test)]
    fn new(
        core: Arc<Core>,
        address: String,
        channel: u32,
        rx: Receiver<StreamEvent>,
        size: Arc<(AtomicU16, AtomicU16)>,
        exit: Arc<ChildExit>,
    ) -> Self {
        let epoch = core.epoch.load(Ordering::SeqCst);
        Self {
            session: Arc::new(Session {
                core,
                address,
                bound: Mutex::new(Some((epoch, channel))),
                available: Condvar::new(),
                size: Mutex::new(WinSize {
                    cols: size.0.load(Ordering::Relaxed),
                    rows: size.1.load(Ordering::Relaxed),
                    cell_w: 8,
                    cell_h: 16,
                }),
                exit,
                detached: AtomicBool::new(false),
                preserved: AtomicBool::new(false),
            }),
            rx,
            buffer: Vec::new(),
            offset: 0,
        }
    }
    fn try_reattach(&mut self) -> bool {
        let session = &self.session;
        if let Some((epoch, channel)) = session.bound.lock().unwrap().take() {
            session.core.detach(channel, epoch);
        }
        let deadline = Instant::now() + REATTACH_WINDOW;
        while !session.detached.load(Ordering::SeqCst) && Instant::now() < deadline {
            if session.core.ensure_connected().is_ok() {
                let terminal = if session.core.watching.load(Ordering::SeqCst) {
                    session
                        .core
                        .terminal_fact(&session.address)
                        .ok_or_else(|| anyhow::anyhow!("terminal not_found"))
                } else {
                    session.core.get(&session.address)
                };
                match terminal {
                    Ok(t) if t.state == "finished" => {
                        session.exit.set(t.exit_code.unwrap_or(1) as u32);
                        return false;
                    }
                    Ok(t) if t.state == "live" && t.pid.is_some() => {
                        let size = *session.size.lock().unwrap();
                        let epoch = session.core.epoch.load(Ordering::SeqCst);
                        match session.core.attach_display(
                            &session.address,
                            size.cols,
                            size.rows,
                            2000,
                            true,
                            display(size),
                        ) {
                            Ok((channel, _, rx)) => {
                                *session.bound.lock().unwrap() = Some((epoch, channel));
                                self.rx = rx;
                                session.available.notify_all();
                                return true;
                            }
                            Err(e) if gone(&e) => return false,
                            Err(e) => log::debug!("hc reattach pending: {e}"),
                        }
                    }
                    Err(e) if gone(&e) => return false,
                    _ => {}
                }
            }
            let mirror = session.core.mirror.lock().unwrap();
            let revision = mirror.revision;
            let _ = session
                .core
                .changed
                .wait_timeout_while(mirror, Duration::from_millis(500), |m| {
                    m.revision == revision
                })
                .unwrap();
        }
        false
    }
}
impl Read for HcReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            if self.session.detached.load(Ordering::SeqCst) {
                return Ok(0);
            }
            if self.offset < self.buffer.len() {
                let n = out.len().min(self.buffer.len() - self.offset);
                out[..n].copy_from_slice(&self.buffer[self.offset..self.offset + n]);
                self.offset += n;
                let _ = self.session.send(CREDIT, &(n as u32).to_le_bytes());
                if self.offset == self.buffer.len() {
                    self.buffer.clear();
                    self.offset = 0;
                }
                return Ok(n);
            }
            match self.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(StreamEvent::Data(bytes)) => {
                    self.buffer = bytes;
                    self.offset = 0;
                }
                Ok(StreamEvent::Close(payload)) => {
                    let (code, reason) = parse_close(&payload);
                    if reason_is_loss(reason.as_deref()) && self.try_reattach() {
                        continue;
                    }
                    let code = code
                        .or_else(|| {
                            self.session
                                .core
                                .terminal_fact(&self.session.address)
                                .and_then(|t| t.exit_code)
                        })
                        .or_else(|| {
                            self.session
                                .core
                                .get(&self.session.address)
                                .ok()
                                .and_then(|t| t.exit_code)
                        })
                        .unwrap_or(1);
                    // Preserve signed Windows status bits, as the native PTY
                    // does. The public pane protocol converts them back to i32.
                    self.session.exit.set(code as u32);
                    return Ok(0);
                }
                Ok(StreamEvent::Lost) | Err(RecvTimeoutError::Disconnected) => {
                    if self.try_reattach() {
                        continue;
                    }
                    self.session.exit.set(1);
                    return Ok(0);
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }
}
pub(crate) struct HcWriter(pub(crate) Arc<Session>);
impl Write for HcWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf == b"\x1b[I" || buf == b"\x1b[O" {
            self.0.focus(buf == b"\x1b[I");
        } else if !buf.is_empty() {
            self.0
                .send(DATA, buf)
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) struct HcChild {
    pub(crate) session: Arc<Session>,
    pub(crate) pid: Option<u32>,
}
impl fmt::Debug for HcChild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HcChild")
            .field("address", &self.session.address)
            .finish()
    }
}
impl ChildKiller for HcChild {
    fn kill(&mut self) -> io::Result<()> {
        self.session.close()
    }
    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(HcChild {
            session: self.session.clone(),
            pid: self.pid,
        })
    }
}
impl Child for HcChild {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        Ok(self.session.exit.get())
    }
    fn wait(&mut self) -> io::Result<ExitStatus> {
        Ok(self.session.exit.wait(Duration::from_secs(30)))
    }
    fn process_id(&self) -> Option<u32> {
        self.pid
    }
    #[cfg(windows)]
    fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        None
    }
}

// ---- tests ---------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{PipeReader, PipeWriter};

    /// The client's transport plus the fake core's ends of two pipes.
    pub(crate) fn transport() -> (
        Box<dyn Read + Send>,
        Box<dyn Write + Send>,
        BufReader<PipeReader>,
        PipeWriter,
    ) {
        let (client_read, fake_write) = std::io::pipe().unwrap();
        let (fake_read, client_write) = std::io::pipe().unwrap();
        (
            Box::new(client_read),
            Box::new(client_write),
            BufReader::new(fake_read),
            fake_write,
        )
    }

    pub(crate) fn hello_reply(generation: u64, version: &str, methods: &[&str]) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 0,
            "result": {
                "generation": generation,
                "version": version,
                "core": {"id": "deadbeef", "name": "test"},
                "framed": true,
                "methods": methods,
                "features": [],
            },
        })
    }

    pub(crate) fn full_methods() -> Vec<&'static str> {
        REQUIRED_METHODS.to_vec()
    }

    pub(crate) fn read_json_line(reader: &mut BufReader<PipeReader>) -> Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("fake reads hello");
        serde_json::from_str(line.trim()).expect("hello is JSON")
    }

    pub(crate) fn read_frame(reader: &mut BufReader<PipeReader>) -> (u32, u8, Vec<u8>) {
        let mut header = [0u8; HEADER];
        reader.read_exact(&mut header).expect("frame header");
        let channel = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let ty = header[4];
        let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).expect("frame payload");
        (channel, ty, payload)
    }

    pub(crate) fn write_frame_to(writer: &mut PipeWriter, channel: u32, ty: u8, payload: &[u8]) {
        let mut header = [0u8; HEADER];
        header[0..4].copy_from_slice(&channel.to_le_bytes());
        header[4] = ty;
        header[5..9].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        writer.write_all(&header).unwrap();
        writer.write_all(payload).unwrap();
        writer.flush().unwrap();
    }

    /// The unframed `hello` reply — the last newline-terminated line before
    /// the connection switches to frames.
    pub(crate) fn write_line_reply(writer: &mut PipeWriter, id: u64, result: Value) {
        let mut line = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
        .unwrap();
        line.push('\n');
        writer.write_all(line.as_bytes()).unwrap();
        writer.flush().unwrap();
    }

    /// A framed channel-0 reply, as every reply after `hello` travels.
    pub(crate) fn write_json_reply(writer: &mut PipeWriter, id: u64, result: Value) {
        let text = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
        .unwrap();
        write_frame_to(writer, 0, DATA, text.as_bytes());
    }

    pub(crate) fn terminal_doc(address: &str, state: &str) -> Value {
        json!({
            "address": address,
            "pid": 4242,
            "state": state,
            "cols": 80,
            "rows": 24,
            "labels": {"hitty.pane": "7", "hitty.owner": "abc"},
        })
    }

    #[test]
    fn handshake_refuses_a_newer_generation() {
        let (read, write, mut fake_read, mut fake_write) = transport();
        let fake = std::thread::spawn(move || {
            let hello = read_json_line(&mut fake_read);
            assert_eq!(hello["params"]["framed"], Value::Bool(true));
            write_line_reply(
                &mut fake_write,
                0,
                hello_reply(GENERATION + 1, "9.9.9", &full_methods())["result"].clone(),
            );
        });
        let err = Core::from_transport(read, write, None)
            .err()
            .expect("newer generation is refused");
        assert!(
            format!("{err:#}").contains("generation"),
            "unexpected error: {err:#}"
        );
        fake.join().unwrap();
    }

    #[test]
    fn handshake_refuses_a_missing_method() {
        let (read, write, mut fake_read, mut fake_write) = transport();
        let fake = std::thread::spawn(move || {
            read_json_line(&mut fake_read);
            let mut methods = full_methods();
            methods.pop();
            write_line_reply(
                &mut fake_write,
                0,
                hello_reply(GENERATION, "0.1.15", &methods)["result"].clone(),
            );
        });
        let err = Core::from_transport(read, write, None)
            .err()
            .expect("missing method is refused");
        assert!(
            format!("{err:#}").contains("missing methods"),
            "unexpected error: {err:#}"
        );
        fake.join().unwrap();
    }

    #[test]
    fn run_attach_roundtrip_with_early_frames() {
        let (read, write, mut fake_read, mut fake_write) = transport();
        let fake = std::thread::spawn(move || {
            read_json_line(&mut fake_read);
            write_line_reply(
                &mut fake_write,
                0,
                hello_reply(GENERATION, "0.1.15", &full_methods())["result"].clone(),
            );
            // terminal.run
            let (channel, ty, payload) = read_frame(&mut fake_read);
            assert_eq!((channel, ty), (0, DATA));
            let run: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(run["method"], "terminal.run");
            assert_eq!(run["params"]["labels"]["hitty.pane"], "7");
            write_json_reply(
                &mut fake_write,
                run["id"].as_u64().unwrap(),
                terminal_doc("t70", "live"),
            );
            // terminal.attach: first paint lands before the reply.
            let (channel, ty, payload) = read_frame(&mut fake_read);
            assert_eq!((channel, ty), (0, DATA));
            let attach: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(attach["method"], "terminal.attach");
            write_frame_to(&mut fake_write, 5, DATA, b"early-bytes");
            write_json_reply(
                &mut fake_write,
                attach["id"].as_u64().unwrap(),
                json!({"channel": 5, "terminal": terminal_doc("t70", "live")}),
            );
            // The client's credit grant.
            let (channel, ty, payload) = read_frame(&mut fake_read);
            assert_eq!((channel, ty), (5, CREDIT));
            assert_eq!(
                u32::from_le_bytes(payload[..4].try_into().unwrap()),
                ATTACH_CREDIT
            );
            // Client input arrives as DATA; resize as RESIZE.
            let (channel, ty, payload) = read_frame(&mut fake_read);
            assert_eq!((channel, ty, payload), (5, DATA, b"hi".to_vec()));
            let (channel, ty, payload) = read_frame(&mut fake_read);
            assert_eq!((channel, ty), (5, RESIZE));
            assert_eq!(
                (
                    u16::from_le_bytes([payload[0], payload[1]]),
                    u16::from_le_bytes([payload[2], payload[3]])
                ),
                (120, 40)
            );
            // End of stream.
            write_frame_to(&mut fake_write, 5, CLOSE, br#"{"exit_code": 3}"#);
        });

        let core = Core::from_transport(read, write, None).expect("handshake");
        let terminal = core
            .run(json!({
                "argv": ["cmd.exe"],
                "labels": {"hitty.pane": "7"},
                "no_parent": true,
            }))
            .expect("run");
        assert_eq!(terminal.address, "t70");
        let (channel, _terminal, rx) = core.attach("t70", 80, 24, 0, false).expect("attach");
        assert_eq!(channel, 5);
        match rx
            .recv_timeout(Duration::from_secs(5))
            .expect("early frame")
        {
            StreamEvent::Data(bytes) => assert_eq!(bytes, b"early-bytes"),
            other => panic!("expected early data, got {other:?}"),
        }
        core.input(5, b"hi").unwrap();
        core.send_resize(5, 120, 40).unwrap();
        match rx.recv_timeout(Duration::from_secs(5)).expect("close") {
            StreamEvent::Close(payload) => {
                assert_eq!(parse_close(&payload), (Some(3), None));
            }
            other => panic!("expected close, got {other:?}"),
        }
        fake.join().unwrap();
    }

    #[test]
    fn reader_replenishes_credit_and_reports_exit() {
        let (read, write, mut fake_read, mut fake_write) = transport();
        let fake = std::thread::spawn(move || {
            read_json_line(&mut fake_read);
            write_line_reply(
                &mut fake_write,
                0,
                hello_reply(GENERATION, "0.1.15", &full_methods())["result"].clone(),
            );
            let (_c, _t, payload) = read_frame(&mut fake_read);
            let attach: Value = serde_json::from_slice(&payload).unwrap();
            write_json_reply(
                &mut fake_write,
                attach["id"].as_u64().unwrap(),
                json!({"channel": 4, "terminal": terminal_doc("t4", "live")}),
            );
            let (channel, ty, _payload) = read_frame(&mut fake_read);
            assert_eq!((channel, ty), (4, CREDIT));
            write_frame_to(&mut fake_write, 4, DATA, b"0123456789");
            let (channel, ty, payload) = read_frame(&mut fake_read);
            assert_eq!((channel, ty), (4, CREDIT));
            assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 10);
            write_frame_to(&mut fake_write, 4, CLOSE, br#"{"exit_code": 7}"#);
        });

        let core = Core::from_transport(read, write, None).expect("handshake");
        let (channel, _terminal, rx) = core.attach("t4", 80, 24, 0, false).expect("attach");
        let exit = Arc::new(ChildExit::new());
        let size = Arc::new((AtomicU16::new(80), AtomicU16::new(24)));
        let mut reader = HcReader::new(core, "t4".to_string(), channel, rx, size, exit.clone());
        let mut out = [0u8; 32];
        let n = reader.read(&mut out).unwrap();
        assert_eq!(&out[..n], b"0123456789");
        let n = reader.read(&mut out).unwrap();
        assert_eq!(n, 0, "close is an EOF");
        assert_eq!(exit.get().unwrap().exit_code(), 7);
        fake.join().unwrap();
    }

    #[test]
    fn reader_reattaches_after_the_core_restart() {
        // Two transports: the first connection dies mid-stream; the second is
        // what the pane reattaches on.
        let (read1, write1, mut fake_read1, mut fake_write1) = transport();
        let (read2, write2, mut fake_read2, mut fake_write2) = transport();
        let mut second = Some((read2, write2));
        let fake1 = std::thread::spawn(move || {
            read_json_line(&mut fake_read1);
            write_line_reply(
                &mut fake_write1,
                0,
                hello_reply(GENERATION, "0.1.15", &full_methods())["result"].clone(),
            );
            let (_, _, payload) = read_frame(&mut fake_read1);
            let subscribe: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(subscribe["method"], "events.subscribe");
            write_json_reply(
                &mut fake_write1,
                subscribe["id"].as_u64().unwrap(),
                json!({"seq": 100, "channel": 20, "terminals": [terminal_doc("t4", "live")]}),
            );
            read_frame(&mut fake_read1); // event credit
            let (_c, _t, payload) = read_frame(&mut fake_read1);
            let attach: Value = serde_json::from_slice(&payload).unwrap();
            write_json_reply(
                &mut fake_write1,
                attach["id"].as_u64().unwrap(),
                json!({"channel": 4, "terminal": terminal_doc("t4", "live")}),
            );
            read_frame(&mut fake_read1); // credit
            write_frame_to(&mut fake_write1, 4, DATA, b"first-life");
            // Simulate the core going away: drop the connection.
            drop(fake_write1);
        });
        let fake2 = std::thread::spawn(move || {
            read_json_line(&mut fake_read2);
            write_line_reply(
                &mut fake_write2,
                0,
                hello_reply(GENERATION, "0.1.15", &full_methods())["result"].clone(),
            );
            // Re-subscribe after loss: seq restarts with the new core process.
            let (_c, _t, payload) = read_frame(&mut fake_read2);
            let subscribe: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(subscribe["method"], "events.subscribe");
            write_json_reply(
                &mut fake_write2,
                subscribe["id"].as_u64().unwrap(),
                json!({"seq": 1, "channel": 21, "terminals": [terminal_doc("t4", "live")]}),
            );
            read_frame(&mut fake_read2); // event credit
            let (_c, _t, payload) = read_frame(&mut fake_read2);
            let attach: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(attach["method"], "terminal.attach");
            assert_eq!(attach["params"]["resumed"], Value::Bool(true));
            write_json_reply(
                &mut fake_write2,
                attach["id"].as_u64().unwrap(),
                json!({"channel": 9, "terminal": terminal_doc("t4", "live")}),
            );
            read_frame(&mut fake_read2); // credit
            write_frame_to(&mut fake_write2, 9, DATA, b"second-life");
            let mut seen = Vec::new();
            while seen.len() < 4 {
                let (ch, kind, bytes) = read_frame(&mut fake_read2);
                assert_eq!(ch, 9, "every handle must follow the new channel");
                if kind == CREDIT {
                    continue;
                }
                seen.push(kind);
                match kind {
                    DATA => assert_eq!(bytes, b"after-reconnect"),
                    FOCUS => assert_eq!(bytes, [1]),
                    FACTS => assert_eq!(
                        serde_json::from_slice::<Value>(&bytes).unwrap()["cell"],
                        json!([9, 18])
                    ),
                    RESIZE => assert_eq!(bytes, [100, 0, 40, 0]),
                    _ => panic!("unexpected stream frame"),
                }
            }
            assert_eq!(seen, [DATA, FOCUS, FACTS, RESIZE]);
            write_frame_to(&mut fake_write2, 9, CLOSE, br#"{"exit_code": 0}"#);
        });

        let core = Core::from_transport(read1, write1, None).expect("first handshake");
        core.start_watch().unwrap();
        *core.connector.lock().unwrap() = Some(Box::new(move || {
            let (read, write) = second
                .take()
                .ok_or_else(|| anyhow::anyhow!("no more test connections"))?;
            Ok((read, write, None))
        }));
        let (channel, _terminal, rx) = core.attach("t4", 80, 24, 0, false).expect("attach");
        let exit = Arc::new(ChildExit::new());
        let size = Arc::new((AtomicU16::new(80), AtomicU16::new(24)));
        let mut reader = HcReader::new(core, "t4".to_string(), channel, rx, size, exit.clone());

        let mut collected = Vec::new();
        let mut out = [0u8; 64];
        loop {
            let n = reader.read(&mut out).unwrap();
            if n == 0 {
                break;
            }
            collected.extend_from_slice(&out[..n]);
            if collected.ends_with(b"second-life") {
                let mut writer = HcWriter(reader.session.clone());
                writer.write_all(b"after-reconnect").unwrap();
                writer.write_all(b"\x1b[I").unwrap();
                HcMaster(reader.session.clone())
                    .resize(PtySize {
                        cols: 100,
                        rows: 40,
                        pixel_width: 900,
                        pixel_height: 720,
                    })
                    .unwrap();
            }
        }
        let text = String::from_utf8_lossy(&collected);
        assert!(text.contains("first-life"), "text: {text:?}");
        assert!(text.contains("second-life"), "text: {text:?}");
        assert_eq!(exit.get().unwrap().exit_code(), 0);
        fake1.join().unwrap();
        fake2.join().unwrap();
    }

    #[test]
    fn event_snapshot_tracks_membership_and_ignores_stale_updates() {
        let (read, write, mut fake_read, mut fake_write) = transport();
        let (events, feed) = mpsc::channel::<Value>();
        let owner = owner_key();
        let mut initial = terminal_doc("t4", "live");
        initial["labels"] = pane_labels(4, &owner, Some("test"));
        let fake = std::thread::spawn(move || {
            read_json_line(&mut fake_read);
            write_line_reply(
                &mut fake_write,
                0,
                hello_reply(GENERATION, MIN_VERSION, &full_methods())["result"].clone(),
            );
            let (_, _, payload) = read_frame(&mut fake_read);
            let subscribe: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(subscribe["method"], "events.subscribe");
            write_json_reply(
                &mut fake_write,
                subscribe["id"].as_u64().unwrap(),
                json!({"seq": 10, "channel": 20, "terminals": [initial]}),
            );
            read_frame(&mut fake_read); // initial credit
            while let Ok(event) = feed.recv_timeout(Duration::from_secs(5)) {
                let line = format!("{event}\n");
                let split = line.len() / 2;
                // Events are newline-delimited; frame boundaries are arbitrary.
                write_frame_to(&mut fake_write, 20, DATA, &line.as_bytes()[..split]);
                write_frame_to(&mut fake_write, 20, DATA, &line.as_bytes()[split..]);
                read_frame(&mut fake_read); // credit for both fragments
                read_frame(&mut fake_read);
            }
        });
        let core = Core::from_transport(read, write, None).unwrap();
        core.start_watch().unwrap();
        assert_eq!(core.owned_terminals().len(), 1);
        let wait = |condition: &dyn Fn() -> bool| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while !condition() {
                assert!(
                    Instant::now() < deadline,
                    "lifecycle event was not reflected"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let event = |seq, kind, address, data| {
            json!({"method": "event", "params": {
                "seq": seq, "type": kind, "terminal": address, "data": data
            }})
        };
        let mut added = terminal_doc("t5", "live");
        added["labels"] = pane_labels(5, &owner, Some("test"));
        events
            .send(event(
                11,
                "labels",
                "t5",
                json!({
                    "labels": added["labels"], "record": added
                }),
            ))
            .unwrap();
        wait(&|| core.owned_terminals().len() == 2);
        events
            .send(event(12, "labels", "t4", json!({"labels": {}})))
            .unwrap();
        wait(&|| core.terminal_fact("t4").is_none());
        added["pid"] = 500.into();
        events
            .send(json!({"method": "missed", "params": {
                "seq": 20, "terminals": [added]
            }}))
            .unwrap();
        events.send(event(19, "removed", "t5", json!({}))).unwrap();
        events
            .send(json!({"method": "missed", "params": {
                "seq": 18, "terminals": []
            }}))
            .unwrap();
        events
            .send(event(
                21,
                "foreground",
                "t5",
                json!({"pid": 501, "cwd": "test-cwd"}),
            ))
            .unwrap();
        wait(&|| {
            core.terminal_fact("t5").is_some_and(|t| {
                t.pid == Some(500) && t.foreground.is_some_and(|p| p.cwd == "test-cwd")
            })
        });
        events
            .send(event(22, "lost", "t5", json!({"exit_code": null})))
            .unwrap();
        wait(&|| {
            core.terminal_fact("t5")
                .is_some_and(|t| t.pid.is_none() && t.state == "live")
        });
        added["pid"] = 600.into();
        events.send(event(23, "restarted", "t5", added)).unwrap();
        wait(&|| core.terminal_fact("t5").is_some_and(|t| t.pid == Some(600)));
        events
            .send(event(24, "finished", "t5", json!({"exit_code": 3})))
            .unwrap();
        wait(&|| {
            core.terminal_fact("t5")
                .is_some_and(|t| t.state == "finished" && t.exit_code == Some(3))
        });
        events.send(event(25, "removed", "t5", json!({}))).unwrap();
        wait(&|| core.owned_terminals().is_empty());
        drop(events);
        fake.join().unwrap();
    }

    #[test]
    fn version_comparison_refuses_unparseable_strings() {
        assert!(version_lt("0.1.9", "0.1.10"));
        assert!(!version_lt("0.1.10", "0.1.10"));
        assert!(!version_lt("0.2.0", "0.1.10"));
        assert!(version_lt("garbage", "0.1.10"));
    }

    /// End to end against the real core, when one is installed: run a program
    /// in a core terminal, attach, read its output, and remove it again.
    /// Skips cleanly when `hc` is not on this machine, and only ever touches
    /// the terminal it started itself.
    #[test]
    fn real_core_roundtrip_when_installed() {
        if locate().is_err() {
            eprintln!("herdr-core not installed; skipping the real-core roundtrip");
            return;
        }
        let core = Core::connect().expect("connect to the real core");
        let argv: Vec<String> = if cfg!(windows) {
            vec![
                "cmd.exe".into(),
                "/C".into(),
                "echo HITTYTEST %HC_TERMINAL_ID% & exit /b 3".into(),
            ]
        } else {
            vec![
                "/bin/sh".into(),
                "-c".into(),
                "echo HITTYTEST $HC_TERMINAL_ID; exit 3".into(),
            ]
        };
        let terminal = core
            .run(json!({
                "argv": argv,
                "labels": {"hitty.test": std::process::id().to_string()},
                "no_parent": true,
                "no_notify": true,
            }))
            .expect("run a test terminal");
        let (channel, _attached, rx) = core
            .attach(&terminal.address, 80, 24, 0, false)
            .expect("attach the test terminal");

        let mut output = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(StreamEvent::Data(bytes)) => output.extend_from_slice(&bytes),
                Ok(StreamEvent::Close(_)) => break,
                Ok(StreamEvent::Lost) => break,
                Err(RecvTimeoutError::Timeout) => {
                    if Instant::now() >= deadline {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = channel; // kept for readability: the stream ends by itself here

        // Record the exit code before removing the terminal.
        let mut code = None;
        for _ in 0..20 {
            if let Ok(terminal) = core.get(&terminal.address) {
                if let Some(exit_code) = terminal.exit_code {
                    code = Some(exit_code);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = core.kill(&terminal.address);
        let _ = core.remove(&terminal.address);

        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("HITTYTEST t"), "output: {text:?}");
        assert_eq!(code, Some(3), "exit code; output: {text:?}");
    }
}
