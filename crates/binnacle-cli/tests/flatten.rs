//! A host's own `clap` parser flattening `FlagArgs` and taking `--format`
//! prints what `binnacle-tables` prints for the same flags.

use binnacle_cli::{FlagArgs, Format, print};
use binnacle_described::{Decoder, DescribedSource, Encoding, HostRequest, Scope};
use clap::Parser;

#[derive(Parser)]
struct Host {
    #[command(flatten)]
    table: FlagArgs,
    #[arg(long, value_enum, default_value = "aligned")]
    format: Format,
}

fn crews() -> DescribedSource {
    let bytes = include_bytes!("../../binnacle-described/tests/fixtures/crews.ndjson");
    let mut decoder = Decoder::new(Encoding::Ndjson);
    decoder.feed(bytes);
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    assert!(matches!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
    while let Some(message) = decoder.next() {
        source.receive(message.unwrap());
    }
    source
}

fn run(args: &[&str]) -> Result<String, String> {
    let host = Host::try_parse_from(["host"].into_iter().chain(args.iter().copied()))
        .map_err(|e| e.to_string())?;
    let flags = host.table.flags().map_err(|e| e.to_string())?;
    let mut source = crews();
    let base = source.presentation(None, None);
    let layout = flags
        .apply_to(&mut source, &base, |_| None)
        .map_err(|e| e.to_string())?;
    let (mut out, mut note) = (Vec::new(), Vec::new());
    print(&mut source, &layout, host.format, &mut out, &mut note, None).unwrap();
    Ok(String::from_utf8(out).unwrap())
}

#[test]
fn a_flattened_host_prints_what_binnacle_tables_prints() {
    // `binnacle-tables crews.ndjson` with the same flags prints this.
    let text = run(&[
        "--columns",
        "name,phase",
        "--sort",
        "-name",
        "--where",
        "phase==Running",
        "--format",
        "tsv",
    ])
    .unwrap();
    assert_eq!(
        text,
        "name\tphase\nfoxtrot\tRunning\ndelta\tRunning\nalpha\tRunning\n"
    );
}

#[test]
fn a_bad_where_is_refused_when_the_args_convert() {
    for clause in ["phase", "phase~", "=x"] {
        let err = run(&["--where", clause]).unwrap_err();
        assert!(err.contains("--where"), "{clause}: {err}");
    }
}

#[test]
fn the_conversion_itself_checks_where() {
    let host = Host::try_parse_from(["host", "--where", "phase"]).unwrap();
    assert!(host.table.flags().is_err());
}

#[test]
fn a_bad_columns_list_is_refused_when_the_args_convert() {
    let err = run(&["--columns", "name,name"]).unwrap_err();
    assert!(err.contains("twice"), "{err}");
}

#[test]
fn the_hosts_format_is_a_value_enum() {
    assert!(run(&["--format", "yaml"]).is_err());
    assert!(run(&["--format", "jsonl"]).unwrap().starts_with('{'));
}
