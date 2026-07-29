pub mod encoder;
pub mod parser;

/// Wire protocol version negotiated per connection.
///
/// All connections default to RESP2. A client may upgrade to RESP3 by
/// issuing a `HELLO 3` handshake (to be implemented in P-01).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProtocolVersion {
    #[default]
    Resp2,
    Resp3,
}
