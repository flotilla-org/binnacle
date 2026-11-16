//! The row conditions a source sends its server as `where` clauses, in the
//! condition language flotillad speaks (`rigging-condition`). A clause keeps
//! every row the In-Memory Table Source's filter keeps, and of the rows whose
//! cell the server holds, no other; a blank, an invalid cell or an integer
//! past i64 it reads as missing, and the clause may keep more of those. So
//! the server only narrows what comes, and the source filters every row
//! again. A condition with no such clause, or whose field's `whereOperators`
//! lack an operator it needs, is filtered client-side only.

use binnacle_core::{Comparison, Condition, ConditionKind, Scalar, Value as Listed, ViewSpec};
use binnacle_format::{FixedOffset, FormatOptions, Value, ValueType};
use rigging_condition::{
    Checked, Condition as Clause, Field as Named, Literal, Operand, Path, Relop, Schema, Timestamp,
    Type,
};

use crate::message::FieldType;
use crate::source::{Column, stable_text};

/// The most bytes and list elements a clause may have (the condition
/// language's limits).
const MAX_TEXT: usize = 4096;
const MAX_LIST: usize = 32;

/// 2^53: each integer strictly between its bounds is an f64 of its own, so
/// a value compared as an f64 compares as the integer does.
const EXACT: f64 = 9_007_199_254_740_992.0;

/// The clauses for the rows' conditions of `spec`, in order and each once:
/// those on the Leaves and the Slicers. A comparison a grouped axis reads
/// after aggregating (`HAVING`) is never sent.
pub(crate) fn clauses(columns: &[Column], spec: &ViewSpec) -> Vec<String> {
    let grouping = spec.grouping.as_ref();
    let values = grouping
        .filter(|g| !g.row_levels.is_empty() || g.pivot())
        .map_or(&[][..], |g| g.values.as_slice());
    let rows = (spec.rows.filter.conditions.conditions().iter())
        .filter(|c| binnacle_group::having(c, values).is_none());
    let slicers = grouping.into_iter().flat_map(|g| g.slicers.iter());
    let mut clauses: Vec<String> = Vec::new();
    for clause in rows.chain(slicers).filter_map(|c| clause(columns, c)) {
        if !clauses.contains(&clause) {
            clauses.push(clause);
        }
    }
    clauses
}

/// Whether the source applies Native condition `condition`: its text parses
/// in some CEL spelling of the subset and checks against `columns`, naming
/// only filterable fields with the operators they allow. Text that does not
/// is never sent, so the server never refuses it and stops the watch.
pub(crate) fn native_sent(columns: &[Column], condition: &Condition) -> bool {
    native(columns, condition).is_some()
}

/// The clause for Native condition `condition`: its text's canonical print,
/// negated with `not`.
fn native(columns: &[Column], condition: &Condition) -> Option<String> {
    let ConditionKind::Native(text) = &condition.kind else {
        return None;
    };
    let parsed = Clause::parse_lenient(text).ok()?;
    let filterable: Vec<(Path, &Column)> = (columns.iter())
        .filter(|c| !c.field().where_operators.is_empty())
        .filter_map(|c| Some((path(columns, c)?, c)))
        .collect();
    let mut schema = Schema::new();
    for (path, column) in &filterable {
        if let Some(ty) = condition_type(column) {
            schema.insert_field(path.clone(), ty, true).ok()?;
        }
    }
    let allowed = |path: &Path, op: &str| {
        (filterable.iter())
            .any(|(p, c)| p == path && Ops(&c.field().where_operators).allow(op).is_some())
    };
    if !operators(&parsed)?
        .iter()
        .all(|(path, op)| allowed(path, op))
    {
        return None;
    }
    let parsed = Checked::new(parsed, &schema).ok()?.into_inner();
    printed(match condition.not {
        false => parsed,
        true => Clause::Not(Box::new(parsed)),
    })
}

/// The condition type of `column`'s values, as flotillad declares them: a
/// string with categories or `constraints.enum` is an enum.
fn condition_type(column: &Column) -> Option<Type> {
    let field = column.field();
    let listed = (field.property("constraints"))
        .and_then(|c| c.get("enum"))
        .and_then(serde_json::Value::as_array)
        .map(|values| values.iter().filter_map(|v| v.as_str().map(String::from)));
    Some(match column.field_type() {
        FieldType::String => match (column.categories(), listed) {
            (Some(categories), _) => Type::enumeration(categories.iter().map(|c| c.value.clone())),
            (None, Some(listed)) => Type::enumeration(listed),
            (None, None) => Type::String,
        },
        FieldType::Other(name) if name == "address" => Type::String,
        FieldType::Integer => Type::Int,
        FieldType::Number => Type::Number,
        FieldType::Boolean => Type::Bool,
        FieldType::Datetime => Type::Timestamp,
        _ => return None,
    })
}

