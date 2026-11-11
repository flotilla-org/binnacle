//! The rules a column filter's menu follows, shared by every host so each
//! writes the same conditions and names them the same way: the
//! list-of-values [`checklist`], the [`compare`] row, and [`chip_text`].

pub mod checklist;
pub mod compare;

pub use checklist::{Column, Shown, Tick};

use binnacle_core::{Condition, ConditionKind, Value};

/// A chip's text: `label: text`, with `not ` before the text, the text in
/// slashes for a regex, and ` Aa` when it matches case.
pub fn chip_text(label: &str, c: &Condition) -> String {
    let not = if c.not { "not " } else { "" };
    match &c.kind {
        ConditionKind::Text(q) => {
            let text = if q.regex {
                format!("/{}/", q.text)
            } else {
                q.text.clone()
            };
            let case = if q.case_sensitive { " Aa" } else { "" };
            format!("{label}: {not}{text}{case}")
        }
        ConditionKind::OneOf(values) => format!("{label}: {not}{}", listed_text(values)),
        ConditionKind::Compare(c) => format!("{label}: {not}{}", compare::text(c)),
        // The source's own language, passed through as typed.
        ConditionKind::Native(text) => format!("{label}: {not}‹{text}›"),
        _ => format!("{label}: …"),
    }
}

/// A OneOf's values for its chip: the first three, then how many more;
/// "nothing" for none.
fn listed_text(values: &[Value]) -> String {
    if values.is_empty() {
        return "nothing".to_string();
    }
    let named: Vec<&str> = (values.iter().take(3))
        .map(|v| match v {
            Value::Blank => "(Blanks)",
            Value::Text(t) if t.is_empty() => "\"\"",
            Value::Text(t) => t,
        })
        .collect();
    let more = values.len() - named.len();
    let more = if more > 0 {
        format!(" +{more}")
    } else {
        String::new()
    };
    named.join(", ") + &more
}
