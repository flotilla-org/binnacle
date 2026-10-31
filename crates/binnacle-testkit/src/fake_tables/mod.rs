//! `fake-tables`: a fake described-table server for tests and demos, over
//! `std::net` and threads. It serves tables shaped like flotillad's
//! (`GET /tables`, `GET /tables/{t}`, `GET /tables/{t}/rows`) from recorded
//! fixtures or generated data, over TCP or a Unix socket, and plays a
//! script of live changes to the watches open on it.
//!
//! - A read (`/rows` without `watch`) sends `table`, one page of `rows`
//!   (`limit`, else 200, from `cursor`, with an opaque cursor and `partial:
//!   true` while more remain), `synced`, then ends.
//! - A watch sends `table`, one `rows` of up to `limit`, `synced`, then the
//!   script's changes as they play, with `progress` heartbeats.
//! - Rows live in a namespace, `flotilla` by default. A request without
//!   `namespace` reads `default`: an empty table, and no error, as
//!   flotillad does.
//! - `where` takes `.field.path == value`; anything else is a 400. An
//!   unknown table is a 404.
//!
//! The script starts playing once a watch has synced; [`Running::push`]
//! adds steps to it as a test goes.

mod data;
mod http;
mod script;

use std::collections::VecDeque;
use std::io;
use std::net::{SocketAddr, TcpListener};
#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use binnacle_described::{
    Encoding, Message, Row, RowsMessage, TableMessage, Wire, deterministic_cbor, encode_cbor_item,
    encode_json_line,
};
use serde_json::json;

pub use data::{FakeTable, Where};
pub use http::{Addr, Request, Response, Stream, fetch, open};
pub use script::{Step, parse_script};

use http::Conn;

/// How often waiting threads look up to see whether the fake stopped.
const TICK: Duration = Duration::from_millis(20);

/// A fake to start: its tables, its script and where it listens.
#[derive(Clone, Debug)]
pub struct FakeTables {
    tables: Vec<FakeTable>,
    script: Vec<Step>,
    listen: Listen,
    page: usize,
    heartbeat: Duration,
    fail: Vec<u16>,
    answer_in: Option<Encoding>,
}

#[derive(Clone, Debug)]
enum Listen {
    Tcp(SocketAddr),
    #[cfg(unix)]
    Unix(PathBuf),
}

impl Default for FakeTables {
    fn default() -> Self {
        FakeTables::new()
    }
}

impl FakeTables {
    /// No tables yet, on a free port of 127.0.0.1, 200 rows a page and a
    /// heartbeat a second.
    pub fn new() -> FakeTables {
        FakeTables {
            tables: Vec::new(),
            script: Vec::new(),
            listen: Listen::Tcp(SocketAddr::from(([127, 0, 0, 1], 0))),
            page: 200,
            heartbeat: Duration::from_secs(1),
            fail: Vec::new(),
            answer_in: None,
        }
    }

    pub fn table(mut self, table: FakeTable) -> FakeTables {
        self.tables.push(table);
        self
    }

    /// Serves the table a recorded stream holds ([`FakeTable::from_fixture`]).
    pub fn fixture(self, path: impl AsRef<std::path::Path>) -> Result<FakeTables, String> {
        Ok(self.table(FakeTable::from_fixture(path)?))
    }

    /// Serves a made table of `rows` rows ([`FakeTable::generated`]).
    pub fn generate(self, name: impl Into<String>, rows: usize) -> FakeTables {
        self.table(FakeTable::generated(name, rows))
    }

    /// Adds `steps` to the script.
    pub fn script(mut self, steps: impl IntoIterator<Item = Step>) -> FakeTables {
        self.script.extend(steps);
        self
    }

    pub fn tcp(mut self, addr: SocketAddr) -> FakeTables {
        self.listen = Listen::Tcp(addr);
        self
    }

    /// Listens on a Unix socket at `path`, which it removes when it stops.
    #[cfg(unix)]
    pub fn unix(mut self, path: impl Into<PathBuf>) -> FakeTables {
        self.listen = Listen::Unix(path.into());
        self
    }

    /// How many rows a read sends at most, without a `limit`.
    pub fn page(mut self, rows: usize) -> FakeTables {
        self.page = rows.max(1);
        self
    }