/// Each field `clause` reads, with the operator it reads it by; `None` for
/// a form this doesn't know.
fn operators(clause: &Clause) -> Option<Vec<(Path, &'static str)>> {
    let mut found = Vec::new();
    let mut stack = vec![clause];
    while let Some(clause) = stack.pop() {
        match clause {
            Clause::And(all) | Clause::Or(all) => stack.extend(all),
            Clause::Not(inner) => stack.push(inner),
            Clause::Has(path) => found.push((path.clone(), "has")),
            Clause::Compare { field, op, .. } => found.push((field.path.clone(), op.symbol())),
            Clause::In { field, .. } | Clause::Member { field, .. } => {
                found.push((field.path.clone(), "in"))
            }
            Clause::Under { field, .. } => found.push((field.path.clone(), "under")),
            Clause::Field(field) => found.push((field.path.clone(), "==")),
            Clause::Bool(_) => {}
            _ => return None,
        }
    }
    Some(found)
}

/// The clause for `condition`, if one keeps every row it keeps and its
/// field allows every operator in it. Native text goes in its canonical
/// spelling.
pub(crate) fn clause(columns: &[Column], condition: &Condition) -> Option<String> {
    let column = columns.iter().find(|c| c.key() == condition.key)?;
    if let ConditionKind::Native(_) = &condition.kind {
        return native(columns, condition);
    }
    let ops = Ops(&column.field().where_operators);
    let field = Named::new(path(columns, column)?);
    let built = match &condition.kind {
        ConditionKind::OneOf(values) => one_of(column, &field, values, condition.not, ops)?,
        ConditionKind::Compare(comparison) => {
            let kept = compare(column, &field, comparison, ops)?;
            // An integer past i64 is greater than any literal, and the
            // server reads it as missing.
            let past = matches!(column.field_type(), FieldType::Integer)
                && matches!(
                    comparison,
                    Comparison::Greater(_) | Comparison::AtLeast(_) | Comparison::NotEqual(_)
                );
            match (condition.not, past) {
                (false, false) => kept,
                (false, true) => {
                    ops.allow("has")?;
                    Clause::or([absent(&field), kept])
                }
                (true, _) => {
                    ops.allow("has")?;
                    Clause::or([absent(&field), Clause::Not(Box::new(kept))])
                }
            }
        }
        _ => return None,
    };
    printed(built)
}

/// `clause`'s canonical text, if it is within the limits and parses back to
/// itself.
fn printed(clause: Clause) -> Option<String> {
    let text = clause.to_string();
    let back = Clause::parse_strict(&text).ok()?;
    (text.len() <= MAX_TEXT && back == clause).then_some(text)
}

/// A field's `whereOperators`.
#[derive(Clone, Copy)]
struct Ops<'a>(&'a [String]);

impl Ops<'_> {
    fn allow(self, op: &str) -> Option<()> {
        self.0.iter().any(|o| o == op).then_some(())
    }
}

/// The path that names `column` in a condition: its name split at each dot,
/// or the whole name as one segment where another filterable column's name
/// is a dotted prefix of it, as flotillad names them.
fn path(columns: &[Column], column: &Column) -> Option<Path> {
    let name = column.name();
    let prefixed = (columns.iter())
        .filter(|c| c.key() != column.key() && !c.field().where_operators.is_empty())
        .any(|c| {
            name.strip_prefix(c.name())
                .is_some_and(|r| r.starts_with('.'))
        });
    match prefixed {
        true => Path::new([name]).ok(),
        false => Path::new(name.split('.')).ok(),
    }
}

fn absent(field: &Named) -> Clause {
    Clause::Not(Box::new(Clause::Has(field.path.clone())))
}

