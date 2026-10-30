//! The binnacle tables example: a Described Table as a command-line tool.
//! Its rows come from a recorded rows stream, or live from a table server.
//! Piped, it prints the table (`--format aligned|tsv|json|jsonl`); in a
//! terminal it opens the table inline below the prompt, live, with the
//! full widget (sort, the header menu with grouping on `m`, the chooser,
//! filters on Ctrl+L and Alt+Down, find on Ctrl+F), and on Enter prints the
//! chosen rows' primary keys, Space picking rows. The nested columns start
//! hidden and the key and state columns pinned leading.
//!
//! ```text
//! binnacle-tables <fixture.ndjson|.cborseq> [flags]
//! binnacle-tables (--url <base> | --socket <path>) --table <name>
//!     [--namespace <ns>] [--scope-where <clause>]... [--token <t> | --cookie <c>] [flags]
//! flags: --columns a,b  --sort [-]field  --where <cond>  --view default  --wide
//!        --format <format>  --interactive | --no-interactive
//! ```
//!
//! `--where` filters here (`phase==Running`, `cpus>=4`, `name~al`);
//! `--scope-where` passes a clause to the server, in its own syntax.
//!
//! Run it with
//! `cargo run -p binnacle-described --example binnacle-tables -- crates/binnacle-described/tests/data/rows-hosts.ndjson`,
//! or against `fake-tables` with `-- --url http://127.0.0.1:7777 --table hosts`.

// Shared with the render tests, which use the parts this CLI does not.
#[allow(dead_code)]
mod table;

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use binnacle_core::{Axis, ChangeNotice};
use binnacle_described::{DescribedSource, Received, Scope};
use binnacle_described_http::{Auth, Blocking, Endpoint, Event as Wire, Options};
use binnacle_format::Timestamp;
use binnacle_layout::IdMap;
use binnacle_ratatui::cli::{Flags, InlineOptions, Live, print, run_inline};

use table::Tables;

/// Where the rows come from.
enum Rows {
    Fixture(PathBuf),
    Live {
        options: Options,
        table: String,
        scope: Scope,
    },
}

fn fail(message: &str) -> ! {
    eprintln!("binnacle-tables: {message}");
    std::process::exit(2);
}

fn rows(args: Vec<String>) -> Rows {
    let mut args = args.into_iter();
    let (mut endpoint, mut table, mut namespace) = (None, None, "flotilla".to_owned());
    let (mut wheres, mut auth) = (Vec::new(), Auth::None);
    let mut fixture = None;
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .unwrap_or_else(|| fail(&format!("{arg} needs a value")))
        };
        match arg.as_str() {
            "--url" => endpoint = Some(Endpoint::Tcp(value())),
            "--socket" => endpoint = Some(Endpoint::Unix(value().into())),
            "--table" => table = Some(value()),
            "--namespace" => namespace = value(),
            "--scope-where" => wheres.push(value()),
            "--token" => auth = Auth::Bearer(value()),
            "--cookie" => auth = Auth::Cookie(value()),
            flag if flag.starts_with("--") => fail(&format!("unknown flag {flag}")),
            path => fixture = Some(PathBuf::from(path)),
        }
    }
    match (fixture, endpoint, table) {
        (Some(path), None, None) => Rows::Fixture(path),
        (None, Some(endpoint), Some(table)) => {
            let scope = (wheres.into_iter()).fold(Scope::new(namespace), Scope::with_where);
            Rows::Live {
                options: Options::new(endpoint).auth(auth),
                table,
                scope,
            }
        }
        (None, Some(_), None) => fail("--table is needed"),
        _ => fail("a fixture, or --url or --socket with --table"),
    }
}

/// Feeds `source` every event waiting on `transport`, opening the watch
/// again after an end; returns the Change Notices.
fn drain(source: &mut DescribedSource, transport: &Blocking) -> Vec<(Axis, ChangeNotice)> {
    let mut notices = Vec::new();
    while let Some(event) = transport.try_recv() {
        match event {
            Wire::Message(message) => {
                if let Received::Applied(n) = source.receive(message) {
                    notices.extend(n);
                }
            }
            Wire::Ended => {
                source.ended();
            }
            Wire::Failed { status, body } => {
                fail(&format!("{status} {}", String::from_utf8_lossy(&body)));
            }
            _ => {}
        }
        if let Some(request) = source.poll_request() {
            transport.open(request);
        }
    }
    notices
}

fn main() -> io::Result<()> {
    let (flags, rest) =
        Flags::parse_known(std::env::args().skip(1)).unwrap_or_else(|e| fail(&e.to_string()));
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
    let origin = Timestamp::from_unix_nanos(now.map_or(0, |d| d.as_nanos() as i128));
    let (mut tables, transport) = match rows(rest) {
        Rows::Fixture(path) => {
            let tables = Tables::load(&path, origin).unwrap_or_else(|e| fail(&e));
            (tables, None)
        }
        Rows::Live {
            options,
            table,
            scope,
        } => {
            let transport = Blocking::new(options)?;
            let mut tables = Tables::live(&table, scope, origin);
            let deadline = Instant::now() + Duration::from_secs(30);
            while !tables.source.status().synced {
                if let Some(request) = tables.source.poll_request() {
                    transport.open(request);
                }
                drain(&mut tables.source, &transport);
                if Instant::now() > deadline {
                    fail("no table within 30 s");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            (tables, Some(transport))
        }
    };
    let base = tables.default_layout();
    let fields: Vec<String> = (tables.source.columns().iter())
        .map(|c| c.name().to_owned())
        .collect();
    let views = |name: &str| (name == "default").then(|| base.clone());
    let layout = flags
        .apply_with(&base, &fields, views)
        .unwrap_or_else(|e| fail(&e.to_string()));
    if !flags.interactive.now() {
        let mut out = io::stdout().lock();
        let format = flags.format.unwrap_or_default();
        return match print(&mut tables.source, &layout, format, &mut out).and_then(|()| out.flush())
        {
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            other => other,
        };
    }
    let live = transport.map(|transport| -> Live<'_, DescribedSource> {
        Box::new(move |source: &mut DescribedSource| drain(source, &transport))
    });
    let options = InlineOptions {
        live,
        origin: Some(origin),
        ..InlineOptions::default()
    };
    let chosen = run_inline(&mut tables.source, &layout, options)?;
    if let Some(selection) = chosen {
        for row in selection.rows {
            if let Some(id) = IdMap::row_id(&tables.source, row) {
                println!("{id}");
            }
        }
    }
    Ok(())
}