    /// How long a quiet watch waits between `progress` heartbeats.
    pub fn heartbeat(mut self, every: Duration) -> FakeTables {
        self.heartbeat = every;
        self
    }

    /// Answers the first rows request with `status`, before any script
    /// plays; called again, the next one too.
    pub fn failing(mut self, status: u16) -> FakeTables {
        self.fail.push(status);
        self
    }

    /// Answers every rows request in `encoding`, whatever it accepts.
    pub fn answer_in(mut self, encoding: Encoding) -> FakeTables {
        self.answer_in = Some(encoding);
        self
    }

    pub fn start(self) -> io::Result<Running> {
        let (listener, addr) = match &self.listen {
            Listen::Tcp(addr) => {
                let listener = TcpListener::bind(addr)?;
                let addr = Addr::Tcp(listener.local_addr()?);
                (Listener::Tcp(listener), addr)
            }
            #[cfg(unix)]
            Listen::Unix(path) => {
                let _ = std::fs::remove_file(path);
                (
                    Listener::Unix(UnixListener::bind(path)?),
                    Addr::Unix(path.clone()),
                )
            }
        };
        listener.set_nonblocking(true)?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                tables: self.tables,
                queue: self.script.into(),
                started: false,
                playing: false,
                watchers: Vec::new(),
                requests: Vec::new(),
                sequence: 1,
                fail: self.fail.into(),
                answer_in: self.answer_in,
                page: self.page,
                heartbeat: self.heartbeat,
                next: 0,
            }),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
        });
        let accept = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || accept(listener, shared))
        };
        let player = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || play(shared))
        };
        Ok(Running {
            addr,
            shared,
            threads: vec![accept, player],
        })
    }
}

/// A fake that is running. It stops when dropped.
pub struct Running {
    addr: Addr,
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl Running {
    pub fn addr(&self) -> &Addr {
        &self.addr
    }

    /// The base URL of a fake on TCP, such as `http://127.0.0.1:7777`.
    pub fn url(&self) -> Option<String> {
        match &self.addr {
            Addr::Tcp(addr) => Some(format!("http://{addr}")),
            #[cfg(unix)]
            Addr::Unix(_) => None,
        }
    }

    /// Adds `step` to the end of the script.
    pub fn push(&self, step: Step) {
        self.shared.lock().queue.push_back(step);
        self.shared.wake.notify_all();
    }

    /// Every request taken so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.shared.lock().requests.clone()
    }

    /// How many watches are open and synced.
    pub fn watches(&self) -> usize {
        self.shared.lock().watchers.len()
    }

    /// Waits until the script has played out, or `timeout` passes. Returns
    /// whether it did.
    pub fn settle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.lock();
        while !state.queue.is_empty() || state.playing {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            state = self
                .shared
                .wake
                .wait_timeout(state, left.min(TICK))
                .unwrap()
                .0;
        }
        true
    }

    /// A table as the fake holds it now.
    pub fn table(&self, name: &str) -> Option<FakeTable> {
        let state = self.shared.lock();
        state.index(name).map(|i| state.tables[i].clone())
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        #[cfg(unix)]
        if let Addr::Unix(path) = &self.addr {
            let _ = std::fs::remove_file(path);
        }
    }
}

enum Listener {
    Tcp(TcpListener),
    #[cfg(unix)]
    Unix(UnixListener),
}

impl Listener {
    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        match self {
            Listener::Tcp(l) => l.set_nonblocking(nonblocking),
            #[cfg(unix)]
            Listener::Unix(l) => l.set_nonblocking(nonblocking),
        }
    }

    fn accept(&self) -> io::Result<Conn> {
        match self {
            Listener::Tcp(l) => l.accept().map(|(s, _)| Conn::Tcp(s)),
            #[cfg(unix)]
            Listener::Unix(l) => l.accept().map(|(s, _)| Conn::Unix(s)),
        }
    }
}

