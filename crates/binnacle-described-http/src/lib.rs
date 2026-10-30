//! binnacle-described-http: an optional HTTP transport for Described
//! Tables. A [`binnacle_described::DescribedSource`] does no I/O; it asks
//! its host to open a watch ([`HostRequest`]). This crate serves those
//! requests over HTTP/1.1 (TCP or a Unix socket, no TLS) and hands back
//! [`Event`]s for the host to feed the source.
//!
//! The host passes in everything: where to connect ([`Endpoint`]), what
//! credential to send ([`Auth`], exactly as given), the encoding to ask for
//! and the backoff. The crate carries no routing policy and never looks for
//! credentials.
//!
//! The source is `!Send`, so the host keeps it on its own thread and the
//! transport's events are `Send`. An async host drives [`Transport::watch`]
//! on its own runtime; a sync host uses [`Blocking`], which runs a
//! current-thread tokio runtime on a thread of its own.
//!
//! Reconnecting is the source's: after [`Event::Ended`] the host calls
//! `source.ended()`, which asks to open the watch again, and the transport
//! waits out its backoff before that request.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use binnacle_described::{DecodeError, Decoder, Encoding, HostRequest, Message};
use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::Request;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};

/// Where the table server is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// A base URL over plain HTTP, such as `http://127.0.0.1:7777`; a path
    /// in it prefixes every request.
    Tcp(String),
    /// A Unix socket, asked with `Host: localhost`.
    Unix(PathBuf),
}

/// The credential to send, exactly as given. Its `Debug` leaves the secret
/// out.
#[derive(Clone, Default, PartialEq, Eq)]
pub enum Auth {
    #[default]
    None,
    /// `Authorization: Bearer {token}`.
    Bearer(String),
    /// `Cookie: {text}`.
    Cookie(String),
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Auth::None => "None",
            Auth::Bearer(_) => "Bearer(..)",
            Auth::Cookie(_) => "Cookie(..)",
        })
    }
}

/// How long to wait before opening a watch again: `initial`, doubling to
/// `max`, back to `initial` once a stream delivers `synced`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
}

impl Default for Backoff {
    /// 250 ms doubling to 10 s.
    fn default() -> Self {
        Backoff {
            initial: Duration::from_millis(250),
            max: Duration::from_secs(10),
        }
    }
}

/// What the host gives the transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub endpoint: Endpoint,
    pub auth: Auth,
    /// The encoding to ask for (`Accept`); NDJSON by default.
    pub encoding: Encoding,
    pub backoff: Backoff,
    /// How long connecting and the response's head (and a failure's body)
    /// may take before the attempt is retried; 10 s by default. A watch's
    /// stream is not bounded.
    pub timeout: Duration,
}

impl Options {
    pub fn new(endpoint: Endpoint) -> Options {
        Options {
            endpoint,
            auth: Auth::None,
            encoding: Encoding::Ndjson,
            backoff: Backoff::default(),
            timeout: Duration::from_secs(10),
        }
    }

    pub fn auth(mut self, auth: Auth) -> Options {
        self.auth = auth;
        self
    }

    pub fn encoding(mut self, encoding: Encoding) -> Options {
        self.encoding = encoding;
        self
    }

    pub fn backoff(mut self, backoff: Backoff) -> Options {
        self.backoff = backoff;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Options {
        self.timeout = timeout;
        self
    }
}

/// What a watch brings, for the host to feed its source.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// A message: give it to `source.receive`.
    Message(Message),
    /// Bytes that are not a message; the stream goes on.
    Undecodable(DecodeError),
    /// The stream ended, cleanly or with an I/O error after the headers:
    /// call `source.ended()`, and serve the request it makes.
    Ended,
    /// The server answered with a status that is not a success (and not
    /// one retried): the watch is not open, and is not opened again.
    Failed { status: u16, body: Vec<u8> },
}

/// Serves a source's watch requests over HTTP.
#[derive(Clone, Debug)]
pub struct Transport {
    options: Options,
    state: Arc<Mutex<Pace>>,
}

/// When the next request may go.
#[derive(Debug)]
struct Pace {
    /// The wait before the next attempt after a failure or an end.
    delay: Duration,
    /// A stream ended or failed: the next request waits `delay` first.
    wait: bool,
}

/// Why an attempt did not open a stream.
enum Attempt {
    /// Try again after the backoff: refused, or a 5xx.
    Retry,
    /// Give up: the host has been told.
    Done,
}

