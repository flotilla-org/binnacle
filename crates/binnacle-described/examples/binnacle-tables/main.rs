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
//! `--where` filters here (`phase==Running`, `cpus>=4`, `name~al`); the
//! first operator ends the field, so a field whose name holds one can't
//! be named;
//! `--scope-where` passes a clause to the server, in its own syntax.
//! `--token` shows in the process list (`ps`); `BINNACLE_TOKEN` in the
//! environment is the bearer token without it, when neither `--token` nor
//! `--cookie` is given.
//! `--interactive` opens the table even when stdout is not a terminal,
//! writing the viewport's escape sequences there.
//!
//! A table over the row cap prints the rows that came and then says so on
//! stderr, still exiting 0, so stdout holds only the table; the viewport's
//! status line says "10,000 of more rows" (or, under a filter,
//! "90 shown of 10,000 read, more on the server"). A live message that
//! can't be decoded shows on the status line, or on stderr when piped. Ctrl-C, or SIGINT, SIGTERM or
//! SIGHUP, leaves the viewport choosing nothing and exits 128 and the
//! signal's number (130 for Ctrl-C).
//!
//! Run it with
//! `cargo run -p binnacle-described --example binnacle-tables -- crates/binnacle-described/tests/data/rows-hosts.ndjson`,
//! or against `fake-tables` with `-- --url http://127.0.0.1:7777 --table hosts`.

// Shared with the render tests, which use the parts this CLI does not.
#[allow(dead_code)]
mod table;

mod feed;

use std::cell::RefCell;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use binnacle_described::{DescribedSource, Scope};
use binnacle_described_http::{Auth, Blocking, Endpoint, Options};
use binnacle_format::Timestamp;
use binnacle_layout::IdMap;
use binnacle_ratatui::cli::{Exit, Flags, InlineOptions, Live, print, run_inline};

use feed::{Notes, drain};
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

const USAGE: &str = "usage: binnacle-tables <fixture.ndjson|fixture.cborseq> [flags]
       binnacle-tables (--url <base> | --socket <path>) --table <name>
           [--namespace <ns>] [--scope-where <clause>]... [--token <t> | --cookie <c>] [flags]
       (--token shows in ps; BINNACLE_TOKEN in the environment does not)
flags: --columns a,b  --sort [-]field  --where <cond>  --view default  --wide
       --format aligned|tsv|json|jsonl  --interactive | --no-interactive";

fn fail(message: &str) -> ! {
    eprintln!("binnacle-tables: {message}");
    std::process::exit(2);
}

/// `fail` for a malformed command line, with the usage.
fn usage(message: &str) -> ! {
    fail(&format!("{message}\n{USAGE}"))
}

fn rows(args: Vec<String>) -> Rows {
    let mut args = args.into_iter();
    let (mut endpoint, mut table, mut namespace) = (None, None, "flotilla".to_owned());
    let (mut wheres, mut auth) = (Vec::new(), Auth::None);
    let mut fixture = None;
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .unwrap_or_else(|| usage(&format!("{arg} needs a value")))
        };
        match arg.as_str() {
            "--url" => endpoint = Some(Endpoint::Tcp(value())),
            "--socket" => endpoint = Some(Endpoint::Unix(value().into())),
            "--table" => table = Some(value()),
            "--namespace" => namespace = value(),
            "--scope-where" => wheres.push(value()),
            "--token" => auth = Auth::Bearer(value()),
            "--cookie" => auth = Auth::Cookie(value()),
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            flag if flag.starts_with("--") => usage(&format!("unknown flag {flag}")),
            path => fixture = Some(PathBuf::from(path)),
        }
    }
    if namespace.is_empty() {
        usage("--namespace can't be empty");
    }
    if auth == Auth::None
        && let Some(token) = std::env::var("BINNACLE_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
    {
        auth = Auth::Bearer(token);
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
        (None, Some(_), None) => usage("--table is needed"),
        _ => usage("a fixture, or --url or --socket with --table"),
    }
}