/// OneOf: `f in [...]`, with `!has(f) ||` where a row the server reads as
/// missing may show as a listed value: a blank, or an invalid cell showing
/// its raw text. With `not`, `!has(f) || !(f in [...])`, which keeps every
/// row the server reads as missing; the client drops the blanks again.
/// `not` with only a blank listed is not sent: the server reads a blank and
/// an invalid cell alike, and the client keeps the invalid one.
fn one_of(
    column: &Column,
    field: &Named,
    values: &[Listed],
    not: bool,
    ops: Ops<'_>,
) -> Option<Clause> {
    let blank = values.contains(&Listed::Blank);
    let mut literals: Vec<Literal> = Vec::new();
    let mut missing = blank;
    for value in values {
        if let Listed::Text(text) = value {
            let shown = preimage(column, text)?;
            missing |= shown.missing;
            for literal in shown.literals {
                if !literals.contains(&literal) {
                    literals.push(literal);
                }
            }
        }
    }
    let listed = match literals.is_empty() {
        true => None,
        false => Some(member(field, literals, ops)?),
    };
    Some(match (not, missing, listed) {
        (false, false, None) => {
            ops.allow("in").or_else(|| ops.allow("=="))?;
            Clause::Bool(false)
        }
        (false, false, Some(listed)) => listed,
        (false, true, None) => {
            ops.allow("has")?;
            absent(field)
        }
        (false, true, Some(listed)) => {
            ops.allow("has")?;
            Clause::or([absent(field), listed])
        }
        // Nothing listed: `not` keeps every row, or every row but blanks.
        (true, _, None) => return None,
        (true, _, Some(listed)) => {
            ops.allow("has")?;
            Clause::or([absent(field), Clause::Not(Box::new(listed))])
        }
    })
}

/// `f in [...]`, or where the field takes no `in`, `f == a || f == b`.
fn member(field: &Named, literals: Vec<Literal>, ops: Ops<'_>) -> Option<Clause> {
    if literals.len() > MAX_LIST {
        return None;
    }
    let operands = literals.into_iter().map(Operand::Literal);
    if ops.allow("in").is_some() {
        return Some(Clause::In {
            field: field.clone(),
            list: operands.collect(),
        });
    }
    ops.allow("==")?;
    Some(Clause::or(operands.map(|operand| Clause::Compare {
        field: field.clone(),
        op: Relop::Eq,
        operand,
    })))
}

/// What shows as one text: the literals of the values that do, and whether
/// a cell the server reads as missing (an invalid one, showing its raw text)
/// may show as it too.
struct Shown {
    literals: Vec<Literal>,
    missing: bool,
}

/// What shows as `text` in `column`, or `None` when the values that do
/// can't all be listed: text a tab, a line break or a control character may
/// show as; text outside the field's enum (the server holds it but refuses
/// it in a clause); or, with categories, text an invalid cell may show.
fn preimage(column: &Column, text: &str) -> Option<Shown> {
    let shows = |value: Value| {
        let zone = FixedOffset(0);
        let options = FormatOptions::default();
        stable_text(&value, column.unit(), column.role(), &options, &zone).as_deref() == Some(text)
    };
    let texts =
        matches!(column.field_type(), FieldType::String) || column.field_type().name() == "address";
    let exact = |literals| Shown {
        literals,
        missing: false,
    };
    // Any text may be an invalid cell's: a string where the field holds
    // something else.
    let loose = |literals| Shown {
        literals,
        missing: true,
    };
    Some(match (column.field_type(), column.value_type()) {
        (FieldType::String, ValueType::Category) => {
            let categories = column.categories()?;
            // An invalid cell shows text that is no category's value.
            if !categories.iter().any(|c| c.value == text) {
                return None;
            }
            exact(
                (categories.iter())
                    .filter(|c| {
                        shows(Value::Category {
                            value: c.value.clone(),
                            label: c.label.clone(),
                        })
                    })
                    .map(|c| Literal::String(c.value.clone()))
                    .collect(),
            )
        }
        (_, ValueType::Text) if texts => {
            // A tab shows as a space, a line break as `↵` and any other
            // control character as U+FFFD, so text with one of those has
            // values this can't list.
            if text.contains([' ', '↵', '\u{fffd}']) || !in_enum(column, text) {
                return None;
            }
            let literals = match shows(Value::Text(text.to_owned())) {
                true => vec![Literal::String(text.to_owned())],
                false => Vec::new(),
            };
            // An invalid cell, one that isn't text, shows as its JSON.
            let json = serde_json::from_str::<serde_json::Value>(text);
            match json.is_ok_and(|j| !j.is_string() && !j.is_null()) {
                true => loose(literals),
                false => exact(literals),
            }
        }
        (FieldType::Boolean, ValueType::Bool) => loose(
            [true, false]
                .into_iter()
                .filter(|&b| shows(Value::Bool(b)))
                .map(Literal::Bool)
                .collect(),
        ),
        (FieldType::Integer, ValueType::Int) => {
            // Integers show one text each, with no unit or a percentage.
            let digits = match column.unit() {
                None | Some("") | Some("1") => text,
                Some("%") => text.strip_suffix('%').unwrap_or(text),
                Some(_) => return None,
            };
            loose(match (digits.parse::<i64>(), digits.parse::<u64>()) {
                (Ok(n), _) if shows(Value::Int(n)) => vec![Literal::Int(n)],
                // A value past i64 has no literal.
                (Err(_), Ok(u)) if shows(Value::UInt(u)) => return None,
                _ => Vec::new(),
            })
        }
        _ => return None,
    })
}

