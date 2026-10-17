//! Property tests: arbitrary bytes never panic the decoders, and limits hold.

use proptest::prelude::*;
use r9t_protocol::frame::{FrameError, LineDecoder};
use r9t_protocol::rpc::{code, decode_request};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4000))]

    #[test]
    fn decode_request_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode_request(&bytes);
    }

    #[test]
    fn decode_json_shaped_input_never_panics(
        method in "[a-z_]{0,24}",
        params in "\\{[ -~]{0,64}\\}",
        id in prop_oneof![Just("1".to_string()), Just("\"x\"".to_string()), Just("null".to_string()), Just("-3".to_string())],
    ) {
        let line = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"{method}","params":{params}}}"#);
        if let Err((_, e)) = decode_request(line.as_bytes()) {
            prop_assert!([code::PARSE_ERROR, code::INVALID_REQUEST, code::METHOD_NOT_FOUND, code::INVALID_PARAMS].contains(&e.code));
        }
    }

    #[test]
    fn decoder_lines_never_exceed_limit(
        chunks in proptest::collection::vec(proptest::collection::vec(prop_oneof![Just(b'\n'), any::<u8>()], 0..64), 0..32),
        limit in 1usize..48,
    ) {
        let mut d = LineDecoder::new(limit);
        for chunk in &chunks {
            for item in d.push(chunk) {
                match item {
                    Ok(line) => prop_assert!(line.len() <= limit),
                    Err(FrameError::TooLarge { limit: l }) => prop_assert_eq!(l, limit),
                }
            }
            prop_assert!(d.pending() <= limit);
        }
    }
}
