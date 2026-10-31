//! What a Described Table's rows are read within, and the request a source
//! raises for its host to open the watch.

/// What a Described Table's rows are read within: a namespace, which every
/// request needs (a request without one silently gets no rows), and fixed
/// `where` clauses. There is no default, so a source can't be built without
/// a namespace:
///
/// ```compile_fail
/// let scope = binnacle_described::Scope::default();
/// ```
///
/// ```compile_fail
/// let scope = binnacle_described::Scope { namespace: String::new(), wheres: Vec::new() };
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Scope {
    namespace: String,
    wheres: Vec<String>,
}

impl Scope {
    /// The rows of `namespace`, flotilla's own records being in
    /// `flotilla`.
    ///
    /// # Panics
    ///
    /// If `namespace` is empty.
    pub fn new(namespace: impl Into<String>) -> Scope {
        let namespace = namespace.into();
        assert!(!namespace.is_empty(), "a Scope needs a namespace");
        Scope {
            namespace,
            wheres: Vec::new(),
        }
    }

    /// This scope narrowed by a `where` clause in the server's own syntax,
    /// such as `.status.phase == Active`, passed on unchanged.
    pub fn with_where(mut self, clause: impl Into<String>) -> Scope {
        self.wheres.push(clause.into());
        self
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn wheres(&self) -> &[String] {
        &self.wheres
    }
}

/// The most rows a watch asks for. A table with more is shown partial.
pub const ROW_CAP: u64 = 10_000;

/// Something a source asks its host to do, since it does no I/O itself.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostRequest {
    /// Open the rows stream, or reopen it after it ended, and feed each
    /// message back.
    OpenWatch(WatchRequest),
}

/// A request for a table's rows and their changes as they come: `GET
/// {path}?{query}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchRequest {
    pub table: String,
    pub scope: Scope,
    pub limit: u64,
}

impl WatchRequest {
    /// `/tables/{table}/rows`, the table's name percent-encoded.
    pub fn path(&self) -> String {
        format!("/tables/{}/rows", encode(&self.table))
    }

    /// `watch&limit=10000&namespace=…`, then a `where=…` for each clause,
    /// each value percent-encoded.
    pub fn query(&self) -> String {
        let mut query = format!(
            "watch&limit={}&namespace={}",
            self.limit,
            encode(self.scope.namespace())
        );
        for clause in self.scope.wheres() {
            query.push_str("&where=");
            query.push_str(&encode(clause));
        }
        query
    }

    /// The path and query together.
    pub fn target(&self) -> String {
        format!("{}?{}", self.path(), self.query())
    }
}

/// `text` percent-encoded but for RFC 3986's unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
