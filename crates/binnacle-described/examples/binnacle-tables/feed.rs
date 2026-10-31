//! The tables example's live feed, shared with its tests: the events a
//! watch brings, given to the source, and what to tell the user of them.

use std::time::{Duration, Instant};

use binnacle_core::{Axis, ChangeNotice};
use binnacle_described::{DescribedSource, HostRequest, Received};
use binnacle_described_http::Event as Wire;

/// What the feed has to say, for the status line (or stderr, piped).
#[derive(Debug, Default)]
pub struct Notes {
    /// Why the transport is reconnecting, while it is.
    pub reconnecting: Option<String>,
    /// How many live messages could not be decoded, and the latest's text.
    pub undecodable: u32,
    pub latest: Option<String>,
}

impl Notes {
    /// The undecodable messages, in a line: the count, when several, and
    /// the latest's text.
    pub fn corrupted(&self) -> Option<String> {
        let latest = self.latest.as_deref()?;
        Some(match self.undecodable {
            0 | 1 => format!("a live message could not be decoded: {latest}"),
            n => format!("{n} live messages could not be decoded, the latest: {latest}"),
        })
    }

    /// The status line's note: the reconnecting, then the corrupted.
    pub fn line(&self) -> Option<String> {
        let parts: Vec<String> = [self.reconnecting.clone(), self.corrupted()]
            .into_iter()
            .flatten()
            .collect();
        (!parts.is_empty()).then(|| parts.join(" | "))
    }
}

/// Feeds `source` the events `next` gives while it has any, for up to
/// `budget`, so a burst of changes still lets frames draw and keys work
/// between its parts. `open` serves the requests the source makes (the
/// watch again after an end). Keeps `notes` up to date; returns the Change
/// Notices, or the server's answer if it refused the watch or the retries
/// ran out.
pub fn drain(
    source: &mut DescribedSource,
    mut next: impl FnMut() -> Option<Wire>,
    mut open: impl FnMut(HostRequest),
    notes: &mut Notes,
    budget: Duration,
) -> Result<Vec<(Axis, ChangeNotice)>, String> {
    let mut notices = Vec::new();
    let until = Instant::now() + budget;
    while Instant::now() < until {
        let Some(event) = next() else {
            break;
        };
        match event {
            Wire::Message(message) => {
                notes.reconnecting = None;
                if let Received::Applied(n) = source.receive(message) {
                    notices.extend(n);
                }
            }
            Wire::Undecodable(e) => {
                notes.undecodable += 1;
                notes.latest = Some(e.to_string());
            }
            Wire::Retrying {
                attempt,
                wait,
                status,
                body,
            } => {
                // The body's first line says why, when the server gave one.
                let body = String::from_utf8_lossy(&body);
                let body = body.lines().map(str::trim).find(|l| !l.is_empty());
                let why = match (status, body) {
                    (Some(status), Some(body)) => format!("{status} {body}"),
                    (Some(status), None) => status.to_string(),
                    (None, body) => body.unwrap_or_default().to_owned(),
                };
                let wait = wait.as_millis();
                notes.reconnecting = Some(format!(
                    "reconnecting ({why}), retry {attempt} in {wait} ms"
                ));
            }
            Wire::Ended => {
                source.ended();
            }
            Wire::Failed { status, body } => {
                let body = String::from_utf8_lossy(&body);
                return Err(match status {
                    0 => body.into_owned(),
                    status => format!("{status} {body}"),
                });
            }
            _ => {}
        }
        if let Some(request) = source.poll_request() {
            open(request);
        }
    }
    Ok(notices)
}
