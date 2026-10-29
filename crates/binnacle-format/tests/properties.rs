//! Formatting never panics, whatever the value, unit, role, options, clock
//! and zone, and its text is always one line. Stable text is the text with
//! dates absolute.

use binnacle_format::{
    FixedOffset, FormatContext, FormatOptions, Role, TimeStyle, Timestamp, Value, ValueType,
    format, format_stable,
};
use hegel::TestCase;
use hegel::generators as gs;
use serde_json::{Map, Value as Json};

/// Any text, or text built from the characters most likely to break a line.
fn text(tc: &TestCase) -> String {
    if tc.draw(gs::booleans()) {
        return tc.draw(gs::text().max_size(40));
    }
    let hostile = vec![
        'a', ' ', '\n', '\r', '\t', '\u{7}', '\u{85}', '\u{2028}', 'é', '"', '…',
    ];
    let chars = tc.draw(gs::vecs(gs::sampled_from(hostile)).max_size(20));
    chars.into_iter().collect()
}

fn nested(tc: &TestCase, depth: u32) -> Json {
    let kinds = if depth == 0 { 4 } else { 6 };
    match tc.draw(gs::integers::<u8>().min_value(0).max_value(kinds - 1)) {
        0 => Json::Null,
        1 => Json::Bool(tc.draw(gs::booleans())),
        2 => serde_json::Number::from_f64(tc.draw(gs::floats::<f64>()))
            .map_or(Json::Null, Json::Number),
        3 => Json::String(text(tc)),
        4 => {
            let n = tc.draw(gs::integers::<usize>().min_value(0).max_value(4));
            Json::Array((0..n).map(|_| nested(tc, depth - 1)).collect())
        }
        _ => {
            let n = tc.draw(gs::integers::<usize>().min_value(0).max_value(4));
            let map: Map<String, Json> =
                (0..n).map(|_| (text(tc), nested(tc, depth - 1))).collect();
            Json::Object(map)
        }
    }
}

fn timestamp(tc: &TestCase) -> Timestamp {
    Timestamp::from_unix_nanos(tc.draw(gs::integers::<i128>()))
}

fn value(tc: &TestCase) -> Value {
    let kind = tc.draw(gs::sampled_from(ValueType::ALL.to_vec()));
    match kind {
        ValueType::Null => Value::Null,
        ValueType::Bool => Value::Bool(tc.draw(gs::booleans())),
        ValueType::Int => Value::Int(tc.draw(gs::integers::<i64>())),
        ValueType::UInt => Value::UInt(tc.draw(gs::integers::<u64>())),
        ValueType::Number => Value::Number(tc.draw(gs::floats::<f64>())),
        ValueType::Text => Value::Text(text(tc)),
        ValueType::Bytes => Value::Bytes(tc.draw(gs::binary().max_size(64))),
        ValueType::DateTime => Value::DateTime(timestamp(tc)),
        ValueType::Category => Value::Category {
            value: text(tc),
            label: tc.draw(gs::booleans()).then(|| text(tc)),
        },
        ValueType::Nested => Value::Nested(nested(tc, 3)),
        ValueType::Invalid => Value::Invalid {
            raw: text(tc),
            expected: tc.draw(gs::sampled_from(ValueType::ALL.to_vec())),
        },
    }
}

#[hegel::test(test_cases = 1000)]
fn formatting_never_panics_and_is_always_one_line(tc: TestCase) {
    let value = value(&tc);
    let unit = match tc.draw(gs::integers::<u8>().min_value(0).max_value(7)) {
        0 => None,
        1 => Some("By".to_owned()),
        2 => Some("s".to_owned()),
        3 => Some("ms".to_owned()),
        4 => Some("%".to_owned()),
        5 => Some("1".to_owned()),
        _ => Some(text(&tc)),
    };
    let role = tc.draw(gs::sampled_from(vec![Role::None, Role::Time]));
    let options = FormatOptions {
        time: tc.draw(gs::sampled_from(vec![
            None,
            Some(TimeStyle::Relative),
            Some(TimeStyle::Absolute),
        ])),
        decimals: tc.draw(gs::optional(gs::integers::<u8>().max_value(20))),
        null_glyph: tc.draw(gs::booleans()).then(|| text(&tc)),
        plain: tc.draw(gs::booleans()),
    };
    let zone = FixedOffset(tc.draw(gs::integers::<i32>()));
    let ctx = FormatContext::new(timestamp(&tc), &zone);
    let formatted = format(&value, unit.as_deref(), role, &options, &ctx);
    let stable = format_stable(&value, unit.as_deref(), role, &options, &ctx);
    for f in [&formatted, &stable] {
        assert!(
            !f.text.contains([
                '\n', '\r', '\u{0b}', '\u{0c}', '\u{85}', '\u{2028}', '\u{2029}'
            ]),
            "{value:?} gave {:?}",
            f.text
        );
    }
    let absolute = FormatOptions {
        time: Some(TimeStyle::Absolute),
        ..options.clone()
    };
    let expected = format(&value, unit.as_deref(), role, &absolute, &ctx);
    assert_eq!(stable, expected, "{value:?}");
}
