//! binnacle-described: a sans-IO Table Source for Described Tables, the
//! tables a server describes on the wire. A `table` message carries a
//! Frictionless-style schema, then rows come as maps from field name to
//! value, as newline-delimited JSON or a CBOR sequence.
//!
//! The host does all I/O: a [`DescribedSource`] asks it to open a watch
//! ([`HostRequest::OpenWatch`]), the host feeds back the bytes it reads
//! through a [`Decoder`] (or messages it decoded itself), and the source
//! answers the Grid as an In-Memory Table Source over the rows. Live
//! changes (`upsert`, `delete`), resyncs, schema changes and the stream's
//! end apply in place; only a message of a type this crate does not know
//! comes back [`Received::Unhandled`]. [`encode_json_line`] and
//! [`encode_cbor_item`] write messages, as a server (or a fake one) does.

mod keys;
mod layout;
mod message;
mod scope;
mod source;
mod wire;

pub use message::{
    Category, DecodeError, Decoder, Encoding, Field, FieldType, ForeignKey, Message, Row,
    RowsMessage, Schema, TableMessage, decode_cbor_item, decode_json_line, encode_cbor_item,
    encode_json_line,
};
pub use scope::{HostRequest, ROW_CAP, Scope, WatchRequest};
pub use source::{Column, DescribedSource, Received, Reference, Status, Unhandled, Values, typed};
pub use wire::{Wire, deterministic_cbor};
