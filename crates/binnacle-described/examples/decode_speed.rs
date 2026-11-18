//! How long a rows stream of a 10,000-row table takes to decode in each
//! encoding, fed whole and in 16 KiB chunks. Run with
//! `cargo run --release -p binnacle-described --example decode_speed`.

use std::time::{Duration, Instant};

use binnacle_described::{
    Decoder, Encoding, Message, RowsMessage, TableMessage, encode_cbor_item, encode_json_line,
};
use binnacle_testkit::fake_tables::FakeTable;

const ROWS: usize = 10_000;
const PAGE: usize = 1_000;
const RUNS: u32 = 20;

fn main() {
    let table = FakeTable::generated("speed", ROWS);
    let mut messages = vec![Message::Table(TableMessage {
        schema: table.schema.clone(),
        incarnation: table.incarnation.clone(),
        table: Some(table.name.clone()),
        sequence: Some(1),
    })];
    for (i, page) in table.rows.chunks(PAGE).enumerate() {
        messages.push(Message::Rows(RowsMessage {
            rows: page.to_vec(),
            cursor: (i + 1 < ROWS / PAGE).then(|| format!("{}", i + 1)),
            partial: false,
            sequence: Some(1),
        }));
    }
    messages.push(Message::Synced { sequence: Some(1) });
    for encoding in [Encoding::Ndjson, Encoding::CborSeq] {
        let bytes: Vec<u8> = (messages.iter())
            .flat_map(|m| match encoding {
                Encoding::Ndjson => encode_json_line(m),
                Encoding::CborSeq => encode_cbor_item(m),
            })
            .collect();
        for chunk in [bytes.len(), 16 << 10] {
            let mut best = Duration::MAX;
            for _ in 0..RUNS {
                let started = Instant::now();
                let mut decoder = Decoder::new(encoding);
                let mut decoded = 0;
                for piece in bytes.chunks(chunk) {
                    decoder.feed(piece);
                    while let Some(message) = decoder.next() {
                        message.unwrap();
                        decoded += 1;
                    }
                }
                assert!(decoder.finish().is_none());
                assert_eq!(decoded, messages.len());
                best = best.min(started.elapsed());
            }
            println!(
                "{:<20} {:>9} bytes in {:>9} byte chunks: {:>7.2} ms",
                encoding.content_type(),
                bytes.len(),
                chunk.min(bytes.len()),
                best.as_secs_f64() * 1e3
            );
        }
    }
}
