//! `fake-tables`: serves fake described tables until stopped.
//!
//! ```text
//! fake-tables [--fixture <path>]... [--generate <name>:<rows>]...
//!             [--script <path>] [--listen <addr>] [--socket <path>]
//!             [--page <rows>] [--heartbeat <ms>]
//! ```

use std::time::Duration;

use binnacle_testkit::fake_tables::{FakeTables, parse_script};

fn main() {
    if let Err(message) = run() {
        eprintln!("fake-tables: {message}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut fake = FakeTables::new().tcp(([127, 0, 0, 1], 7777).into());
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        fake = match flag.as_str() {
            "--fixture" => fake.fixture(value()?)?,
            "--generate" => {
                let spec = value()?;
                let (name, rows) = (spec.split_once(':'))
                    .and_then(|(n, r)| Some((n.to_owned(), r.parse().ok()?)))
                    .ok_or_else(|| format!("--generate wants <name>:<rows>, not {spec}"))?;
                fake.generate(name, rows)
            }
            "--script" => {
                let path = value()?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                fake.script(parse_script(&text)?)
            }
            "--listen" => {
                let addr = value()?;
                fake.tcp(addr.parse().map_err(|e| format!("{addr}: {e}"))?)
            }
            #[cfg(unix)]
            "--socket" => fake.unix(value()?),
            "--page" => fake.page(value()?.parse().map_err(|e| format!("--page: {e}"))?),
            "--heartbeat" => {
                let ms = value()?.parse().map_err(|e| format!("--heartbeat: {e}"))?;
                fake.heartbeat(Duration::from_millis(ms))
            }
            "-h" | "--help" => {
                println!(
                    "fake-tables [--fixture <path>]... [--generate <name>:<rows>]... [--script <path>] [--listen <addr>] [--socket <path>] [--page <rows>] [--heartbeat <ms>]"
                );
                return Ok(());
            }
            other => return Err(format!("unknown flag {other}")),
        };
    }
    let running = fake.start().map_err(|e| e.to_string())?;
    println!("fake-tables listening on {}", running.addr());
    loop {
        std::thread::park();
    }
}
