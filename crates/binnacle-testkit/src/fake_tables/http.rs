//! Just enough HTTP/1.1 for the fake and its tests: one request per
//! connection, responses whole or chunked, and a small blocking client.

use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Where a fake listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Addr {
    Tcp(SocketAddr),
    #[cfg(unix)]
    Unix(PathBuf),
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Addr::Tcp(addr) => write!(f, "http://{addr}"),
            #[cfg(unix)]
            Addr::Unix(path) => write!(f, "unix:{}", path.display()),
        }
    }
}

/// A connection, over TCP or a Unix socket.
pub(crate) enum Conn {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl Conn {
    pub(crate) fn connect(addr: &Addr) -> io::Result<Conn> {
        match addr {
            Addr::Tcp(addr) => TcpStream::connect(addr).map(Conn::Tcp),
            #[cfg(unix)]
            Addr::Unix(path) => UnixStream::connect(path).map(Conn::Unix),
        }
    }

    pub(crate) fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Conn::Tcp(s) => s.set_read_timeout(timeout),
            #[cfg(unix)]
            Conn::Unix(s) => s.set_read_timeout(timeout),
        }
    }

    pub(crate) fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        match self {
            Conn::Tcp(s) => s.set_nonblocking(nonblocking),
            #[cfg(unix)]
            Conn::Unix(s) => s.set_nonblocking(nonblocking),
        }
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Conn::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            Conn::Unix(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Conn::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            Conn::Unix(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Conn::Tcp(s) => s.flush(),
            #[cfg(unix)]
            Conn::Unix(s) => s.flush(),
        }
    }
}

/// A request the fake took: its method, target (path and query) and
/// headers, names lowercased, in order, and when it came.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub at: Instant,
}

impl Request {
    /// The value of header `name` (any case), if it came.
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        (self.headers.iter())
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The path, without the query.
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    /// Each query parameter, percent-decoded; a bare name has an empty
    /// value.
    pub fn query(&self) -> Vec<(String, String)> {
        let Some((_, query)) = self.target.split_once('?') else {
            return Vec::new();
        };
        (query.split('&'))
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (decode(k), decode(v))
            })
            .collect()
    }
}

/// `text` percent-decoded, `+` as a space.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reads a request's head from `conn`.
/// The longest request head the fake reads.
const MAX_HEAD: u64 = 64 << 10;

pub(crate) fn read_request(conn: &mut Conn) -> io::Result<Request> {
    let mut reader = BufReader::new(conn.take(MAX_HEAD));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no request line",
        ));
    };
    let (method, target) = (method.to_owned(), target.to_owned());
    let mut headers = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line.trim_end().is_empty() {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
        if headers.len() > 100 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "too many headers",
            ));
        }
    }
    Ok(Request {
        method,
        target,
        headers,
        at: Instant::now(),
    })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

/// Writes a whole response.
pub(crate) fn respond(
    conn: &mut Conn,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status} {}\r\ncontent-type: {content_type}\r\ncache-control: no-store\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        reason(status),
        body.len()
    );
    conn.write_all(head.as_bytes())?;
    conn.write_all(body)?;
    conn.flush()
}

/// Writes the head of a chunked response.
pub(crate) fn start_chunked(conn: &mut Conn, content_type: &str) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncache-control: no-store\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n"
    );
    conn.write_all(head.as_bytes())?;
    conn.flush()
}

/// Writes one chunk; an empty one ends the body.
pub(crate) fn chunk(conn: &mut Conn, bytes: &[u8]) -> io::Result<()> {
    conn.write_all(format!("{:x}\r\n", bytes.len()).as_bytes())?;
    conn.write_all(bytes)?;
    conn.write_all(b"\r\n")?;
    conn.flush()
}

/// A response as the client reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    /// Names lowercased, in order.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// The value of header `name` (any case), if it came.
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        (self.headers.iter())
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The biggest chunk the client takes.
const MAX_CHUNK: usize = 64 << 20;

/// A response whose body is still coming: chunk by chunk if chunked, else
/// to the end of the connection.
pub struct Stream {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    reader: BufReader<Conn>,
    chunked: bool,
    done: bool,
}

impl Stream {
    /// The value of header `name` (any case), if it came.
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        (self.headers.iter())
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// How long a read may wait before it fails with `WouldBlock` or
    /// `TimedOut`.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.reader.get_ref().set_read_timeout(timeout)
    }

    /// The next piece of the body, `None` at its end.
    pub fn next_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        if self.done {
            return Ok(None);
        }
        if !self.chunked {
            let mut buf = vec![0; 8192];
            let n = self.reader.read(&mut buf)?;
            buf.truncate(n);
            self.done = n == 0;
            return Ok((n > 0).then_some(buf));
        }
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            self.done = true;
            return Ok(None);
        }
        let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or(""), 16)
            .ok()
            .filter(|&size| size <= MAX_CHUNK)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "a bad chunk size"))?;
        let mut buf = vec![0; size + 2];
        self.reader.read_exact(&mut buf)?;
        buf.truncate(size);
        if size == 0 {
            self.done = true;
            return Ok(None);
        }
        Ok(Some(buf))
    }

    /// The rest of the body.
    pub fn read_to_end(&mut self) -> io::Result<Vec<u8>> {
        let mut body = Vec::new();
        while let Some(chunk) = self.next_chunk()? {
            body.extend(chunk);
        }
        Ok(body)
    }
}

/// Sends `GET target` to `addr` with `headers` (and `Host: localhost`),
/// and reads the response's head.
pub fn open(addr: &Addr, target: &str, headers: &[(&str, &str)]) -> io::Result<Stream> {
    let mut conn = Conn::connect(addr)?;
    let mut head = format!("GET {target} HTTP/1.1\r\nhost: localhost\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("connection: close\r\n\r\n");
    conn.write_all(head.as_bytes())?;
    let mut reader = BufReader::new(conn);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let status = (line.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no status line"))?;
    let mut response_headers = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line.trim_end().is_empty() {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(':') {
            response_headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
    }
    let chunked = (response_headers.iter())
        .any(|(n, v)| n == "transfer-encoding" && v.eq_ignore_ascii_case("chunked"));
    Ok(Stream {
        status,
        headers: response_headers,
        reader,
        chunked,
        done: false,
    })
}

/// Sends `GET target` and reads the whole response.
pub fn fetch(addr: &Addr, target: &str, headers: &[(&str, &str)]) -> io::Result<Response> {
    let mut stream = open(addr, target, headers)?;
    let body = stream.read_to_end()?;
    Ok(Response {
        status: stream.status,
        headers: stream.headers,
        body,
    })
}
