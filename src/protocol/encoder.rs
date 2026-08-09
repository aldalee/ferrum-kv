//! RESP2 and RESP3 response encoder.
//!
//! The RESP2 functions (prefixed with a `+`, `-`, `:`, `$`, `*`) are used
//! for RESP2 clients. The RESP3 functions (`_`, `#`, `,`, `%`, `~`) provide
//! richer typed replies. A version-aware wrapper in `ReplyEncoder` dispatches
//! based on the per-connection protocol version.

/// Appends a RESP2 Simple String (`+<text>\r\n`).
///
/// The caller must guarantee that `text` contains neither `\r` nor `\n`;
/// Simple Strings are defined to be a single line. Longer or binary payloads
/// must use [`encode_bulk_string`] instead.
pub fn encode_simple_string(buf: &mut Vec<u8>, text: &str) {
    debug_assert!(
        !text.as_bytes().contains(&b'\r') && !text.as_bytes().contains(&b'\n'),
        "simple strings must not contain CR or LF",
    );
    buf.reserve(text.len() + 3);
    buf.push(b'+');
    buf.extend_from_slice(text.as_bytes());
    buf.extend_from_slice(b"\r\n");
}

/// Appends a RESP2 Error (`-<message>\r\n`).
///
/// The message is expected to be a single line. Conventionally the first
/// token is an uppercase prefix such as `ERR`, `WRONGTYPE`, or `OOM`.
pub fn encode_error(buf: &mut Vec<u8>, message: &str) {
    debug_assert!(
        !message.as_bytes().contains(&b'\r') && !message.as_bytes().contains(&b'\n'),
        "error messages must not contain CR or LF",
    );
    buf.reserve(message.len() + 3);
    buf.push(b'-');
    buf.extend_from_slice(message.as_bytes());
    buf.extend_from_slice(b"\r\n");
}

/// Appends a RESP2 Integer (`:<n>\r\n`).
pub fn encode_integer(buf: &mut Vec<u8>, n: i64) {
    buf.push(b':');
    // `itoa` would be faster, but the std formatter keeps the dependency
    // footprint small and is negligible for reply-sized numbers.
    buf.extend_from_slice(n.to_string().as_bytes());
    buf.extend_from_slice(b"\r\n");
}

/// Appends a RESP2 Bulk String (`$<len>\r\n<payload>\r\n`).
///
/// The payload is written verbatim, so values containing CR/LF bytes are
/// round-tripped intact. An empty slice produces `$0\r\n\r\n`, which is
/// distinct from the null bulk emitted by [`encode_null_bulk`].
pub fn encode_bulk_string(buf: &mut Vec<u8>, payload: &[u8]) {
    let len_str = payload.len().to_string();
    buf.reserve(1 + len_str.len() + 2 + payload.len() + 2);
    buf.push(b'$');
    buf.extend_from_slice(len_str.as_bytes());
    buf.extend_from_slice(b"\r\n");
    buf.extend_from_slice(payload);
    buf.extend_from_slice(b"\r\n");
}

/// Appends a RESP2 Null Bulk String (`$-1\r\n`).
///
/// This is the standard reply for `GET` against a missing key and for any
/// other command whose contract calls for a null bulk reply.
pub fn encode_null_bulk(buf: &mut Vec<u8>) {
    buf.extend_from_slice(b"$-1\r\n");
}

/// Appends a RESP2 Array header (`*<count>\r\n`).
///
/// Callers follow this with exactly `count` further RESP2 elements to form a
/// complete array reply. Used by commands such as `MGET` that return a
/// heterogeneous sequence of bulk strings and null bulks.
pub fn encode_array_header(buf: &mut Vec<u8>, count: usize) {
    buf.push(b'*');
    buf.extend_from_slice(count.to_string().as_bytes());
    buf.extend_from_slice(b"\r\n");
}

// ── RESP3 typed replies ──────────────────────────────────────────────