struct Shared {
    state: Mutex<State>,
    /// Wakes the player when a step comes or a watch syncs, and those
    /// waiting for it to settle.
    wake: Condvar,
    stop: AtomicBool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

struct State {
    tables: Vec<FakeTable>,
    queue: VecDeque<Step>,
    /// A watch has synced: the script plays.
    started: bool,
    /// A step taken from the queue is being played.
    playing: bool,
    watchers: Vec<Watcher>,
    requests: Vec<Request>,
    sequence: u64,
    /// Statuses the next rows requests are answered with.
    fail: VecDeque<u16>,
    /// The encoding every rows request is answered in, if not as asked.
    answer_in: Option<Encoding>,
    page: usize,
    heartbeat: Duration,
    next: u64,
}

/// An open watch: what it reads, and where its messages go.
struct Watcher {
    id: u64,
    table: String,
    view: View,
    tx: Sender<Vec<Out>>,
    /// For a table with no primary key, the place this watch's stream gave
    /// each of the table's rows (`None` for one it was not sent), and the
    /// next place.
    places: Places,
}

#[derive(Default)]
struct Places {
    of: Vec<Option<u64>>,
    next: u64,
}

/// What a request reads of a table: its namespace, `where` clauses and
/// limit.
#[derive(Clone)]
struct View {
    namespace: String,
    wheres: Vec<Where>,
    limit: usize,
}

impl View {
    fn keeps(&self, table: &FakeTable, row: &Row) -> bool {
        self.namespace == table.namespace && self.wheres.iter().all(|w| w.keeps(row))
    }

    fn rows<'a>(&'a self, table: &'a FakeTable) -> impl Iterator<Item = &'a Row> + 'a {
        table.rows.iter().filter(|r| self.keeps(table, r))
    }

    /// The places a stream that sends `table` whole ([`State::whole`])
    /// gives its rows, for a table with no primary key: those it keeps, up
    /// to the limit, in order.
    fn places(&self, table: &FakeTable) -> Places {
        let mut places = Places::default();
        if table.keyed() {
            return places;
        }
        for row in &table.rows {
            let sent = self.keeps(table, row) && places.next < self.limit as u64;
            places.of.push(sent.then_some(places.next));
            places.next += u64::from(sent);
        }
        places
    }
}

enum Out {
    Message(Message),
    Garbage,
    End,
}

impl State {
    /// The table named `name`; the first for an empty name.
    fn index(&self, name: &str) -> Option<usize> {
        match name {
            "" => (!self.tables.is_empty()).then_some(0),
            _ => self.tables.iter().position(|t| t.name == name),
        }
    }

    fn table_message(&self, table: &FakeTable) -> Message {
        Message::Table(TableMessage {
            schema: table.schema.clone(),
            incarnation: table.incarnation.clone(),
            table: Some(table.name.clone()),
            sequence: Some(self.sequence),
        })
    }

    /// `table`, a `rows` of up to `view.limit` rows, and `synced`.
    fn whole(&self, table: &FakeTable, view: &View) -> Vec<Message> {
        let mut rows = view.rows(table);
        let page: Vec<Row> = rows.by_ref().take(view.limit).cloned().collect();
        let more = rows.next().is_some();
        vec![
            self.table_message(table),
            Message::Rows(RowsMessage {
                rows: page,
                cursor: more.then(|| format!("o{}", view.limit)),
                partial: more,
                sequence: Some(self.sequence),
            }),
            Message::Synced {
                sequence: Some(self.sequence),
            },
        ]
    }

    /// Sends `out(watcher)` to each watch on `table`, dropping those gone.
    fn tell(&mut self, table: &str, out: impl Fn(&FakeTable, &View) -> Vec<Out>) {
        let Some(i) = self.index(table) else {
            return;
        };
        let name = self.tables[i].name.clone();
        let table = &self.tables[i];
        self.watchers.retain(|w| {
            if w.table != name {
                return true;
            }
            let outs = out(table, &w.view);
            outs.is_empty() || w.tx.send(outs).is_ok()
        });
    }

