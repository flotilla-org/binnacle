//! The formatter's defaults, per type, unit, role and option.

use binnacle_format::{
    Align, FormatContext, FormatOptions, Formatted, Role, Style, TimeStyle, Timestamp, Value,
    ValueType, fewest_decimals, format,
};
use serde_json::json;

const NOW: &str = "2026-10-29T13:00:00Z";

fn at(rfc3339: &str) -> Timestamp {
    Timestamp::parse_rfc3339(rfc3339).unwrap()
}

fn ctx() -> FormatContext {
    FormatContext {
        now: at(NOW),
        utc_offset: 3600,
    }
}

/// `(text, align, style)` of `value` in a column with `unit` and `role`.
fn shown(value: Value, unit: Option<&str>, role: Role, options: &FormatOptions) -> Formatted {
    format(&value, unit, role, options, &ctx())
}

fn plain(value: Value) -> Formatted {
    shown(value, None, Role::None, &FormatOptions::default())
}

fn f(text: &str, align: Align, style: Style) -> Formatted {
    Formatted {
        text: text.to_owned(),
        align,
        style,
    }
}

use Align::{End, Start};

#[test]
fn each_type_has_its_default_text_align_and_style() {
    let bytes = |n: usize| Value::Bytes((0..n).map(|i| [0x3f, 0xa9, 0xc2, 0x01][i % 4]).collect());
    let table = [
        (Value::Null, f("∅", Start, Style::DIM)),
        (Value::Text(String::new()), f("", Start, Style::NONE)),
        (
            Value::Text("Bergen".into()),
            f("Bergen", Start, Style::NONE),
        ),
        (
            Value::Invalid {
                raw: "12x".into(),
                expected: ValueType::Int,
            },
            f("12x", Start, Style::WARNING),
        ),
        (Value::Int(1_234_567), f("1234567", End, Style::NONE)),
        (Value::Int(-42), f("-42", End, Style::NONE)),
        (
            Value::Int(i64::MIN),
            f("-9223372036854775808", End, Style::NONE),
        ),
        (
            Value::UInt(u64::MAX),
            f("18446744073709551615", End, Style::NONE),
        ),
        (Value::Number(2.5), f("2.5", End, Style::NONE)),
        (Value::Number(-0.125), f("-0.125", End, Style::NONE)),
        (Value::Number(1.0 / 3.0), f("0.333", End, Style::NONE)),
        (Value::Number(-0.0), f("0", End, Style::NONE)),
        (Value::Number(f64::NAN), f("NaN", End, Style::NONE)),
        (Value::Number(f64::INFINITY), f("∞", End, Style::NONE)),
        (Value::Number(f64::NEG_INFINITY), f("-∞", End, Style::NONE)),
        (Value::Number(1.5e20), f("1.5e20", End, Style::NONE)),
        (Value::Bool(true), f("✓", Start, Style::NONE)),
        (Value::Bool(false), f("", Start, Style::NONE)),
        (
            Value::Category {
                value: "warn".into(),
                label: Some("Warning".into()),
            },
            f("Warning", Start, Style::NONE),
        ),
        (
            Value::Category {
                value: "warn".into(),
                label: None,
            },
            f("warn", Start, Style::NONE),
        ),
        (bytes(32), f("32 B 3fa9c2…", Start, Style::NONE)),
        (bytes(3), f("3 B 3fa9c2", Start, Style::NONE)),
        (bytes(0), f("0 B", Start, Style::NONE)),
        (bytes(2048), f("2.0 KiB 3fa9c2…", Start, Style::NONE)),
        (
            Value::Nested(json!({"b": [1, 2], "a": null})),
            f(r#"{"a":null,"b":[1,2]}"#, Start, Style::NONE),
        ),
        (
            Value::Nested(json!({"text": "x".repeat(100)})),
            f(
                &format!(r#"{{"text":"{}…"#, "x".repeat(38)),
                Start,
                Style::NONE,
            ),
        ),
        (
            Value::DateTime(at("2026-10-29T12:03:59Z")),
            f("2026-10-29 13:03", End, Style::NONE),
        ),
        (
            Value::DateTime(at("1969-12-31T23:00:00Z")),
            f("1970-01-01 00:00", End, Style::NONE),
        ),
    ];
    for (value, expected) in table {
        assert_eq!(plain(value.clone()), expected, "{value:?}");
    }
}

#[test]
fn line_breaks_and_control_characters_never_reach_the_text() {
    let cases = [
        ("two\nlines", "two↵lines"),
        ("crlf\r\nonce", "crlf↵once"),
        ("tab\there", "tab here"),
        ("bell\u{7}", "bell\u{fffd}"),
        ("sep\u{2028}", "sep↵"),
    ];
    for (raw, text) in cases {
        assert_eq!(plain(Value::Text(raw.into())).text, text);
        let invalid = Value::Invalid {
            raw: raw.into(),
            expected: ValueType::Number,
        };
        assert_eq!(plain(invalid).text, text);
    }
}

#[test]
fn units_go_through_the_registry_and_unknown_ones_are_a_suffix() {
    let none = FormatOptions::default();
    let unit = |value, unit| shown(value, Some(unit), Role::None, &none).text;
    let table = [
        (Value::Int(0), "By", "0 B"),
        (Value::Int(512), "By", "512 B"),
        (Value::Int(1536), "By", "1.5 KiB"),
        (Value::Int(5 << 20), "By", "5.0 MiB"),
        (Value::UInt(3 << 30), "By", "3.0 GiB"),
        (Value::UInt(u64::MAX), "By", "16777216.0 TiB"),
        (Value::Int(-2048), "By", "-2.0 KiB"),
        (Value::Number(0.5), "By", "0.5 B"),
        (Value::Number(f64::NAN), "By", "NaN B"),
        (Value::Number(0.35), "s", "350 ms"),
        (Value::Int(42), "s", "42 s"),
        (Value::Number(4.25), "s", "4.2 s"),
        (Value::Int(62), "s", "1m 2s"),
        (Value::Int(3720), "s", "1h 2m"),
        (Value::Int(183_600), "s", "2d 3h"),
        (Value::Int(-62), "s", "-1m 2s"),
        (Value::Number(f64::INFINITY), "s", "∞ s"),
        (Value::Int(350), "ms", "350 ms"),
        (Value::Number(0.25), "ms", "0.25 ms"),
        (Value::Int(3_720_000), "ms", "1h 2m"),
        (Value::Number(12.5), "%", "12.5%"),
        (Value::Int(-3), "%", "-3%"),
        (Value::Number(0.75), "1", "0.75"),
        (Value::Int(12), "widgets", "12 widgets"),
        (Value::Int(12), "{widgets}", "12 widgets"),
        (Value::Number(2.5), "m/s", "2.5 m/s"),
        (Value::UInt(u64::MAX), "{rows}", "18446744073709551615 rows"),
    ];
    for (value, code, text) in table {
        assert_eq!(unit(value.clone(), code), text, "{value:?} {code}");
        assert_eq!(
            shown(value.clone(), Some(code), Role::None, &none).align,
            End,
            "{value:?} {code}"
        );
    }
    // A unit leaves values that are not numbers alone.
    assert_eq!(unit(Value::Text("n/a".into()), "By"), "n/a");
    assert_eq!(unit(Value::Null, "By"), "∅");
}

#[test]
fn a_time_role_date_is_relative_to_now_either_way() {
    let none = FormatOptions::default();
    let rel = |rfc| shown(Value::DateTime(at(rfc)), None, Role::Time, &none);
    assert_eq!(rel("2026-10-29T13:00:00Z"), f("just now", End, Style::NONE));
    let table = [
        ("2026-10-29T12:59:01Z", "just now"),
        ("2026-10-29T13:00:59Z", "just now"),
        ("2026-10-29T12:57:00Z", "3m ago"),
        ("2026-10-29T11:00:00Z", "2h ago"),
        ("2026-10-29T15:00:00Z", "in 2h"),
        ("2026-10-29T13:05:30Z", "in 5m"),
        ("2026-10-26T13:00:00Z", "3d ago"),
        ("2024-10-29T13:00:00Z", "2y ago"),
    ];
    for (rfc, text) in table {
        assert_eq!(rel(rfc).text, text, "{rfc}");
    }
}

#[test]
fn a_time_role_date_moves_on_as_the_injected_clock_does() {
    let value = Value::DateTime(at(NOW));
    let options = FormatOptions::default();
    let at_secs = |s| {
        let ctx = FormatContext {
            now: Timestamp::from_unix_secs(at(NOW).unix_secs() + s),
            ..ctx()
        };
        format(&value, None, Role::Time, &options, &ctx).text
    };
    assert_eq!(at_secs(0), "just now");
    assert_eq!(at_secs(59), "just now");
    assert_eq!(at_secs(60), "1m ago");
    assert_eq!(at_secs(3600), "1h ago");
}

#[test]
fn options_override_the_column_defaults() {
    let date = Value::DateTime(at("2026-10-29T12:57:00Z"));
    let absolute = FormatOptions {
        time: Some(TimeStyle::Absolute),
        ..FormatOptions::default()
    };
    assert_eq!(
        shown(date.clone(), None, Role::Time, &absolute).text,
        "2026-10-29 13:57"
    );
    let relative = FormatOptions {
        time: Some(TimeStyle::Relative),
        ..FormatOptions::default()
    };
    assert_eq!(shown(date, None, Role::None, &relative).text, "3m ago");

    let fixed = FormatOptions {
        decimals: Some(2),
        ..FormatOptions::default()
    };
    let decimals = |x| shown(Value::Number(x), None, Role::None, &fixed).text;
    assert_eq!(decimals(7.0), "7.00");
    assert_eq!(decimals(-0.125), "-0.12");
    assert_eq!(decimals(1.0 / 3.0), "0.33");
    // Integers stay digits whatever the decimals.
    assert_eq!(
        shown(Value::Int(7), None, Role::None, &fixed).text,
        "7".to_owned()
    );

    let plain = FormatOptions {
        plain: true,
        ..FormatOptions::default()
    };
    assert_eq!(
        shown(Value::Null, None, Role::None, &plain),
        f("-", Start, Style::DIM)
    );
    let glyph = FormatOptions {
        null_glyph: Some("NULL".into()),
        plain: true,
        ..FormatOptions::default()
    };
    assert_eq!(shown(Value::Null, None, Role::None, &glyph).text, "NULL");
}

#[test]
fn a_column_takes_the_fewest_decimals_that_show_all_its_numbers() {
    assert_eq!(fewest_decimals([]), 0);
    assert_eq!(fewest_decimals([1.0, 2.0, -7.0]), 0);
    assert_eq!(fewest_decimals([1.0, 2.5]), 1);
    assert_eq!(fewest_decimals([1.25, 2.5, f64::NAN, f64::INFINITY]), 2);
    assert_eq!(fewest_decimals([0.1 + 0.2]), 3);
    assert_eq!(fewest_decimals([1.0 / 3.0, 1.0]), 3);
    assert_eq!(fewest_decimals([1e300]), 0);
}

#[test]
fn unknown_roles_are_none_and_types_round_trip_their_names() {
    assert_eq!(Role::from_name("time"), Role::Time);
    assert_eq!(Role::from_name("money"), Role::None);
    for t in ValueType::ALL {
        assert_eq!(ValueType::from_name(t.name()), Some(t));
    }
}

#[test]
fn rfc3339_instants_parse_with_fractions_and_offsets() {
    let base = at("2026-10-29T13:00:00Z").unix_nanos();
    assert_eq!(at("2026-10-29T14:00:00+01:00").unix_nanos(), base);
    assert_eq!(
        at("2026-10-29T13:00:00.5Z").unix_nanos(),
        base + 500_000_000
    );
    assert_eq!(
        at("2026-10-29T13:00:00.123456789Z").unix_nanos(),
        base + 123_456_789
    );
    for bad in [
        "2026-10-29",
        "2026-13-01T00:00:00Z",
        "2026-10-29T13:00:00",
        "x",
    ] {
        assert_eq!(Timestamp::parse_rfc3339(bad), None, "{bad}");
    }
}
