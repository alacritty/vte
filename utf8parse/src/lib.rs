//! A table-driven UTF-8 Parser
//!
//! This module implements a table-driven UTF-8 parser which should
//! theoretically contain the minimal number of branches (1). The only branch is
//! on the `Action` returned from unpacking a transition.
#![no_std]

use core::char;

mod table;
mod types;

use table::TRANSITIONS;
use types::{unpack, Action, State};

/// Handles codepoint and invalid sequence events from the parser.
pub trait Receiver {
    /// Called whenever a codepoint is parsed successfully
    fn codepoint(&mut self, _: char);

    /// Called when an invalid_sequence is detected
    fn invalid_sequence(&mut self);
}

/// A parser for Utf8 Characters
///
/// Repeatedly call `advance` with bytes to emit Utf8 characters
#[derive(Default)]
pub struct Parser {
    point: u32,
    state: State,
}

/// Continuation bytes are masked with this value.
const CONTINUATION_MASK: u8 = 0b0011_1111;

impl Parser {
    /// Create a new Parser
    pub fn new() -> Parser {
        Parser { point: 0, state: State::Ground }
    }

    /// Advance the parser
    ///
    /// The provider receiver will be called whenever a codepoint is completed or an invalid
    /// sequence is detected.
    pub fn advance<R>(&mut self, receiver: &mut R, byte: u8)
    where
        R: Receiver,
    {
        let cur = self.state as usize;
        let change = TRANSITIONS[cur][byte as usize];
        let (state, action) = unsafe { unpack(change) };

        self.perform_action(receiver, byte, action);
        self.state = state;
    }

    fn perform_action<R>(&mut self, receiver: &mut R, byte: u8, action: Action)
    where
        R: Receiver,
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