    fn apply(&mut self, step: Step) {
        match step {
            Step::Upsert { table, row } => {
                let Some(i) = self.index(&table) else {
                    return;
                };
                if !self.tables[i].keyed() {
                    self.tables[i].upsert(row.clone());
                    self.sequence += 1;
                    let t = &self.tables[i];
                    self.watchers.retain_mut(|w| {
                        if w.table != t.name {
                            return true;
                        }
                        let sent = w.view.keeps(t, &row);
                        w.places.of.push(sent.then_some(w.places.next));
                        if !sent {
                            return true;
                        }
                        let key = vec![Wire::Int(w.places.next.into())];
                        w.places.next += 1;
                        let upsert = Message::Upsert {
                            key,
                            row: row.clone(),
                        };
                        w.tx.send(vec![Out::Message(upsert)]).is_ok()
                    });
                    return;
                }
                let old = self.tables[i].upsert(row.clone());
                self.sequence += 1;
                self.tell(&table, |t, view| {
                    let key = t.key(&row);
                    match (
                        view.keeps(t, &row),
                        old.as_ref().is_some_and(|o| view.keeps(t, o)),
                    ) {
                        (true, _) => vec![Out::Message(Message::Upsert {
                            key,
                            row: row.clone(),
                        })],
                        (false, true) => vec![Out::Message(Message::Delete { key })],
                        (false, false) => Vec::new(),
                    }
                });
            }
            Step::Delete { table, key } => {
                let Some(i) = self.index(&table) else {
                    return;
                };
                let Some(at) = self.tables[i].find(&key) else {
                    return;
                };
                let old = self.tables[i].rows.remove(at);
                self.sequence += 1;
                if !self.tables[i].keyed() {
                    let name = self.tables[i].name.clone();
                    self.watchers.retain_mut(|w| {
                        if w.table != name {
                            return true;
                        }
                        let Some(place) = w.places.of.remove(at) else {
                            return true;
                        };
                        let key = vec![Wire::Int(place.into())];
                        w.tx.send(vec![Out::Message(Message::Delete { key })])
                            .is_ok()
                    });
                    return;
                }
                self.tell(&table, |t, view| match view.keeps(t, &old) {
                    true => vec![Out::Message(Message::Delete { key: key.clone() })],
                    false => Vec::new(),
                });
            }
            Step::Resync { table } => {
                self.sequence += 1;
                let whole = |state: &State, t: &FakeTable, view: &View| {
                    let mut outs = vec![Out::Message(Message::Resync)];
                    outs.extend(state.whole(t, view).into_iter().map(Out::Message));
                    outs
                };
                let messages: Vec<(u64, Vec<Out>)> = self.for_watchers(&table, whole);
                self.send(messages);
                if let Some(i) = self.index(&table) {
                    let t = &self.tables[i];
                    for w in self.watchers.iter_mut().filter(|w| w.table == t.name) {
                        w.places = w.view.places(t);
                    }
                }
            }
            Step::AddField {
                table,
                field,
                value,
            } => {
                let Some(i) = self.index(&table) else {
                    return;
                };
                let t = &mut self.tables[i];
                if !t.schema.fields.iter().any(|f| f.name == field.name) {
                    t.schema.fields.push(field.clone());
                }
                for row in &mut t.rows {
                    row.insert(field.name.clone(), value.clone());
                }
                self.schema_changed(&table);
            }
            Step::RemoveField { table, name } => {
                let Some(i) = self.index(&table) else {
                    return;
                };
                let t = &mut self.tables[i];
                t.schema.fields.retain(|f| f.name != name);
                for row in &mut t.rows {
                    row.remove(&name);
                }
                self.schema_changed(&table);
            }
            Step::End => {
                for watcher in self.watchers.drain(..) {
                    let _ = watcher.tx.send(vec![Out::End]);
                }
            }
            Step::Sleep(_) => {}
            Step::Fail { status } => self.fail.push_back(status),
            Step::Garbage { table } => self.tell(&table, |_, _| vec![Out::Garbage]),
        }
    }

    /// Watches get the new `table`, then a `rows` with every row.
    fn schema_changed(&mut self, table: &str) {
        self.sequence += 1;
        let changed = |state: &State, t: &FakeTable, view: &View| {
            let mut whole = state.whole(t, view);
            whole.pop();
            whole.into_iter().map(Out::Message).collect()
        };
        let messages = self.for_watchers(table, changed);
        self.send(messages);
    }

    fn for_watchers(
        &self,
        table: &str,
        out: impl Fn(&State, &FakeTable, &View) -> Vec<Out>,
    ) -> Vec<(u64, Vec<Out>)> {
        let Some(i) = self.index(table) else {
            return Vec::new();
        };
        let t = &self.tables[i];
        (self.watchers.iter())
            .filter(|w| w.table == t.name)
            .map(|w| (w.id, out(self, t, &w.view)))
            .collect()
    }