/// The longest [`drain`] takes events at once, so a burst of changes still
/// lets frames draw and keys work between its parts; while changes keep
/// coming, the viewport doesn't wait for input between frames.
const BUDGET: Duration = Duration::from_millis(30);

/// What a partial table's note adds: `--where` only filters the rows
/// already read, so the way to more of them is the server's own read.
const PARTIAL_HINT: &str = "narrow the server's read with --scope-where";

/// How many failed attempts in a row a print (not a terminal) retries
/// before it gives up: about 4 s with the default backoff.
const PRINT_RETRIES: u32 = 4;

fn main() -> io::Result<()> {
    let (flags, rest) =
        Flags::parse_known(std::env::args().skip(1)).unwrap_or_else(|e| usage(&e.to_string()));
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
    let origin = Timestamp::from_unix_nanos(now.map_or(0, |d| d.as_nanos() as i128));
    let interactive = flags.interactive.now();
    let notes = RefCell::new(Notes::default());
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
            // A print fails on a server down or a wrong port; a terminal
            // keeps trying, saying so.
            let options = match interactive {
                true => options,
                false => options.retries(Some(PRINT_RETRIES)),
            };
            let transport = Blocking::new(options)?;
            let mut tables = Tables::live(&table, scope, origin);
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut said = None;
            while !tables.source.status().synced {
                if let Some(request) = tables.source.poll_request() {
                    transport.open(request);
                }
                let fed = drain(
                    &mut tables.source,
                    || transport.try_recv(),
                    |request| transport.open(request),
                    &mut notes.borrow_mut(),
                    BUDGET,
                );
                fed.unwrap_or_else(|e| fail(&e));
                let now = notes.borrow().reconnecting.clone();
                if now.is_some() && now != said {
                    eprintln!("binnacle-tables: {}", now.as_deref().unwrap_or_default());
                }
                said = now;
                if Instant::now() > deadline {
                    fail("no table within 30 s");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            (tables, Some(transport))
        }
    };
    // No flotilla default view or saved user Layout here: the generic one.
    let base = tables.source.presentation(None, None);
    let views = |name: &str| (name == "default").then(|| base.clone());
    let layout = flags
        .apply_to(&mut tables.source, &base, views)
        .unwrap_or_else(|e| fail(&e.to_string()));
    if !interactive {
        let mut out = io::stdout().lock();
        let format = flags.format.unwrap_or_default();
        let mut note = Vec::new();
        let printed = print(
            &mut tables.source,
            &layout,
            format,
            &mut out,
            &mut note,
            Some(PARTIAL_HINT),
        )
        .and_then(|()| out.flush());
        for line in String::from_utf8_lossy(&note).lines() {
            eprintln!("binnacle-tables: {line}");
        }
        if let Some(line) = notes.borrow().corrupted() {
            eprintln!("binnacle-tables: {line}");
        }
        return match printed {
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            other => other,
        };
    }
    // A failure mid-session ends it; it is reported once the terminal is
    // restored, as exiting in raw mode would leave the shell in it.
    let failed = RefCell::new(None);
    let failed_ref = &failed;
    let notes = &notes;
    let live = transport.map(|transport| -> Live<'_, DescribedSource> {
        Box::new(move |source: &mut DescribedSource| {
            drain(
                source,
                || transport.try_recv(),
                |request| transport.open(request),
                &mut notes.borrow_mut(),
                BUDGET,
            )
            .map_err(|e| *failed_ref.borrow_mut() = Some(e))
            .ok()
        })
    });
    let options = InlineOptions {
        live,
        note: Some(Box::new(|| notes.borrow().line())),
        origin: Some(origin),
        ..InlineOptions::default()
    };
    let exit = run_inline(&mut tables.source, &layout, options)?;
    if let Some(e) = failed.take() {
        fail(&e);
    }
    if let Some(status) = exit.status() {
        std::process::exit(status);
    }
    if let Exit::Chosen(selection) = exit {
        for row in selection.rows {
            if let Some(id) = IdMap::row_id(&tables.source, row) {
                println!("{id}");
            }
        }
    }
    Ok(())
}