impl Transport {
    pub fn new(options: Options) -> Transport {
        let delay = options.backoff.initial;
        Transport {
            options,
            state: Arc::new(Mutex::new(Pace { delay, wait: false })),
        }
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Serves `request`: `GET {WatchRequest::target()}`, after the backoff
    /// if the last stream ended or failed, retrying a refused connection or
    /// a 5xx with backoff. Each event goes to `emit`, ending with
    /// [`Event::Ended`] or [`Event::Failed`].
    pub async fn watch(&self, request: &HostRequest, mut emit: impl FnMut(Event)) {
        let HostRequest::OpenWatch(watch) = request else {
            return;
        };
        let target = watch.target();
        loop {
            let wait = {
                let mut pace = self.pace();
                match std::mem::replace(&mut pace.wait, false) {
                    true => Some(self.grow(&mut pace)),
                    false => None,
                }
            };
            if let Some(wait) = wait {
                tokio::time::sleep(wait).await;
            }
            match self.attempt(&target, &mut emit).await {
                Attempt::Retry => self.pace().wait = true,
                Attempt::Done => return,
            }
        }
    }

    fn pace(&self) -> std::sync::MutexGuard<'_, Pace> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The wait due now, doubling the next, up to the most.
    fn grow(&self, pace: &mut Pace) -> Duration {
        let wait = pace.delay;
        pace.delay = (pace.delay * 2).min(self.options.backoff.max);
        wait
    }

    async fn attempt(&self, target: &str, emit: &mut impl FnMut(Event)) -> Attempt {
        let (host, path) = match &self.options.endpoint {
            Endpoint::Tcp(base) => match split(base) {
                Some((host, prefix)) => (host, format!("{prefix}{target}")),
                None => {
                    emit(Event::Failed {
                        status: 0,
                        body: format!("not an http:// URL without userinfo: {base}").into_bytes(),
                    });
                    return Attempt::Done;
                }
            },
            Endpoint::Unix(_) => ("localhost".to_owned(), target.to_owned()),
        };
        let request = Request::get(path)
            .header(hyper::header::HOST, host.as_str())
            .header(hyper::header::ACCEPT, self.options.encoding.content_type());
        let secret = |text: String| {
            let mut value = hyper::header::HeaderValue::try_from(text).ok()?;
            value.set_sensitive(true);
            Some(value)
        };
        let request = match &self.options.auth {
            Auth::None => Some(request),
            Auth::Bearer(token) => (secret(format!("Bearer {token}")))
                .map(|v| request.header(hyper::header::AUTHORIZATION, v)),
            Auth::Cookie(text) => {
                (secret(text.clone())).map(|v| request.header(hyper::header::COOKIE, v))
            }
        };
        let Some(request) = request else {
            emit(Event::Failed {
                status: 0,
                body: b"the credential is not a valid HTTP header value".to_vec(),
            });
            return Attempt::Done;
        };
        let Ok(request) = request.body(Empty::<Bytes>::new()) else {
            emit(Event::Failed {
                status: 0,
                body: b"the credential or target is not a valid HTTP header or path".to_vec(),
            });
            return Attempt::Done;
        };
        #[cfg(not(unix))]
        if let Endpoint::Unix(_) = &self.options.endpoint {
            emit(Event::Failed {
                status: 0,
                body: b"no Unix sockets here".to_vec(),
            });
            return Attempt::Done;
        }
        // Connecting and the response's head are bounded; the body, a
        // watch's stream, is not.
        let opened = tokio::time::timeout(self.options.timeout, async {
            match &self.options.endpoint {
                Endpoint::Tcp(_) => match tokio::net::TcpStream::connect(&host).await {
                    Ok(stream) => send(stream, request).await.ok(),
                    Err(_) => None,
                },
                #[cfg(unix)]
                Endpoint::Unix(path) => match tokio::net::UnixStream::connect(path).await {
                    Ok(stream) => send(stream, request).await.ok(),
                    Err(_) => None,
                },
                #[cfg(not(unix))]
                Endpoint::Unix(_) => None,
            }
        });
        let Ok(Some(response)) = opened.await else {
            return Attempt::Retry;
        };
        let status = response.status();
        if status.is_server_error() {
            return Attempt::Retry;
        }
        if !status.is_success() {
            let mut body = Vec::new();
            let mut incoming = response.into_body();
            let read = async {
                while body.len() < MAX_FAILED_BODY {
                    match incoming.frame().await {
                        Some(Ok(frame)) => {
                            if let Ok(data) = frame.into_data() {
                                body.extend_from_slice(&data);
                            }
                        }
                        _ => break,
                    }
                }
            };
            // What came in time is enough to say why.
            let _ = tokio::time::timeout(self.options.timeout, read).await;
            body.truncate(MAX_FAILED_BODY);
            emit(Event::Failed {
                status: status.as_u16(),
                body,
            });
            self.pace().wait = true;
            return Attempt::Done;
        }
        let encoding = (response.headers().get(hyper::header::CONTENT_TYPE))
            .and_then(|v| v.to_str().ok())
            .and_then(Encoding::from_content_type)
            .unwrap_or(self.options.encoding);
        let mut decoder = Decoder::new(encoding);
        let mut body = response.into_body();
        let drain = |decoder: &mut Decoder, emit: &mut dyn FnMut(Event)| {
            while let Some(message) = decoder.next() {
                self.emit(message, emit);
            }
        };
        while let Some(frame) = body.frame().await {
            let Ok(frame) = frame else {
                break;
            };
            if let Ok(data) = frame.into_data() {
                decoder.feed(&data);
                drain(&mut decoder, emit);
            }
        }
        if let Some(last) = decoder.finish() {
            self.emit(last, emit);
        }
        self.pace().wait = true;
        emit(Event::Ended);
        Attempt::Done
    }

    /// Hands on one decoded message; a `synced` resets the backoff.
    fn emit(&self, message: Result<Message, DecodeError>, emit: &mut dyn FnMut(Event)) {
        match message {
            Ok(message) => {
                if let Message::Synced { .. } = message {
                    self.pace().delay = self.options.backoff.initial;
                }
                emit(Event::Message(message));
            }
            Err(e) => emit(Event::Undecodable(e)),
        }
    }
}

/// Sends `request` over a fresh connection.
async fn send<S>(
    stream: S,
    request: Request<Empty<Bytes>>,
) -> Result<hyper::Response<hyper::body::Incoming>, hyper::Error>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let (mut sender, connection) = http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    sender.send_request(request).await
}