    fn send(&mut self, messages: Vec<(u64, Vec<Out>)>) {
        for (id, outs) in messages {
            if let Some(at) = self.watchers.iter().position(|w| w.id == id)
                && self.watchers[at].tx.send(outs).is_err()
            {
                self.watchers.remove(at);
            }
        }
    }
}

fn accept(listener: Listener, shared: Arc<Shared>) {
    while !shared.stopped() {
        match listener.accept() {
            Ok(conn) => {
                let shared = Arc::clone(&shared);
                thread::spawn(move || serve(conn, shared));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(_) => thread::sleep(TICK),
        }
    }
}

fn play(shared: Arc<Shared>) {
    loop {
        let step = {
            let mut state = shared.lock();
            loop {
                if shared.stopped() {
                    return;
                }
                if state.started
                    && let Some(step) = state.queue.pop_front()
                {
                    state.playing = true;
                    break step;
                }
                state = shared.wake.wait_timeout(state, TICK).unwrap().0;
            }
        };
        if let Step::Sleep(wait) = step {
            let until = Instant::now() + wait;
            while Instant::now() < until && !shared.stopped() {
                thread::sleep(TICK.min(until.saturating_duration_since(Instant::now())));
            }
        }
        let mut state = shared.lock();
        state.apply(step);
        state.playing = false;
        drop(state);
        shared.wake.notify_all();
    }
}

fn error(code: &str, message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"code": code, "message": message})).expect("JSON")
}

fn serve(mut conn: Conn, shared: Arc<Shared>) {
    let _ = conn.set_nonblocking(false);
    let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(request) = http::read_request(&mut conn) else {
        return;
    };
    shared.lock().requests.push(request.clone());
    let json = "application/json";
    if request.method != "GET" {
        let _ = http::respond(
            &mut conn,
            405,
            json,
            &error("method_not_allowed", "only GET"),
        );
        return;
    }
    let path: Vec<String> = (request.path().trim_matches('/').split('/'))
        .map(|p| {
            let request = Request {
                method: String::new(),
                target: format!("?{p}"),
                headers: Vec::new(),
                at: Instant::now(),
            };
            request.query().pop().map(|(k, _)| k).unwrap_or_default()
        })
        .collect();
    let accept = request
        .header("accept")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let _ = match path.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["tables"] => {
            let state = shared.lock();
            let kinds: Vec<serde_json::Value> = (state.tables.iter())
                .map(|t| {
                    let mut kind: String = t.name.trim_end_matches('s').to_owned();
                    if let Some(first) = kind.get_mut(..1) {
                        first.make_ascii_uppercase();
                    }
                    json!({"kind": kind, "root": t.name, "tables": [t.name]})
                })
                .collect();
            let body = serde_json::to_vec(&json!({ "kinds": kinds })).expect("JSON");
            drop(state);
            http::respond(&mut conn, 200, json, &body)
        }
        ["tables", name] => {
            let state = shared.lock();
            match state.index(name).filter(|_| !name.is_empty()) {
                None => {
                    drop(state);
                    http::respond(&mut conn, 404, json, &error("not_found", "unknown table"))
                }
                Some(i) => {
                    let schema = state.tables[i].schema.to_json();
                    drop(state);
                    match accept.contains("application/cbor") && !accept.contains("cbor-seq") {
                        true => {
                            let body = deterministic_cbor(&Wire::from_json(&schema));
                            http::respond(&mut conn, 200, "application/cbor", &body)
                        }
                        false => {
                            let body = serde_json::to_vec(&schema).expect("JSON");
                            http::respond(&mut conn, 200, json, &body)
                        }
                    }
                }
            }
        }
        ["tables", name, "rows"] => rows(conn, &shared, &request, name, &accept),
        _ => http::respond(&mut conn, 404, json, &error("not_found", "no such route")),
    };
}

