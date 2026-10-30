//! The CLI's flags, which patch a Layout rather than layer one: each flag
//! sets only the fields it names in a copy of the resolved base Layout, so
//! the base's hidden columns, pins and the rest stay.

use std::fmt;

use binnacle_format::Timestamp;
use binnacle_layout::view::{
    Comparison, Condition, ConditionKind, Direction, Scalar, SortColumn, TextMatch,
};
use binnacle_layout::{JsonId, Layout};

use super::Interactive;
use super::print::Format;

/// A flag the parser could not take: unknown, without its value, or with a
/// malformed one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagError(pub String);

impl fmt::Display for FlagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FlagError {}

fn error<T>(message: impl Into<String>) -> Result<T, FlagError> {
    Err(FlagError(message.into()))
}

/// The CLI's flags, as parsed: `--columns a,b`, `--sort [-]field`
/// (repeated or comma-separated), `--where <cond>` (repeated),
/// `--view <name>`, `--wide`, and the host's `--format <format>` and
/// `--interactive` or `--no-interactive`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flags {
    pub columns: Option<Vec<String>>,
    /// Field names, each descending if written with a leading `-`.
    pub sort: Vec<(String, Direction)>,
    pub wheres: Vec<String>,
    pub view: Option<String>,
    pub wide: bool,
    pub format: Option<Format>,
    pub interactive: Interactive,
}

impl Flags {
    /// Parses `args`, each flag `--name value` or `--name=value`. Anything
    /// else is an error.
    pub fn parse<S: AsRef<str>>(args: impl IntoIterator<Item = S>) -> Result<Flags, FlagError> {
        let (flags, rest) = Flags::parse_known(args)?;
        match rest.first() {
            Some(other) => error(format!("unknown flag or argument `{other}`")),
            None => Ok(flags),
        }
    }