/// The most of a failed answer's body kept for [`Event::Failed`].
const MAX_FAILED_BODY: usize = 64 << 10;

/// `http://host:port/prefix` as `host:port` and `/prefix`, port 80 if none
/// is given; an IPv6 host is bracketed, `http://[::1]:7777`. A URL with
/// userinfo is refused: credentials go in [`Auth`].
fn split(base: &str) -> Option<(String, String)> {
    let rest = base.strip_prefix("http://")?;
    let (host, prefix) = match rest.find('/') {
        Some(at) => (&rest[..at], rest[at..].trim_end_matches('/')),
        None => (rest, ""),
    };
    if host.contains('@') {
        // Credentials go in `Auth`, never in the URL.
        return None;
    }
    let host = host.strip_suffix(':').unwrap_or(host);
    let port = match host.rfind(']') {
        Some(end) => host[end..].contains(':'),
        None => host.contains(':'),
    };
    let host = match port {
        true => host.to_owned(),
        false => format!("{host}:80"),
    };
    Some((host, prefix.to_owned()))
}

/// A transport for a sync host: a thread of its own runs a current-thread
/// tokio runtime, and events come back through a channel. The channels are
/// unbounded, so a host reads events each frame. Dropping it stops the
/// thread and waits for its runtime to shut down, dropping any watch still
/// open.
pub struct Blocking {
    requests: Option<tokio::sync::mpsc::UnboundedSender<HostRequest>>,
    events: mpsc::Receiver<Event>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Blocking {
    pub fn new(options: Options) -> std::io::Result<Blocking> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (requests, mut incoming) = tokio::sync::mpsc::unbounded_channel::<HostRequest>();
        let (events, outgoing) = mpsc::channel();
        let transport = Transport::new(options);
        let thread = thread::Builder::new()
            .name("binnacle-described-http".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let local = tokio::task::LocalSet::new();
                    local
                        .run_until(async move {
                            while let Some(request) = incoming.recv().await {
                                let transport = transport.clone();
                                let events = events.clone();
                                tokio::task::spawn_local(async move {
                                    transport
                                        .watch(&request, |event| {
                                            let _ = events.send(event);
                                        })
                                        .await;
                                });
                            }
                        })
                        .await;
                });
            })?;
        Ok(Blocking {
            requests: Some(requests),
            events: outgoing,
            thread: Some(thread),
        })
    }

    /// Serves `request`, as [`Transport::watch`] does.
    pub fn open(&self, request: HostRequest) {
        if let Some(requests) = &self.requests {
            let _ = requests.send(request);
        }
    }

    /// The next event, waiting up to `timeout`.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.events.recv_timeout(timeout).ok()
    }

    /// The next event, if one is waiting.
    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}

impl Drop for Blocking {
    fn drop(&mut self) {
        // Closing the channel ends the loop; a watch still streaming is
        // dropped with the runtime.
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn a_base_url_splits_into_its_address_and_prefix() {
        let split = |base| split(base).unwrap();
        assert_eq!(
            split("http://127.0.0.1:7777"),
            ("127.0.0.1:7777".into(), "".into())
        );
        assert_eq!(split("http://host/api/"), ("host:80".into(), "/api".into()));
        assert_eq!(split("http://[::1]"), ("[::1]:80".into(), "".into()));
        assert_eq!(
            split("http://[::1]:7777/x"),
            ("[::1]:7777".into(), "/x".into())
        );
        assert_eq!(split("http://host:"), ("host:80".into(), "".into()));
        assert_eq!(super::split("https://host"), None);
        assert_eq!(super::split("http://user@host"), None);
    }
}
