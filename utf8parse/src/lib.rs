//! A table-driven UTF-8 Parser
//!
//! This module implements a table-driven UTF-8 parser which should
//! theoretically contain the minimal number of branches (1). The only branch is
//! on the `Action` returned from unpacking a transition.
#![cfg_attr(all(feature = "nightly", test), feature(test))]

use std::char;

mod types;
use self::types::{State, Action};

#[allow(dead_code)]
mod table;

/// Handles codepoint and invalid sequence events from the parser.
pub trait Receiver {
    /// Called whenever a codepoint is parsed successfully
    fn codepoint(&mut self, char);

    /// Called when an invalid_sequence is detected
    fn invalid_sequence(&mut self);
}

/// A parser for Utf8 Characters
///
/// Repeatedly call `advance` with bytes to emit Utf8 characters
pub struct Parser {
    point: u32,
    state: State,
}

/// Continuation bytes are masked with this value.
const CONTINUATION_MASK: u8 = 0b0011_1111;

impl Parser {
    /// Create a new Parser
    pub fn new() -> Parser {
        Parser {
            point: 0,
            state: State::Ground,
        }
    }

    /// Advance the parser
    ///
    /// The provider receiver will be called whenever a codepoint is completed or an invalid
    /// sequence is detected.
    pub fn advance<R>(&mut self, receiver: &mut R, byte: u8)
        where R: Receiver
    {
        let (state, action) = self.next(byte);
        self.perform_action(receiver, byte, action);
        self.state = state;
    }

    #[inline]
    fn next(&self, byte: u8) -> (State, Action) {
        match self.state {
            State::Ground => match byte {
                0x00...0x7f => (State::Ground,      Action::EmitByte),
                0xc2...0xdf => (State::Tail1,       Action::SetByte2Top),
                0xe0        => (State::U3_2_e0,     Action::SetByte3Top),
                0xe1...0xec => (State::Tail2,       Action::SetByte3Top),
                0xed        => (State::U3_2_ed,     Action::SetByte3Top),
                0xee...0xef => (State::Tail2,       Action::SetByte3Top),
                0xf0        => (State::Utf8_4_3_f0, Action::SetByte4),
                0xf1...0xf3 => (State::Tail3,       Action::SetByte4),
                0xf4        => (State::Utf8_4_3_f4, Action::SetByte4),
                _ => (State::Ground, Action::InvalidSequence),
            },
            State::U3_2_e0 => match byte {
                0xa0...0xbf => (State::Tail1, Action::SetByte2),
                _ => (State::Ground, Action::InvalidSequence),
            },
            State::U3_2_ed => match byte {
                0x80...0x9f => (State::Tail1, Action::SetByte2),
                _ => (State::Ground, Action::InvalidSequence),
            },
            State::Utf8_4_3_f0 => match byte {
                0x90...0xbf => (State::Tail2, Action::SetByte3),
                _ => (State::Ground, Action::InvalidSequence),
            },
            State::Utf8_4_3_f4 => match byte {
                0x80...0x8f => (State::Tail2, Action::SetByte3),
                _ => (State::Ground, Action::InvalidSequence),
            },
            State::Tail3 => match byte {
                0x80...0xbf => (State::Tail2, Action::SetByte3),
                _ => (State::Ground, Action::InvalidSequence),
            },
            State::Tail2 => match byte {
                0x80...0xbf => (State::Tail1, Action::SetByte2),
                _ => (State::Ground, Action::InvalidSequence),
            },
            State::Tail1 => match byte {
                0x80...0xbf => (State::Ground, Action::SetByte1),
                _ => (State::Ground, Action::InvalidSequence),
            },
        }
    }

    fn perform_action<R>(&mut self, receiver: &mut R, byte: u8, action: Action)
        where R: Receiver
    {
        match action {
            Action::InvalidSequence => {
                self.point = 0;
                receiver.invalid_sequence();
            },
            Action::EmitByte => {
                receiver.codepoint(byte as char);
            },
            Action::SetByte1 => {
                let point = self.point | ((byte & CONTINUATION_MASK) as u32);
                let c = unsafe { char::from_u32_unchecked(point) };
                self.point = 0;

                receiver.codepoint(c);
            },
            Action::SetByte2 => {
                self.point |= ((byte & CONTINUATION_MASK) as u32) << 6;
            },
            Action::SetByte2Top => {
                self.point |= ((byte & 0b0001_1111) as u32) << 6;
            },
            Action::SetByte3 => {
                self.point |= ((byte & CONTINUATION_MASK) as u32) << 12;
            },
            Action::SetByte3Top => {
                self.point |= ((byte & 0b0000_1111) as u32) << 12;
            },
            Action::SetByte4 => {
                self.point |= ((byte & 0b0000_0111) as u32) << 18;
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::fs::File;
    use Receiver;
    use Parser;

    impl Receiver for String {
        fn codepoint(&mut self, c: char) {
            self.push(c);
        }

        fn invalid_sequence(&mut self) {
        }
    }

    pub fn get_utf8_text() -> String {
        let mut buffer = String::new();
        let mut file = File::open("src/UTF-8-demo.txt").unwrap();
        // read the file to a buffer
        file.read_to_string(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn utf8parse_test() {
        let buffer = get_utf8_text();
        let mut parser = Parser::new();

        // standard library implementation
        let expected = String::from_utf8(buffer.as_bytes().to_vec()).unwrap();

        // utf8parse implementation
        let mut actual = String::new();

        for byte in buffer.as_bytes().to_vec() {
            parser.advance(&mut actual, byte)
        }

        assert_eq!(actual, expected);
    }
}

#[cfg(all(test, feature="nightly"))]
mod benches {
    extern crate test;

    use super::{Parser, Receiver};
    use super::tests::get_utf8_text;

    use self::test::{black_box, Bencher};

    impl Receiver for () {
        fn codepoint(&mut self, c: char) {
            black_box(c);
        }

        fn invalid_sequence(&mut self) {}
    }

    #[bench]
    fn parse_bench_utf8_demo(b: &mut Bencher) {
        let utf8_bytes = get_utf8_text().into_bytes();

        let mut parser = Parser::new();

        b.iter(|| {
            for byte in &utf8_bytes {
                parser.advance(&mut (), *byte);
            }
        })
    }

    #[bench]
    fn std_string_parse_utf8(b: &mut Bencher) {
        let utf8_bytes = get_utf8_text().into_bytes();

        b.iter(|| {
            for c in ::std::str::from_utf8(&utf8_bytes).unwrap().chars() {
                black_box(c);
            }
        });
    }
}