    /// Parses the flags it knows from `args` and hands back the rest, in
    /// order, for the host's own. A known flag without its value, or with a
    /// malformed one, is an error.
    pub fn parse_known<S: AsRef<str>>(
        args: impl IntoIterator<Item = S>,
    ) -> Result<(Flags, Vec<String>), FlagError> {
        let mut flags = Flags::default();
        let mut rest = Vec::new();
        let mut args = args.into_iter().map(|a| a.as_ref().to_owned());
        while let Some(arg) = args.next() {
            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) if name.starts_with("--") => {
                    (name.to_owned(), Some(value.to_owned()))
                }
                _ => (arg.clone(), None),
            };
            let mut value = |name: &str| -> Result<String, FlagError> {
                match inline.clone().or_else(|| args.next()) {
                    Some(v) if !v.is_empty() => Ok(v),
                    _ => error(format!("{name} needs a value")),
                }
            };
            match name.as_str() {
                "--columns" => {
                    let list = names(&value("--columns")?, "--columns")?;
                    flags.columns = Some(list);
                }
                "--sort" => {
                    for term in names(&value("--sort")?, "--sort")? {
                        let (field, direction) = match term.strip_prefix('-') {
                            Some(field) => (field, Direction::Descending),
                            None => (
                                term.strip_prefix('+').unwrap_or(&term),
                                Direction::Ascending,
                            ),
                        };
                        let field = field.strip_prefix('.').unwrap_or(field);
                        if field.is_empty() {
                            return error(format!("--sort: no field in `{term}`"));
                        }
                        flags.sort.push((field.to_owned(), direction));
                    }
                }
                "--where" => {
                    let clause = value("--where")?;
                    parse_where(&clause)?;
                    flags.wheres.push(clause);
                }
                "--view" => flags.view = Some(value("--view")?),
                "--wide" | "--interactive" | "--no-interactive" if inline.is_some() => {
                    return error(format!("{name} takes no value"));
                }
                "--wide" => flags.wide = true,
                "--format" => {
                    let format = value("--format")?;
                    flags.format = Some(format.parse().map_err(FlagError)?);
                }
                "--interactive" => flags.interactive = Interactive::Always,
                "--no-interactive" => flags.interactive = Interactive::Never,
                _ => rest.push(arg),
            }
        }
        Ok((flags, rest))
    }

    /// `base`, the resolved base Layout (the default and the user's own),
    /// patched with these flags. See [`Flags::apply_with`]; a `--view` is
    /// an error here, as no view can be looked up.
    pub fn apply(
        &self,
        base: &Layout<JsonId, JsonId>,
        fields: &[String],
    ) -> Result<Layout<JsonId, JsonId>, FlagError> {
        self.apply_with(base, fields, |_| None)
    }

    /// `base` patched with these flags, the table having `fields`:
    /// `--view` swaps the base for the view `views` finds by name, `--wide`
    /// empties the columns' hidden, `--columns a,b` sets the columns'
    /// order to `[a, b]` and hides every other field, `--sort` sets the
    /// rows' sort and `--where` adds conditions to the rows' filter. Each
    /// leaves every other part of the base as it was. A field the table
    /// lacks, or a view not found, is an error.
    pub fn apply_with(
        &self,
        base: &Layout<JsonId, JsonId>,
        fields: &[String],
        views: impl Fn(&str) -> Option<Layout<JsonId, JsonId>>,
    ) -> Result<Layout<JsonId, JsonId>, FlagError> {
        let known = |name: &str, flag: &str| match fields.iter().any(|f| f == name) {
            true => Ok(JsonId::from(name)),
            false => error(format!("{flag}: no field `{name}`")),
        };
        let mut layout = match &self.view {
            Some(name) => match views(name) {
                Some(view) => view,
                None => return error(format!("--view: no view `{name}`")),
            },
            None => base.clone(),
        };
        let mut view = layout.view.take().unwrap_or_default();
        if self.wide {
            view.cols.hidden.clear();
        }
        if let Some(columns) = &self.columns {
            let order = (columns.iter())
                .map(|c| known(c, "--columns"))
                .collect::<Result<Vec<_>, _>>()?;
            view.cols.hidden = (fields.iter())
                .filter(|f| !columns.contains(f))
                .map(|f| JsonId::from(f.as_str()))
                .collect();
            let mut set = layout.order.take().unwrap_or_default();
            set.cols = Some(order);
            layout.order = Some(set);
        }
        if !self.sort.is_empty() {
            view.rows.sort = (self.sort.iter())
                .map(|(field, direction)| {
                    Ok(SortColumn {
                        id: known(field, "--sort")?,
                        direction: *direction,
                    })
                })
                .collect::<Result<_, FlagError>>()?;
        }
        for clause in &self.wheres {
            let condition = parse_where(clause)?;
            known(condition.id.value().as_str().unwrap_or_default(), "--where")?;
            view.rows.filter.conditions.push(condition);
        }
        layout.view = Some(view);
        Ok(layout)
    }
}

/// A comma-separated list of names, none empty.
fn names(list: &str, flag: &str) -> Result<Vec<String>, FlagError> {
    let names: Vec<String> = list.split(',').map(|n| n.trim().to_owned()).collect();
    match names.iter().any(String::is_empty) {
        true => error(format!("{flag}: an empty name in `{list}`")),
        false => Ok(names),
    }
}

/// The operators of a `--where`, two-character ones first.
const OPERATORS: [&str; 7] = ["==", "!=", "<=", ">=", "<", ">", "~"];