/// Whether `text` is in the field's `constraints.enum`, when it has one: the
/// server refuses a value outside it in a clause.
fn in_enum(column: &Column, text: &str) -> bool {
    let listed = (column.field().property("constraints"))
        .and_then(|c| c.get("enum"))
        .and_then(serde_json::Value::as_array);
    listed.is_none_or(|values| values.iter().any(|v| v.as_str() == Some(text)))
}

/// A comparison as relations: on an integer field rounded to the integers it
/// keeps, and on a date field to the millisecond, as the filter compares
/// dates.
fn compare(
    column: &Column,
    field: &Named,
    comparison: &Comparison,
    ops: Ops<'_>,
) -> Option<Clause> {
    let relation = |op: Relop, literal: Literal| -> Option<Clause> {
        ops.allow(op.symbol())?;
        Some(Clause::Compare {
            field: field.clone(),
            op,
            operand: Operand::Literal(literal),
        })
    };
    let both = |a: Option<Clause>, b: Option<Clause>| Some(Clause::and([a?, b?]));
    let either = |a: Option<Clause>, b: Option<Clause>| Some(Clause::or([a?, b?]));
    match (column.field_type(), column.value_type()) {
        (FieldType::Number, ValueType::Number) => {
            let at = |s: &Scalar| match *s {
                Scalar::Number(x) if x.is_finite() => Some(Literal::Float(x)),
                _ => None,
            };
            let rel = |op, s: &Scalar| relation(op, at(s)?);
            match comparison {
                Comparison::Equal(s) => rel(Relop::Eq, s),
                Comparison::NotEqual(s) => rel(Relop::Ne, s),
                Comparison::Less(s) => rel(Relop::Lt, s),
                Comparison::AtMost(s) => rel(Relop::Le, s),
                Comparison::Greater(s) => rel(Relop::Gt, s),
                Comparison::AtLeast(s) => rel(Relop::Ge, s),
                Comparison::Between(low, high) => both(rel(Relop::Ge, low), rel(Relop::Le, high)),
            }
        }
        (FieldType::Integer, ValueType::Int) => {
            let number = |s: &Scalar| match *s {
                Scalar::Number(x) if x.abs() < EXACT => Some(x),
                _ => None,
            };
            let int = |x: f64| Literal::Int(x as i64);
            let up = |op, s: &Scalar| relation(op, int(number(s)?.ceil()));
            let down = |op, s: &Scalar| relation(op, int(number(s)?.floor()));
            let whole = |op, s: &Scalar| {
                let x = number(s)?;
                relation(op, (x.fract() == 0.0).then(|| int(x))?)
            };
            match comparison {
                Comparison::Equal(s) => whole(Relop::Eq, s),
                Comparison::NotEqual(s) => whole(Relop::Ne, s),
                Comparison::Less(s) => up(Relop::Lt, s),
                Comparison::AtMost(s) => down(Relop::Le, s),
                Comparison::Greater(s) => down(Relop::Gt, s),
                Comparison::AtLeast(s) => up(Relop::Ge, s),
                Comparison::Between(low, high) => both(up(Relop::Ge, low), down(Relop::Le, high)),
            }
        }
        (FieldType::Datetime, ValueType::DateTime) => {
            // A value compares as the millisecond it falls in.
            let at = |s: &Scalar, plus: i64| match *s {
                Scalar::Date(ms) => {
                    let micros = ms.checked_add(plus)?.checked_mul(1000)?;
                    Timestamp::from_unix_micros(micros).map(Literal::Timestamp)
                }
                Scalar::Number(_) => None,
            };
            let from = |s: &Scalar| relation(Relop::Ge, at(s, 0)?);
            let before = |s: &Scalar| relation(Relop::Lt, at(s, 0)?);
            let after = |s: &Scalar| relation(Relop::Ge, at(s, 1)?);
            let through = |s: &Scalar| relation(Relop::Lt, at(s, 1)?);
            match comparison {
                Comparison::Equal(s) => both(from(s), through(s)),
                Comparison::NotEqual(s) => either(before(s), after(s)),
                Comparison::Less(s) => before(s),
                Comparison::AtMost(s) => through(s),
                Comparison::Greater(s) => after(s),
                Comparison::AtLeast(s) => from(s),
                Comparison::Between(low, high) => both(from(low), through(high)),
            }
        }
        _ => None,
    }
}
