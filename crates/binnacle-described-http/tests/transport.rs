//! The HTTP transport against an in-process fake table server: both
//! encodings, live changes, reconnecting as a resync with jittered backoff,
//! the idle timeout, credentials sent exactly as given, errors, Unix
//! sockets, and tokio kept out of the other crates.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use binnacle_core::{Axis, AxisConfig, Grid, Viewport};
use binnacle_described::{DescribedSource, Encoding, Message, Received, Scope, Wire};
use binnacle_described_http::{
    Auth, Backoff, Blocking, Endpoint, Event, Options, Transport, Unserved,
};
use binnacle_testkit::fake_tables::{FakeTables, Running, Step};

fn crews() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../binnacle-described/tests/fixtures/crews.ndjson")
}

fn fake() -> FakeTables {
    FakeTables::new().fixture(crews()).unwrap()
}

fn options(fake: &Running) -> Options {
    Options::new(Endpoint::Tcp(fake.url().unwrap()))
}

fn source() -> DescribedSource {
    DescribedSource::new("crews", Scope::new("flotilla"))
}

const WAIT: Duration = Duration::from_secs(10);

/// A sync host's loop: feeds each event to `source` (calling `ended()` on
/// an end and opening what it asks), until `until` holds after an event.
/// Returns the events.
fn host(
    transport: &Blocking,
    source: &mut DescribedSource,
    until: impl Fn(&DescribedSource, &Event) -> bool,
) -> Vec<Event> {
    let deadline = Instant::now() + WAIT;
    let mut events = Vec::new();
    loop {
        if let Some(request) = source.poll_request() {
            transport.open(request);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        let event = transport.recv_timeout(left).expect("an event in time");
        match &event {
            Event::Message(message) => {
                source.receive(message.clone());
            }
            Event::Ended => {
                source.ended();
            }
            _ => {}
        }
        let done = until(source, &event);
        events.push(event);
        if done {
            return events;
        }
    }
}

fn synced(source: &DescribedSource, _: &Event) -> bool {
    source.status().synced
}

#[tokio::test(flavor = "current_thread")]
async fn the_same_table_as_ndjson_and_cbor_gives_equal_messages() {
    let mut sequences = Vec::new();
    let mut accepted = Vec::new();
    for encoding in [Encoding::Ndjson, Encoding::CborSeq] {
        let fake = fake().start().unwrap();
        // The script plays once the watch syncs: an upsert, then the end.
        let row = [("name".to_owned(), Wire::Text("golf".into()))].into();
        fake.push(Step::Upsert {
            table: "crews".into(),
            row,
        });
        fake.push(Step::End);
        let transport = Transport::new(options(&fake).encoding(encoding));
        let request = source().poll_request().unwrap();
        let mut events = Vec::new();
        transport.watch(&request, |event| events.push(event)).await;
        sequences.push(events);
        accepted.push(fake.requests()[0].header("accept").unwrap().to_owned());
    }
    let types: Vec<&str> = (sequences[0].iter())
        .map(|e| match e {
            Event::Message(Message::Table(_)) => "table",
            Event::Message(Message::Rows(_)) => "rows",
            Event::Message(Message::Synced { .. }) => "synced",
            Event::Message(Message::Upsert { .. }) => "upsert",
            Event::Message(Message::Count { .. }) => "count",
            Event::Ended => "ended",
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        types,
        [
            "table", "count", "rows", "synced", "upsert", "count", "ended"
        ]
    );
    assert_eq!(accepted, ["application/x-ndjson", "application/cbor-seq"]);
    assert_eq!(sequences[0], sequences[1]);
}

#[test]
fn the_encoding_follows_the_content_type_not_the_one_asked_for() {
    let fake = fake().answer_in(Encoding::CborSeq).start().unwrap();
    let transport = Blocking::new(options(&fake)).unwrap();
    let mut source = source();
    let events = host(&transport, &mut source, synced);
    assert!(
        events.iter().all(|e| matches!(e, Event::Message(_))),
        "{events:?}"
    );
    assert_eq!(source.row_count(), 6);
}

#[test]
fn the_blocking_wrapper_reads_a_table_to_synced() {
    let fake = fake().start().unwrap();
    let transport = Blocking::new(options(&fake)).unwrap();
    let mut source = source();
    host(&transport, &mut source, synced);
    assert_eq!(source.row_count(), 6);
    assert!(transport.try_recv().is_none(), "nothing more yet");
}

/// The Keys a Grid pumped over `source` shows.
fn shown(grid: &mut Grid, source: &mut DescribedSource) -> Vec<binnacle_core::Key> {
    binnacle_memory::pump(grid, source, |_, _| {});
    let mut keys = Vec::new();
    for (axis, range) in grid.begin_frame(Viewport::new(200.0, 20.0)) {
        let window = source.window(axis, range.clone());
        grid.set_window(axis, range.start, &window.keys);
        if axis == Axis::Rows {
            keys = window.keys;
        }
    }
    keys
}

#[test]
fn an_upsert_after_synced_reaches_the_source_and_the_grid() {
    let fake = fake().start().unwrap();
    let transport = Blocking::new(options(&fake)).unwrap();
    let mut source = source();
    host(&transport, &mut source, synced);
    let mut grid = Grid::new(
        AxisConfig::new(source.count(Axis::Rows), 1.0),
        AxisConfig::new(source.count(Axis::Cols), 10.0),
    );
    source.declare(&mut grid);
    assert_eq!(shown(&mut grid, &mut source).len(), 6);
    let row = [("name".to_owned(), Wire::Text("golf".into()))].into();
    fake.push(Step::Upsert {
        table: "crews".into(),
        row,
    });
    // A host hands the Grid the notices each message gives.
    loop {
        let event = transport.recv_timeout(WAIT).expect("the upsert");
        let Event::Message(message) = event else {
            panic!("{event:?}")
        };
        let upsert = matches!(message, Message::Upsert { .. });
        if let Received::Applied(notices) = source.receive(message) {
            for (axis, notice) in notices {
                grid.notify(axis, notice);
            }
        }
        if upsert {
            break;
        }
    }
    let golf = source.row_key(&[Wire::Text("golf".into())]).unwrap();
    assert_eq!(shown(&mut grid, &mut source).last(), Some(&golf));
}

#[test]
fn a_reconnect_is_a_resync_after_the_backoff() {
    let backoff = Backoff {
        initial: Duration::from_millis(300),
        max: Duration::from_secs(5),
        settle: Duration::from_millis(300),
    };
    let fake = fake().start().unwrap();
    // Seed 3 draws waits of about 151 ms and 156 ms, well clear of the
    // stale backoff's 300 ms least.
    let options = options(&fake).backoff(backoff).seed(Some(3));
    let transport = Blocking::new(options).unwrap();
    let mut source = source();
    host(&transport, &mut source, synced);
    let keys = source.row_keys().to_vec();
    // The stream ends, and bravo goes while it is down.
    fake.push(Step::End);
    fake.push(Step::Delete {
        table: "crews".into(),
        key: vec![Wire::Text("bravo".into())],
    });
    host(&transport, &mut source, |_, e| *e == Event::Ended);
    assert!(!source.status().synced);
    assert_eq!(source.row_count(), 6, "the rows stay while it reconnects");
    host(&transport, &mut source, synced);
    let requests = fake.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target, requests[1].target);
    let gap = requests[1].at - requests[0].at;
    assert!(gap >= backoff.initial / 2, "{gap:?}");
    assert_eq!(source.row_count(), 5);
    let bravo = keys[1];
    let survivors: Vec<_> = keys.iter().copied().filter(|&k| k != bravo).collect();
    assert_eq!(source.row_keys(), survivors, "survivors keep their Keys");
    // The stream stays open `settle` after its first synced, which resets
    // the backoff: the next end waits up to the initial delay again, not
    // double. A resync on the way syncs again, but `settle` runs from the
    // first.
    std::thread::sleep(Duration::from_millis(200));
    fake.push(Step::Resync {
        table: "crews".into(),
    });
    host(&transport, &mut source, |s, e| {
        s.status().synced && matches!(e, Event::Message(Message::Synced { .. }))
    });
    std::thread::sleep(Duration::from_millis(200));
    let before = Instant::now();
    fake.push(Step::End);
    host(&transport, &mut source, |_, e| *e == Event::Ended);
    let ended = Instant::now();
    host(&transport, &mut source, synced);
    let gap = fake.requests()[2].at - ended;
    assert!(gap >= backoff.initial / 2, "{gap:?}");
    // Not doubled: a stale backoff would wait at least 300 ms.
    assert!(gap < backoff.initial, "{gap:?} after {:?}", ended - before);
}

#[test]
fn a_server_that_fails_right_after_synced_is_waited_on_longer_each_time() {
    let backoff = Backoff {
        initial: Duration::from_millis(100),
        max: Duration::from_secs(5),
        settle: Duration::from_secs(10),
    };
    let fake = fake().start().unwrap();
    let transport = Blocking::new(options(&fake).backoff(backoff)).unwrap();
    let mut source = source();
    let mut ends = Vec::new();
    for _ in 0..4 {
        host(&transport, &mut source, synced);
        fake.push(Step::End);
        host(&transport, &mut source, |_, e| *e == Event::Ended);
        ends.push(Instant::now());
    }
    host(&transport, &mut source, synced);
    let requests = fake.requests();
    let gaps: Vec<Duration> = (ends.iter().zip(&requests[1..]))
        .map(|(ended, request)| request.at - *ended)
        .collect();
    // Delays of 100, 200, 400 and 800 ms, each wait in its upper half; a
    // backoff reset by each `synced` would wait at most 100 ms each time.
    let ms = Duration::from_millis;
    for (gap, least) in gaps.iter().zip([ms(50), ms(100), ms(200), ms(400)]) {
        assert!(*gap >= least, "{gaps:?}");
    }
}

#[test]
fn a_new_request_replaces_the_watch_and_drops_its_unread_events() {
    let fake = fake().start().unwrap();
    let transport = Blocking::new(options(&fake)).unwrap();
    let mut source = source();
    host(&transport, &mut source, synced);
    // The open watch carries an upsert the host has not read when it opens
    // another, narrowed to alpha.
    let row = [("name".to_owned(), Wire::Text("golf".into()))].into();
    fake.push(Step::Upsert {
        table: "crews".into(),
        row,
    });
    std::thread::sleep(Duration::from_millis(300));
    let alpha = Scope::new("flotilla").with_where(r#"name == "alpha""#);
    let request = DescribedSource::new("crews", alpha).poll_request().unwrap();
    transport.open(request);
    let mut events = Vec::new();
    while !events
        .iter()
        .any(|e| matches!(e, Event::Message(Message::Synced { .. })))
    {
        events.push(
            transport
                .recv_timeout(WAIT)
                .expect("the new watch's messages"),
        );
    }
    let rows: Vec<&Wire> = (events.iter())
        .filter_map(|e| match e {
            Event::Message(Message::Rows(rows)) => Some(rows.rows.iter().map(|r| &r["name"])),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(rows, [&Wire::Text("alpha".into())], "{events:?}");
    assert!(
        matches!(events[0], Event::Message(Message::Table(_))),
        "nothing of the old watch: {events:?}"
    );
}

#[test]
fn credentials_are_sent_exactly_as_given() {
    let fake = fake().start().unwrap();
    let credentials = [
        "authorization",
        "cookie",
        "proxy-authorization",
        "x-api-key",
    ];
    for (auth, want) in [
        (Auth::None, None),
        (
            Auth::Bearer("t0k".into()),
            Some(("authorization", "Bearer t0k")),
        ),
        (
            Auth::Cookie("session=abc".into()),
            Some(("cookie", "session=abc")),
        ),
    ] {
        let transport = Blocking::new(options(&fake).auth(auth)).unwrap();
        host(&transport, &mut source(), synced);
        let request = fake.requests().pop().unwrap();
        let sent: Vec<(&str, &str)> = (request.headers.iter())
            .filter(|(n, _)| credentials.contains(&n.as_str()))
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect();
        assert_eq!(sent, want.into_iter().collect::<Vec<_>>());
    }
}

#[test]
fn a_404_fails_once_and_a_500_is_retried() {
    let first = fake().start().unwrap();
    let transport = Blocking::new(options(&first)).unwrap();
    let mut missing = DescribedSource::new("nosuchtable", Scope::new("flotilla"));
    let events = host(&transport, &mut missing, |_, e| {
        matches!(e, Event::Failed { .. })
    });
    let Some(Event::Failed { status, body }) = events.last() else {
        panic!("{events:?}")
    };
    assert_eq!(*status, Some(404));
    assert!(String::from_utf8_lossy(body).contains("unknown table"));
    assert_eq!(transport.recv_timeout(Duration::from_millis(700)), None);
    assert_eq!(first.requests().len(), 1, "not retried");

    let backoff = Backoff {
        initial: Duration::from_millis(50),
        max: Duration::from_secs(1),
        ..Backoff::default()
    };
    let fake = fake().failing(500).failing(503).start().unwrap();
    let transport = Blocking::new(options(&fake).backoff(backoff)).unwrap();
    let mut source = source();
    let events = host(&transport, &mut source, synced);
    let retries: Vec<(u32, Duration, Option<u16>)> = (events.iter())
        .filter_map(|e| match e {
            Event::Retrying {
                attempt,
                wait,
                status,
                body,
            } => {
                assert!(String::from_utf8_lossy(body).contains("failed"), "{e:?}");
                Some((*attempt, *wait, *status))
            }
            _ => None,
        })
        .collect();
    let [(1, first, Some(500)), (2, second, Some(503))] = retries[..] else {
        panic!("each retry says why: {retries:?}")
    };
    assert!(
        first >= backoff.initial / 2 && first <= backoff.initial,
        "and how long: {first:?}"
    );
    assert!(
        second >= backoff.initial && second <= backoff.initial * 2,
        "{second:?}"
    );
    assert!(
        (events[2..].iter()).all(|e| matches!(e, Event::Message(_))),
        "{events:?}"
    );
    let requests = fake.requests();
    assert_eq!(requests.len(), 3, "two failures, then the table");
    assert!(requests[1].at - requests[0].at >= first);
    assert!(requests[2].at - requests[1].at >= second, "as said");
}

#[test]
fn a_refused_connection_is_retried() {
    let fake = fake().start().unwrap();
    let url = fake.url().unwrap();
    let binnacle_testkit::fake_tables::Addr::Tcp(addr) = fake.addr().clone() else {
        unreachable!()
    };
    drop(fake);
    let backoff = Backoff {
        initial: Duration::from_millis(20),
        max: Duration::from_millis(40),
        ..Backoff::default()
    };
    let transport = Blocking::new(Options::new(Endpoint::Tcp(url)).backoff(backoff)).unwrap();
    let mut source = source();
    transport.open(source.poll_request().unwrap());
    let Some(Event::Retrying {
        attempt: 1,
        status: None,
        body,
        ..
    }) = transport.recv_timeout(Duration::from_millis(300))
    else {
        panic!("a retry, with no status")
    };
    assert!(String::from_utf8_lossy(&body).contains("connecting to"));
    // A server on that port at last: the transport, still trying, reads it.
    let fake = FakeTables::new()
        .fixture(crews())
        .unwrap()
        .tcp(addr)
        .start()
        .unwrap();
    let events = host(&transport, &mut source, synced);
    assert!(events.iter().any(|e| matches!(e, Event::Message(_))));
    assert_eq!(source.row_count(), 6);
    assert_eq!(fake.requests().len(), 1);
}

/// The waits of a transport seeded with `seed` retrying a server that
/// fails `failures` times, and the gaps between its requests.
fn retried(backoff: Backoff, seed: u64, failures: usize) -> (Vec<Duration>, Vec<Duration>) {
    let fake = (0..failures)
        .fold(fake(), |f, _| f.failing(503))
        .start()
        .unwrap();
    let options = options(&fake).backoff(backoff).seed(Some(seed));
    let transport = Blocking::new(options).unwrap();
    let events = host(&transport, &mut source(), synced);
    let waits = (events.iter())
        .filter_map(|e| match e {
            Event::Retrying { wait, .. } => Some(*wait),
            _ => None,
        })
        .collect();
    let requests = fake.requests();
    let gaps = (requests.windows(2))
        .map(|pair| pair[1].at - pair[0].at)
        .collect();
    (waits, gaps)
}

#[test]
fn the_backoff_doubles_up_to_its_most_each_wait_in_its_upper_half() {
    let ms = Duration::from_millis;
    let backoff = Backoff {
        initial: ms(10),
        max: ms(30),
        ..Backoff::default()
    };
    let (waits, gaps) = retried(backoff, 7, 6);
    let delays = [ms(10), ms(20), ms(30), ms(30), ms(30), ms(30)];
    assert_eq!(waits.len(), delays.len());
    for ((wait, delay), gap) in waits.iter().zip(delays).zip(&gaps) {
        assert!(*wait >= delay / 2 && *wait <= delay, "{waits:?}");
        assert!(gap >= wait, "each request waits as said: {gaps:?}");
    }
    // Jittered: of the four waits at the most, not all are the same.
    assert!(waits[2..].iter().any(|w| *w != waits[2]), "{waits:?}");
    // The same seed draws the same waits; another draws others.
    assert_eq!(retried(backoff, 7, 6).0, waits);
    assert_ne!(retried(backoff, 8, 6).0, waits);
}

#[test]
fn a_retry_limit_turns_a_server_down_or_failing_into_failed() {
    // A port nothing listens on.
    let url = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let backoff = Backoff {
        initial: Duration::from_millis(5),
        max: Duration::from_millis(10),
        ..Backoff::default()
    };
    let down = Options::new(Endpoint::Tcp(url))
        .backoff(backoff)
        .retries(Some(2));
    let transport = Blocking::new(down).unwrap();
    let mut source = source();
    let events = host(&transport, &mut source, |_, e| {
        matches!(e, Event::Failed { .. })
    });
    assert!(
        matches!(
            &events[..],
            [
                Event::Retrying { attempt: 1, .. },
                Event::Retrying { attempt: 2, .. },
                Event::Failed { status: None, .. }
            ]
        ),
        "{events:?}"
    );

    let fake = (0..3).fold(fake(), |f, _| f.failing(503)).start().unwrap();
    let failing = options(&fake).backoff(backoff).retries(Some(1));
    let transport = Blocking::new(failing).unwrap();
    let events = host(&transport, &mut self::source(), |_, e| {
        matches!(e, Event::Failed { .. })
    });
    let Some(Event::Failed { status, body }) = events.last() else {
        panic!("{events:?}")
    };
    assert_eq!(*status, Some(503));
    assert!(String::from_utf8_lossy(body).contains("failed"));
    assert_eq!(fake.requests().len(), 2, "one try, one retry");
}

#[cfg(unix)]
#[test]
fn a_table_reads_over_a_unix_socket() {
    let dir = TempDir::new("described-http");
    let path = dir.0.join("tables.sock");
    let fake = fake().unix(&path).start().unwrap();
    let transport = Blocking::new(Options::new(Endpoint::Unix(path.clone()))).unwrap();
    let mut source = source();
    host(&transport, &mut source, synced);
    assert_eq!(source.row_count(), 6);
    assert_eq!(fake.requests()[0].header("host"), Some("localhost"));
}

/// A directory of its own, removed when dropped, assertions or not.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn no_other_crate_reaches_tokio() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    // On every target, so a target-specific dependency (wasm's) counts.
    let tree = |package: &str| {
        let output = std::process::Command::new(&cargo)
            .args(["tree", "-e", "normal", "-i", "tokio", "--target", "all"])
            .args(["-p", package])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        (
            output.status.success(),
            text(&output.stdout),
            text(&output.stderr),
        )
    };
    let (ok, stdout, _) = tree("binnacle-described-http");
    assert!(ok && stdout.contains("tokio v"), "the check can see it");
    for package in [
        "binnacle-core",
        "binnacle-memory",
        "binnacle-wasm",
        "binnacle-ffi",
        "binnacle-described",
    ] {
        // Only cargo's two ways of saying no path passes: a tree is tokio
        // reached (or `package` gone, which prints the whole workspace's),
        // and any other error is the check not running.
        let (ok, stdout, stderr) = tree(package);
        let none = match ok {
            false => stderr.contains("`tokio` did not match any packages"),
            true => stderr.contains("nothing to print"),
        };
        assert!(
            none && stdout.is_empty(),
            "{package} reaches tokio, or the check failed:\n{stdout}{stderr}"
        );
    }
}

#[test]
fn a_server_that_stalls_is_given_up_on_and_tried_again() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = std::sync::Arc::clone(&accepted);
    std::thread::spawn(move || {
        // Takes each connection and never answers.
        let mut held = Vec::new();
        for stream in listener.incoming() {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            held.push(stream);
        }
    });
    let backoff = Backoff {
        initial: Duration::from_millis(20),
        max: Duration::from_millis(40),
        ..Backoff::default()
    };
    let options = Options::new(Endpoint::Tcp(url))
        .backoff(backoff)
        .timeout(Duration::from_millis(100));
    let transport = Blocking::new(options).unwrap();
    transport.open(source().poll_request().unwrap());
    let Some(Event::Retrying {
        status: None, body, ..
    }) = transport.recv_timeout(Duration::from_millis(600))
    else {
        panic!("a retry, with no status")
    };
    assert!(String::from_utf8_lossy(&body).contains("no answer within"));
    let again = transport.recv_timeout(Duration::from_millis(600));
    assert!(matches!(again, Some(Event::Retrying { attempt: 2, .. })));
    // The handshake completes before `accept()` returns, so the retry can
    // be reported before the accept thread counts its connection.
    let deadline = std::time::Instant::now() + WAIT;
    while accepted.load(std::sync::atomic::Ordering::SeqCst) < 2 {
        assert!(std::time::Instant::now() < deadline, "tried again");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_credential_never_shows_in_debug_output() {
    let options = Options::new(Endpoint::Tcp("http://x".into())).auth(Auth::Bearer("t0k".into()));
    assert!(!format!("{options:?}").contains("t0k"));
    assert!(!format!("{:?}", Auth::Cookie("s=1".into())).contains("s=1"));
}

/// The events a watch brings to synced and for `within` after, its
/// source fed as a host feeds it.
fn watched(options: Options, within: Duration) -> Vec<Event> {
    let transport = Blocking::new(options).unwrap();
    let mut source = source();
    let mut events = host(&transport, &mut source, synced);
    let until = Instant::now() + within;
    loop {
        if let Some(request) = source.poll_request() {
            transport.open(request);
        }
        let left = until.saturating_duration_since(Instant::now());
        let Some(event) = transport.recv_timeout(left) else {
            return events;
        };
        match &event {
            Event::Message(message) => {
                source.receive(message.clone());
            }
            Event::Ended => {
                source.ended();
            }
            _ => {}
        }
        events.push(event);
    }
}

#[test]
fn a_silent_stream_ends_after_the_idle_timeout_and_opens_again() {
    let quiet = fake().heartbeat(Duration::from_secs(3600)).start().unwrap();
    let idle = Duration::from_millis(200);
    let backoff = Backoff {
        initial: Duration::from_millis(20),
        ..Backoff::default()
    };
    let idling = options(&quiet).backoff(backoff).idle(Some(idle));
    let events = watched(idling, Duration::from_millis(500));
    assert!(events.contains(&Event::Ended), "{events:?}");
    let requests = quiet.requests();
    assert!(requests.len() >= 2, "opened again");
    assert!(requests[1].at - requests[0].at >= idle);

    // Without it, the silent stream stays open.
    let quiet = fake().heartbeat(Duration::from_secs(3600)).start().unwrap();
    let events = watched(options(&quiet), Duration::from_millis(500));
    assert!(!events.contains(&Event::Ended), "{events:?}");
    assert_eq!(quiet.requests().len(), 1);
}

#[test]
fn heartbeats_keep_a_stream_open_past_the_idle_timeout() {
    let beating = fake().heartbeat(Duration::from_millis(50)).start().unwrap();
    let options = options(&beating).idle(Some(Duration::from_millis(600)));
    let events = watched(options, Duration::from_millis(1200));
    assert!(!events.contains(&Event::Ended), "{events:?}");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Message(Message::Progress { .. }))),
        "{events:?}"
    );
    assert_eq!(beating.requests().len(), 1);
}

#[test]
fn a_bad_endpoint_or_credential_is_unserved_and_nothing_is_sent() {
    let fake = fake().start().unwrap();
    let unserved = |options: Options| {
        let transport = Blocking::new(options).unwrap();
        transport.open(source().poll_request().unwrap());
        let event = transport.recv_timeout(WAIT);
        assert_eq!(transport.recv_timeout(Duration::from_millis(100)), None);
        event
    };
    let base = fake.url().unwrap();
    let with_user = base.replace("http://", "http://user@");
    assert_eq!(
        unserved(Options::new(Endpoint::Tcp(with_user))),
        Some(Event::Unserved(Unserved::Endpoint))
    );
    let bad_prefix = format!("{base}/a\u{1}b");
    assert_eq!(
        unserved(Options::new(Endpoint::Tcp(bad_prefix))),
        Some(Event::Unserved(Unserved::Endpoint))
    );
    let bad_token = options(&fake).auth(Auth::Bearer("a\nb".into()));
    assert_eq!(unserved(bad_token), Some(Event::Unserved(Unserved::Auth)));
    assert!(fake.requests().is_empty());
}

#[test]
fn settling_leaves_a_stream_with_an_idle_timeout_open() {
    let beating = fake().heartbeat(Duration::from_millis(50)).start().unwrap();
    let backoff = Backoff {
        settle: Duration::from_millis(100),
        ..Backoff::default()
    };
    let options = options(&beating)
        .backoff(backoff)
        .idle(Some(Duration::from_millis(600)));
    let events = watched(options, Duration::from_millis(1200));
    assert!(!events.contains(&Event::Ended), "{events:?}");
    assert_eq!(beating.requests().len(), 1);
}

#[test]
fn a_message_coming_slowly_a_byte_at_a_time_is_not_idle() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let line = b"{\"type\": \"progress\", \"sequence\": 7}\n";
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut header = String::new();
        while reader.read_line(&mut header).unwrap() > 2 {
            header.clear();
        }
        stream.set_nodelay(true).unwrap();
        let head =
            "HTTP/1.1 200 OK\r\ncontent-type: application/x-ndjson\r\nconnection: close\r\n\r\n";
        stream.write_all(head.as_bytes()).unwrap();
        // Six pieces 150 ms apart: the message takes longer than `idle`,
        // each piece well within it.
        for piece in line.chunks(line.len().div_ceil(6)) {
            stream.write_all(piece).unwrap();
            std::thread::sleep(Duration::from_millis(150));
        }
    });
    let options = Options::new(Endpoint::Tcp(url)).idle(Some(Duration::from_millis(500)));
    let transport = Blocking::new(options).unwrap();
    transport.open(source().poll_request().unwrap());
    let progress = Event::Message(Message::Progress { sequence: Some(7) });
    assert_eq!(transport.recv_timeout(WAIT), Some(progress));
    assert_eq!(transport.recv_timeout(WAIT), Some(Event::Ended));
}

#[test]
fn a_stream_gone_idle_inside_a_message_ends_without_an_undecodable() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut header = String::new();
        while reader.read_line(&mut header).unwrap() > 2 {
            header.clear();
        }
        let head =
            "HTTP/1.1 200 OK\r\ncontent-type: application/x-ndjson\r\nconnection: close\r\n\r\n";
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(b"{\"type\": \"pro").unwrap();
        // Holds the connection open, silent, well past `idle`.
        std::thread::sleep(Duration::from_secs(5));
    });
    let options = Options::new(Endpoint::Tcp(url)).idle(Some(Duration::from_millis(150)));
    let transport = Blocking::new(options).unwrap();
    transport.open(source().poll_request().unwrap());
    assert_eq!(transport.recv_timeout(WAIT), Some(Event::Ended));
}