/// Reads a `--where` clause as a condition on the rows:
///
/// - `field==value` keeps the rows whose value is `value` (OneOf), and
///   `field!=value` the others;
/// - `<`, `<=`, `>` and `>=` compare with a number, or with a date when the
///   value reads as RFC 3339;
/// - `field~text` keeps the rows whose value contains `text`.
///
/// A leading `.` on the field is taken, as flotilla writes it.
pub fn parse_where(clause: &str) -> Result<Condition<JsonId>, FlagError> {
    let found = (0..clause.len())
        .filter(|&i| clause.is_char_boundary(i))
        .find_map(|i| {
            (OPERATORS.iter())
                .find(|op| clause[i..].starts_with(**op))
                .map(|op| (i, *op))
        });
    let Some((at, op)) = found else {
        return error(format!(
            "--where: no operator in `{clause}` (==, !=, <, <=, >, >=, ~)"
        ));
    };
    let field = clause[..at].trim();
    let field = field.strip_prefix('.').unwrap_or(field);
    let value = clause[at + op.len()..].trim();
    if field.is_empty() || field.contains(char::is_whitespace) {
        return error(format!("--where: no field before `{op}` in `{clause}`"));
    }
    let condition = |kind, not| Condition {
        id: JsonId::from(field),
        kind,
        not,
    };
    let one_of = || ConditionKind::OneOf {
        values: vec![Some(value.to_owned())],
    };
    Ok(match op {
        "==" => condition(one_of(), false),
        "!=" => condition(one_of(), true),
        "~" if value.is_empty() => {
            return error(format!("--where: no text after `~` in `{clause}`"));
        }
        "~" => condition(
            ConditionKind::Text {
                query: TextMatch {
                    text: value.to_owned(),
                    case_sensitive: false,
                    regex: false,
                },
            },
            false,
        ),
        _ => {
            let scalar = match (value.parse::<f64>(), Timestamp::parse_rfc3339(value)) {
                (Ok(x), _) if x.is_finite() => Scalar::Number { value: x },
                (_, Some(at)) => Scalar::Date {
                    value: (at.unix_nanos() / 1_000_000) as f64,
                },
                _ => {
                    return error(format!(
                        "--where: `{value}` is not a number or an RFC 3339 date in `{clause}`"
                    ));
                }
            };
            let comparison = match op {
                "<" => Comparison::Less { value: scalar },
                "<=" => Comparison::AtMost { value: scalar },
                ">" => Comparison::Greater { value: scalar },
                _ => Comparison::AtLeast { value: scalar },
            };
            condition(ConditionKind::Compare { comparison }, false)
        }
    })
}

/// The flags as a ready-made `clap` argument group, to flatten into a
/// host's own parser; [`FlagArgs::flags`] gives the parser's input.
#[cfg(feature = "clap")]
#[derive(Clone, Debug, Default, clap::Args)]
pub struct FlagArgs {
    /// Show only these columns, in this order (field names, comma-separated).
    #[arg(long, value_delimiter = ',')]
    pub columns: Option<Vec<String>>,
    /// Sort the rows by a field, descending with a leading `-`; repeat or
    /// comma-separate for more.
    #[arg(long, value_delimiter = ',', allow_hyphen_values = true)]
    pub sort: Vec<String>,
    /// Keep the rows a condition keeps: `field==v`, `!=`, `<`, `<=`, `>`,
    /// `>=` or `field~text`.
    #[arg(long = "where")]
    pub wheres: Vec<String>,
    /// Start from the named view.
    #[arg(long)]
    pub view: Option<String>,
    /// Show the hidden columns too.
    #[arg(long)]
    pub wide: bool,
    /// aligned, tsv, json or jsonl.
    #[arg(long)]
    pub format: Option<String>,
    /// Open the table inline even when not in a terminal.
    #[arg(long, conflicts_with = "no_interactive")]
    pub interactive: bool,
    /// Print the table even in a terminal.
    #[arg(long)]
    pub no_interactive: bool,
}

#[cfg(feature = "clap")]
impl FlagArgs {
    /// The flags as [`Flags::parse`] reads them.
    pub fn flags(&self) -> Result<Flags, FlagError> {
        let mut args: Vec<String> = Vec::new();
        if let Some(columns) = &self.columns {
            args.extend(["--columns".into(), columns.join(",")]);
        }
        for term in &self.sort {
            args.push(format!("--sort={term}"));
        }
        for clause in &self.wheres {
            args.extend(["--where".into(), clause.clone()]);
        }
        if let Some(view) = &self.view {
            args.extend(["--view".into(), view.clone()]);
        }
        if self.wide {
            args.push("--wide".into());
        }
        if let Some(format) = &self.format {
            args.extend(["--format".into(), format.clone()]);
        }
        if self.interactive {
            args.push("--interactive".into());
        }
        if self.no_interactive {
            args.push("--no-interactive".into());
        }
        Flags::parse(args)
    }
}