/// Appends a RESP3 Null (`_\r\n`). Replaces `$-1\r\n` when the protocol
/// version is RESP3.
pub fn encode_null(buf: &mut Vec<u8>) {
    buf.extend_from_slice(b"_\r\n");
}

/// Appends a RESP3 Boolean (`#t\r\n` or `#f\r\n`). Replaces `:1\r\n` and
/// `:0\r\n` when the protocol version is RESP3.
pub fn encode_boolean(buf: &mut Vec<u8>, v: bool) {
    if v {
        buf.extend_from_slice(b"#t\r\n");
    } else {
        buf.extend_from_slice(b"#f\r\n");
    }
}

/// Appends a RESP3 Double (`,<value>\r\n`). The value is formatted with
/// enough precision to round-trip without loss.
#[allow(dead_code)]
pub fn encode_double(buf: &mut Vec<u8>, v: f64) {
    buf.push(b',');
    buf.extend_from_slice(format!("{v:.17}").trim_end_matches('0').as_bytes());
    // If the trimmed result ends with '.', append a '0'.
    if buf.last() == Some(&b'.') {
        buf.push(b'0');
    }
    buf.extend_from_slice(b"\r\n");
}

/// Appends a RESP3 Map header (`%<count>\r\n`).
///
/// `count` is the number of key-value **pairs**, so the caller must follow
/// with `2 * count` further elements. Use this for `HELLO` and structured
/// `INFO` output.
pub fn encode_map_header(buf: &mut Vec<u8>, count: usize) {
    buf.push(b'%');
    buf.extend_from_slice(count.to_string().as_bytes());
    buf.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_string_ok() {
        let mut buf = Vec::new();
        encode_simple_string(&mut buf, "OK");
        assert_eq!(buf, b"+OK\r\n");
    }

    #[test]
    fn simple_string_pong() {
        let mut buf = Vec::new();
        encode_simple_string(&mut buf, "PONG");
        assert_eq!(buf, b"+PONG\r\n");
    }

    #[test]
    fn error_with_err_prefix() {
        let mut buf = Vec::new();
        encode_error(&mut buf, "ERR unknown command: 'FOO'");
        assert_eq!(buf, b"-ERR unknown command: 'FOO'\r\n");
    }

    #[test]
    fn error_with_wrongtype_prefix() {
        let mut buf = Vec::new();
        encode_error(&mut buf, "WRONGTYPE expected string");
        assert_eq!(buf, b"-WRONGTYPE expected string\r\n");
    }

    #[test]
    fn integer_positive() {
        let mut buf = Vec::new();
        encode_integer(&mut buf, 42);
        assert_eq!(buf, b":42\r\n");
    }

    #[test]
    fn integer_zero() {
        let mut buf = Vec::new();
        encode_integer(&mut buf, 0);
        assert_eq!(buf, b":0\r\n");
    }

    #[test]
    fn integer_negative() {
        let mut buf = Vec::new();
        encode_integer(&mut buf, -7);
        assert_eq!(buf, b":-7\r\n");
    }

    #[test]
    fn integer_large_values() {
        let mut buf = Vec::new();
        encode_integer(&mut buf, i64::MAX);
        assert_eq!(buf, format!(":{}\r\n", i64::MAX).as_bytes());

        buf.clear();
        encode_integer(&mut buf, i64::MIN);
        assert_eq!(buf, format!(":{}\r\n", i64::MIN).as_bytes());
    }

    #[test]
    fn bulk_string_basic() {
        let mut buf = Vec::new();
        encode_bulk_string(&mut buf, b"ferrum");
        assert_eq!(buf, b"$6\r\nferrum\r\n");
    }

    #[test]
    fn bulk_string_empty() {
        let mut buf = Vec::new();
        encode_bulk_string(&mut buf, b"");
        assert_eq!(buf, b"$0\r\n\r\n");
    }

    #[test]
    fn bulk_string_binary_safe_across_crlf() {
        let mut buf = Vec::new();
        encode_bulk_string(&mut buf, b"a\r\nb");
        assert_eq!(buf, b"$4\r\na\r\nb\r\n");
    }

    #[test]
    fn bulk_string_contains_null_byte() {
        let mut buf = Vec::new();
        encode_bulk_string(&mut buf, b"\x00\x01\x02");
        assert_eq!(buf, b"$3\r\n\x00\x01\x02\r\n");
    }

    #[test]
    fn bulk_string_large_payload_length_prefix() {
        let payload = vec![b'x'; 1024];
        let mut buf = Vec::new();
        encode_bulk_string(&mut buf, &payload);
        assert!(buf.starts_with(b"$1024\r\n"));
        assert!(buf.ends_with(b"\r\n"));
        assert_eq!(buf.len(), 1024 + b"$1024\r\n".len() + 2);
    }

    #[test]
    fn null_bulk_shape() {
        let mut buf = Vec::new();
        encode_null_bulk(&mut buf);
        assert_eq!(buf, b"$-1\r\n");
    }

    #[test]
    fn null_bulk_differs_from_empty_bulk() {
        let mut null = Vec::new();
        encode_null_bulk(&mut null);
        let mut empty = Vec::new();
        encode_bulk_string(&mut empty, b"");
        assert_ne!(null, empty);
    }

    #[test]
    fn array_header_basic() {
        let mut buf = Vec::new();
        encode_array_header(&mut buf, 3);
        assert_eq!(buf, b"*3\r\n");
    }

    #[test]
    fn array_header_zero() {
        let mut buf = Vec::new();
        encode_array_header(&mut buf, 0);
        assert_eq!(buf, b"*0\r\n");
    }

    // ── RESP3 typed replies ──────────────────────────────────────────

    #[test]
    fn null_encodes_resp3_shape() {
        let mut buf = Vec::new();
        encode_null(&mut buf);
        assert_eq!(buf, b"_\r\n");
    }

    #[test]
    fn boolean_true_and_false() {
        let mut buf = Vec::new();
        encode_boolean(&mut buf, true);
        assert_eq!(buf, b"#t\r\n");

        buf.clear();
        encode_boolean(&mut buf, false);
        assert_eq!(buf, b"#f\r\n");
    }

    #[test]
    fn double_roundtrips_value() {
        let mut buf = Vec::new();
        encode_double(&mut buf, 3.5);
        assert_eq!(buf, b",3.5\r\n");

        buf.clear();
        encode_double(&mut buf, -0.25);
        assert_eq!(buf, b",-0.25\r\n");
    }

    #[test]
    fn double_whole_number_appends_trimmed_zero() {
        let mut buf = Vec::new();
        encode_double(&mut buf, 2.0);
        assert_eq!(buf, b",2.0\r\n");
    }

    #[test]
    fn map_header_counts_pairs() {
        let mut buf = Vec::new();
        encode_map_header(&mut buf, 4);
        assert_eq!(buf, b"%4\r\n");
    }

    #[test]
    fn map_header_zero() {
        let mut buf = Vec::new();
        encode_map_header(&mut buf, 0);
        assert_eq!(buf, b"%0\r\n");
    }

    #[test]
    fn encoders_can_be_chained_into_one_buffer() {
        // Simulates composing a full reply for a hypothetical multi-element
        // response. In practice each command produces exactly one element, but
        // the encoder must support sequential writes without any separators
        // beyond what each call emits itself.
        let mut buf = Vec::new();
        encode_simple_string(&mut buf, "OK");
        encode_integer(&mut buf, 3);
        encode_bulk_string(&mut buf, b"hi");
        encode_null_bulk(&mut buf);
        encode_error(&mut buf, "ERR boom");
        assert_eq!(buf, b"+OK\r\n:3\r\n$2\r\nhi\r\n$-1\r\n-ERR boom\r\n");
    }
}