/// Answers a rows request: a read, or a watch until its stream ends.
fn rows(
    mut conn: Conn,
    shared: &Arc<Shared>,
    request: &Request,
    name: &str,
    accept: &str,
) -> io::Result<()> {
    let json = "application/json";
    let query = request.query();
    let param = |n: &str| query.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
    let watch = param("watch").is_some();
    let encoding = match (
        shared.lock().answer_in,
        accept.contains("application/cbor-seq"),
    ) {
        (Some(encoding), _) => encoding,
        (None, true) => Encoding::CborSeq,
        (None, false) => Encoding::Ndjson,
    };
    let mut wheres = Vec::new();
    for (_, clause) in query.iter().filter(|(k, _)| k == "where") {
        match Where::parse(clause) {
            Ok(w) => wheres.push(w),
            Err(message) => {
                return http::respond(&mut conn, 400, json, &error("invalid_filter", &message));
            }
        }
    }
    let offset = match param("cursor") {
        None => 0,
        Some(cursor) => match cursor.strip_prefix('o').and_then(|n| n.parse().ok()) {
            Some(offset) => offset,
            None => {
                return http::respond(
                    &mut conn,
                    400,
                    json,
                    &error("invalid_cursor", "a cursor this fake did not give"),
                );
            }
        },
    };
    let (tx, rx) = mpsc::channel();
    let (id, messages, heartbeat) = {
        let mut state = shared.lock();
        if let Some(status) = state.fail.pop_front() {
            drop(state);
            return http::respond(
                &mut conn,
                status,
                json,
                &error("internal", "the fake failed this request"),
            );
        }
        let Some(i) = state.index(name).filter(|_| !name.is_empty()) else {
            drop(state);
            return http::respond(&mut conn, 404, json, &error("not_found", "unknown table"));
        };
        let limit = param("limit")
            .and_then(|l| l.parse().ok())
            .unwrap_or(state.page);
        let view = View {
            namespace: param("namespace").unwrap_or("default").to_owned(),
            wheres,
            limit,
        };
        let table = &state.tables[i];
        let messages = match watch {
            true => state.whole(table, &view),
            false => {
                let mut rows = view.rows(table).skip(offset);
                let page: Vec<Row> = rows.by_ref().take(limit).cloned().collect();
                let more = rows.next().is_some();
                vec![
                    state.table_message(table),
                    Message::Rows(RowsMessage {
                        rows: page,
                        cursor: more.then(|| format!("o{}", offset + limit)),
                        partial: more || offset > 0,
                        sequence: Some(state.sequence),
                    }),
                    Message::Synced {
                        sequence: Some(state.sequence),
                    },
                ]
            }
        };
        let places = view.places(table);
        let table = table.name.clone();
        state.next += 1;
        let id = state.next;
        if watch {
            state.watchers.push(Watcher {
                id,
                table,
                view,
                tx,
                places,
            });
        }
        (id, messages, state.heartbeat)
    };
    let write = |conn: &mut Conn, out: &Out| match out {
        Out::Message(message) => http::chunk(conn, &encode(encoding, message)),
        Out::Garbage => http::chunk(
            conn,
            &match encoding {
                Encoding::Ndjson => b"garbage\n".to_vec(),
                Encoding::CborSeq => deterministic_cbor(&Wire::Text("garbage".into())),
            },
        ),
        Out::End => http::chunk(conn, b""),
    };
    let result = (|| {
        http::start_chunked(&mut conn, encoding.content_type())?;
        for message in messages {
            write(&mut conn, &Out::Message(message))?;
        }
        if !watch {
            return http::chunk(&mut conn, b"");
        }
        shared.lock().started = true;
        shared.wake.notify_all();
        let mut beat = Instant::now();
        loop {
            match rx.recv_timeout(TICK) {
                Ok(outs) => {
                    for out in &outs {
                        write(&mut conn, out)?;
                        if let Out::End = out {
                            return Ok(());
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) if shared.stopped() => {
                    return http::chunk(&mut conn, b"");
                }
                Err(RecvTimeoutError::Timeout) => {
                    if beat.elapsed() >= heartbeat {
                        beat = Instant::now();
                        let sequence = shared.lock().sequence;
                        let progress = Message::Progress {
                            sequence: Some(sequence),
                        };
                        write(&mut conn, &Out::Message(progress))?;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    })();
    shared.lock().watchers.retain(|w| w.id != id);
    result
}

fn encode(encoding: Encoding, message: &Message) -> Vec<u8> {
    match encoding {
        Encoding::Ndjson => encode_json_line(message),
        Encoding::CborSeq => encode_cbor_item(message),
    }
}
