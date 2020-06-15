//! Parser for implementing virtual terminal emulators
//!
//! [`Parser`] is implemented according to [Paul Williams' ANSI parser
//! state machine]. The state machine doesn't assign meaning to the parsed data
//! and is thus not itself sufficient for writing a terminal emulator. Instead,
//! it is expected that an implementation of [`Perform`] is provided which does
//! something useful with the parsed data. The [`Parser`] handles the book
//! keeping, and the [`Perform`] gets to simply handle actions.
//!
//! # Examples
//!
//! For an example of using the [`Parser`] please see the examples folder. The example included
//! there simply logs all the actions [`Perform`] does. One quick thing to see it in action is to
//! pipe `vim` into it
//!
//! ```sh
//! cargo build --release --example parselog
//! vim | target/release/examples/parselog
//! ```
//!
//! Just type `:q` to exit.
//!
//! # Differences from original state machine description
//!
//! * UTF-8 Support for Input
//! * OSC Strings can be terminated by 0x07
//! * Only supports 7-bit codes. Some 8-bit codes are still supported, but they no longer work in
//!   all states.
//! * Support for DCS/SOS/PM/APC can be disabled.
//!
//! [`Parser`]: struct.Parser.html
//! [`Perform`]: trait.Perform.html
//! [Paul Williams' ANSI parser state machine]: https://vt100.net/emu/dec_ansi_parser
#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use, clippy::wrong_pub_self_convention)]
#![cfg_attr(all(feature = "nightly", test), feature(test))]
#![cfg_attr(feature = "no_std", no_std)]

use core::mem::MaybeUninit;

#[cfg(feature = "no_std")]
use arrayvec::ArrayVec;
use utf8parse as utf8;

mod definitions;
mod table;

use definitions::{unpack, Action, State};

const MAX_INTERMEDIATES: usize = 2;
#[cfg(any(feature = "no_std", test))]
const MAX_OSC_RAW: usize = 1024;
const MAX_PARAMS: usize = 16;

struct VtUtf8Receiver<'a, P: Perform>(&'a mut P, &'a mut State);

impl<'a, P: Perform> utf8::Receiver for VtUtf8Receiver<'a, P> {
    fn codepoint(&mut self, c: char) {
        self.0.print(c);
        *self.1 = State::Ground;
    }

    fn invalid_sequence(&mut self) {
        self.0.print('�');
        *self.1 = State::Ground;
    }
}

/// Parser for raw _VTE_ protocol which delegates actions to a [`Perform`]
///
/// [`Perform`]: trait.Perform.html
#[derive(Default)]
pub struct Parser {
    state: State,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_idx: usize,
    params: [i64; MAX_PARAMS],
    param: i64,
    num_params: usize,
    #[cfg(feature = "no_std")]
    osc_raw: ArrayVec<[u8; MAX_OSC_RAW]>,
    #[cfg(not(feature = "no_std"))]
    osc_raw: Vec<u8>,
    osc_params: [(usize, usize); MAX_PARAMS],
    osc_num_params: usize,
    ignoring: bool,
    utf8_parser: utf8::Parser,
    no_dcs_sos_pm_apc: bool,
}

impl Parser {
    /// Create a new Parser
    pub fn new() -> Parser {
        Parser::default()
    }

    /// Disable or enable recognition of DCS, SOS, PM, and APC sequences.
    ///
    /// The only terminator for DCS, SOS, PM, and APC sequences is an 8-bit ST
    /// code, which isn't valid UTF-8, so clients wishing to support only UTF-8
    /// should use this to disable support for these sequences.
    pub fn set_dcs_sos_pm_apc(&mut self, dcs_sos_pm_apc: bool) {
        self.no_dcs_sos_pm_apc = !dcs_sos_pm_apc;
    }

    #[inline]
    fn params(&self) -> &[i64] {
        &self.params[..self.num_params]
    }

    #[inline]
    fn intermediates(&self) -> &[u8] {
        &self.intermediates[..self.intermediate_idx]
    }

    /// Advance the parser state
    ///
    /// Requires a [`Perform`] in case `byte` triggers an action
    ///
    /// [`Perform`]: trait.Perform.html
    #[inline]
    pub fn advance<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        // Utf8 characters are handled out-of-band.
        if let State::Utf8 = self.state {
            self.process_utf8(performer, byte);
            return;
        }

        // Handle state changes in the anywhere state before evaluating changes
        // for current state.
        let mut change = table::STATE_CHANGES[State::Anywhere as usize][byte as usize];

        if change == 0 {
            change = table::STATE_CHANGES[self.state as usize][byte as usize];
        }

        // Unpack into a state and action
        let (state, action) = unpack(change);

        self.perform_state_change(performer, state, action, byte);
    }

    /// Ends the stream.
    ///
    /// This ensures that an incomplete UTF-8 sequence at the end of the input
    /// stream is translated into a replacement character. It isn't necessary to
    /// call this for streams which don't end, such as ttys.
    ///
    /// Requires a [`Perform`] in case the stream ending triggers an action
    ///
    /// [`Perform`]: trait.Perform.html
    #[inline]
    pub fn end<P: Perform>(&mut self, performer: &mut P) {
        if let State::Utf8 = self.state {
            self.process_end_utf8(performer);
            return;
        }

        self.state = State::Ground;
    }

    #[inline]
    fn process_utf8<P>(&mut self, performer: &mut P, byte: u8)
    where
        P: Perform,
    {
        let mut receiver = VtUtf8Receiver(performer, &mut self.state);
        let utf8_parser = &mut self.utf8_parser;
        if !utf8_parser.advance(&mut receiver, byte) {
            // The byte wasn't consumed; reprocess it. Recursion is limited as
            // we reset to ground and a byte can always be processed at ground.
            self.advance(performer, byte);
        }
    }

    #[inline]
    fn process_end_utf8<P>(&mut self, performer: &mut P)
    where
        P: Perform,
    {
        let mut receiver = VtUtf8Receiver(performer, &mut self.state);
        let utf8_parser = &mut self.utf8_parser;
        utf8_parser.end(&mut receiver)
    }

    #[inline]
    fn perform_state_change<P>(&mut self, performer: &mut P, state: State, action: Action, byte: u8)
    where
        P: Perform,
    {
        macro_rules! maybe_action {
            ($action:expr, $arg:expr) => {
                match $action {
                    Action::None => (),
                    action => {
                        self.perform_action(performer, action, $arg);
                    },
                }
            };
        }

        match state {
            State::Anywhere => {
                // Just run the action
                self.perform_action(performer, action, byte);
            },
            state => {
                // Exit action for previous state
                let exit_action = self.state.exit_action();
                maybe_action!(exit_action, byte);

                // Assume the new state
                self.state = state;

                // Transition action
                maybe_action!(action, byte);

                // Entry action for new state
                maybe_action!(self.state.entry_action(), byte);
            },
        }
    }

    /// Separate method for osc_dispatch that borrows self as read-only
    ///
    /// The aliasing is needed here for multiple slices into self.osc_raw
    #[inline]
    fn osc_dispatch<P: Perform>(&self, performer: &mut P, byte: u8) {
        let mut slices: [MaybeUninit<&[u8]>; MAX_PARAMS] =
            unsafe { MaybeUninit::uninit().assume_init() };

        for (i, slice) in slices.iter_mut().enumerate().take(self.osc_num_params) {
            let indices = self.osc_params[i];
            *slice = MaybeUninit::new(&self.osc_raw[indices.0..indices.1]);
        }

        unsafe {
            let num_params = self.osc_num_params;
            let params = &slices[..num_params] as *const [MaybeUninit<&[u8]>] as *const [&[u8]];
            performer.osc_dispatch(&*params, byte == 0x07);
        }
    }

    /// Reset everything on ESC/CSI/DCS entry
    #[inline]
    fn clear(&mut self) {
        self.intermediate_idx = 0;
        self.ignoring = false;
        self.num_params = 0;
        self.param = 0;
    }

    #[inline]
    fn perform_action<P: Perform>(&mut self, performer: &mut P, action: Action, byte: u8) {
        match action {
            Action::Print => performer.print(byte as char),
            Action::Execute => performer.execute(byte),
            Action::Hook => {
                if self.num_params == MAX_PARAMS {
                    self.ignoring = true;
                } else {
                    self.params[self.num_params] = self.param;
                    self.num_params += 1;
                }

                performer.hook(self.params(), self.intermediates(), self.ignoring, byte as char);
            },
            Action::Put => performer.put(byte),
            Action::OscStart => {
                self.osc_raw.clear();
                self.osc_num_params = 0;
            },
            Action::OscPut => {
                #[cfg(feature = "no_std")]
                {
                    if self.osc_raw.is_full() {
                        return;
                    }
                }

                let idx = self.osc_raw.len();

                // Param separator
                if byte == b';' {
                    let param_idx = self.osc_num_params;
                    match param_idx {
                        // Only process up to MAX_PARAMS
                        MAX_PARAMS => return,

                        // First param is special - 0 to current byte index
                        0 => {
                            self.osc_params[param_idx] = (0, idx);
                        },

                        // All other params depend on previous indexing
                        _ => {
                            let prev = self.osc_params[param_idx - 1];
                            let begin = prev.1;
                            self.osc_params[param_idx] = (begin, idx);
                        },
                    }

                    self.osc_num_params += 1;
                } else {
                    self.osc_raw.push(byte);
                }
            },
            Action::OscEnd => {
                let param_idx = self.osc_num_params;
                let idx = self.osc_raw.len();

                match param_idx {
                    // Finish last parameter if not already maxed
                    MAX_PARAMS => (),

                    // First param is special - 0 to current byte index
                    0 => {
                        self.osc_params[param_idx] = (0, idx);
                        self.osc_num_params += 1;
                    },

                    // All other params depend on previous indexing
                    _ => {
                        let prev = self.osc_params[param_idx - 1];
                        let begin = prev.1;
                        self.osc_params[param_idx] = (begin, idx);
                        self.osc_num_params += 1;
                    },
                }
                self.osc_dispatch(performer, byte);
            },
            Action::Unhook => performer.unhook(),
            Action::CsiDispatch => {
                if self.num_params == MAX_PARAMS {
                    self.ignoring = true;
                } else {
                    self.params[self.num_params] = self.param;
                    self.num_params += 1;
                }

                performer.csi_dispatch(
                    self.params(),
                    self.intermediates(),
                    self.ignoring,
                    byte as char,
                );
            },
            Action::EscDispatch => {
                performer.esc_dispatch(self.intermediates(), self.ignoring, byte)
            },
            Action::None => (),
            Action::Collect => {
                if self.intermediate_idx == MAX_INTERMEDIATES {
                    self.ignoring = true;
                } else {
                    self.intermediates[self.intermediate_idx] = byte;
                    self.intermediate_idx += 1;
                }
            },
            Action::Param => {
                // Completed a param
                let idx = self.num_params;

                if idx == MAX_PARAMS {
                    self.ignoring = true;
                    return;
                }

                if byte == b';' {
                    self.params[idx] = self.param;
                    self.param = 0;
                    self.num_params += 1;
                } else {
                    // Continue collecting bytes into param
                    self.param = self.param.saturating_mul(10);
                    self.param = self.param.saturating_add((byte - b'0') as i64);
                }
            },
            Action::Clear => self.clear(),
            Action::BeginUtf8 => self.process_utf8(performer, byte),
            Action::CheckDcsSosPmApc => {
                if self.no_dcs_sos_pm_apc {
                    self.state = State::Escape;
                    self.clear();
                    performer.esc_dispatch(self.intermediates(), self.ignoring, byte);
                    self.state = State::Ground;
                }
            },
        }
    }
}

/// Performs actions requested by the Parser
///
/// Actions in this case mean, for example, handling a CSI escape sequence describing cursor
/// movement, or simply printing characters to the screen.
///
/// The methods on this type correspond to actions described in
/// http://vt100.net/emu/dec_ansi_parser. I've done my best to describe them in
/// a useful way in my own words for completeness, but the site should be
/// referenced if something isn't clear. If the site disappears at some point in
/// the future, consider checking archive.org.
pub trait Perform {
    /// Draw a character to the screen and update states.
    fn print(&mut self, _: char);

    /// Execute a C0 control function.
    fn execute(&mut self, byte: u8);

    /// Invoked when a final character arrives in first part of device control string.
    ///
    /// The control function should be determined from the private marker, final character, and
    /// execute with a parameter list. A handler should be selected for remaining characters in the
    /// string; the handler function should subsequently be called by `put` for every character in
    /// the control string.
    ///
    /// The `ignore` flag indicates that more than two intermediates arrived and
    /// subsequent characters were ignored.
    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, action: char);

    /// Pass bytes as part of a device control string to the handle chosen in `hook`. C0 controls
    /// will also be passed to the handler.
    fn put(&mut self, byte: u8);

    /// Called when a device control string is terminated.
    ///
    /// The previously selected handler should be notified that the DCS has
    /// terminated.
    fn unhook(&mut self);

    /// Dispatch an operating system command.
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool);

    /// A final character has arrived for a CSI sequence
    ///
    /// The `ignore` flag indicates that either more than two intermediates arrived
    /// or the number of parameters exceeded the maximum supported length,
    /// and subsequent characters were ignored.
    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, action: char);

    /// The final character of an escape sequence has arrived.
    ///
    /// The `ignore` flag indicates that more than two intermediates arrived and
    /// subsequent characters were ignored.
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8);
}

#[cfg(all(test, feature = "no_std"))]
#[macro_use]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    use core::i64;
    use std::string::String;
    use std::vec::Vec;

    static OSC_BYTES: &[u8] = &[
        0x1b, 0x5d, // Begin OSC
        b'2', b';', b'j', b'w', b'i', b'l', b'm', b'@', b'j', b'w', b'i', b'l', b'm', b'-', b'd',
        b'e', b's', b'k', b':', b' ', b'~', b'/', b'c', b'o', b'd', b'e', b'/', b'a', b'l', b'a',
        b'c', b'r', b'i', b't', b't', b'y', 0x07, // End OSC
    ];

    #[derive(Default)]
    struct OscDispatcher {
        dispatched_osc: bool,
        bell_terminated: bool,
        params: Vec<Vec<u8>>,
    }

    // All empty bodies except osc_dispatch
    impl Perform for OscDispatcher {
        fn print(&mut self, _: char) {}

        fn execute(&mut self, _: u8) {}

        fn hook(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn put(&mut self, _: u8) {}

        fn unhook(&mut self) {}

        fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
            // Set a flag so we know these assertions all run
            self.dispatched_osc = true;
            self.bell_terminated = bell_terminated;
            self.params = params.iter().map(|p| p.to_vec()).collect();
        }

        fn csi_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
    }

    #[derive(Default)]
    struct CsiDispatcher {
        dispatched_csi: bool,
        ignore: bool,
        params: Vec<i64>,
        intermediates: Vec<u8>,
    }

    impl Perform for CsiDispatcher {
        fn print(&mut self, _: char) {}

        fn execute(&mut self, _: u8) {}

        fn hook(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn put(&mut self, _: u8) {}

        fn unhook(&mut self) {}

        fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}

        fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, _: char) {
            self.intermediates = intermediates.to_vec();
            self.params = params.to_vec();
            self.ignore = ignore;
            self.dispatched_csi = true;
        }

        fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
    }

    #[derive(Default)]
    struct DcsDispatcher {
        dispatched_dcs: bool,
        intermediates: Vec<u8>,
        params: Vec<i64>,
        ignore: bool,
        c: Option<char>,
        s: Vec<u8>,
    }

    impl Perform for DcsDispatcher {
        fn print(&mut self, _: char) {}

        fn execute(&mut self, _: u8) {}

        fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
            self.intermediates = intermediates.to_vec();
            self.params = params.to_vec();
            self.ignore = ignore;
            self.c = Some(c);
            self.dispatched_dcs = true;
        }

        fn put(&mut self, byte: u8) {
            self.s.push(byte);
        }

        fn unhook(&mut self) {
            self.dispatched_dcs = true;
        }

        fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}

        fn csi_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
    }

    #[derive(Default)]
    struct EscDispatcher {
        dispatched_esc: bool,
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    }

    impl Perform for EscDispatcher {
        fn print(&mut self, _: char) {}

        fn execute(&mut self, _: u8) {}

        fn hook(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn put(&mut self, _: u8) {}

        fn unhook(&mut self) {}

        fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}

        fn csi_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
            self.intermediates = intermediates.to_vec();
            self.ignore = ignore;
            self.byte = byte;
            self.dispatched_esc = true;
        }
    }

    #[test]
    fn parse_osc() {
        let mut dispatcher = OscDispatcher::default();
        let mut parser = Parser::new();

        for byte in OSC_BYTES {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert!(dispatcher.dispatched_osc);
        assert_eq!(dispatcher.params.len(), 2);
        assert_eq!(dispatcher.params[0], &OSC_BYTES[2..3]);
        assert_eq!(dispatcher.params[1], &OSC_BYTES[4..(OSC_BYTES.len() - 1)]);
    }

    #[test]
    fn parse_empty_osc() {
        let mut dispatcher = OscDispatcher::default();
        let mut parser = Parser::new();

        for byte in &[0x1b, 0x5d, 0x07] {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert!(dispatcher.dispatched_osc);
    }

    #[test]
    fn parse_osc_max_params() {
        static INPUT: &[u8] = b"\x1b];;;;;;;;;;;;;;;;;\x1b";
        let mut dispatcher = OscDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert!(dispatcher.dispatched_osc);
        assert_eq!(dispatcher.params.len(), MAX_PARAMS);
        for param in dispatcher.params.iter() {
            assert_eq!(param.len(), 0);
        }
    }

    #[test]
    fn parse_dcs_max_params() {
        static INPUT: &[u8] = b"\x1bP1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;p\x1b";
        let mut dispatcher = DcsDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert!(dispatcher.ignore);
        assert!(dispatcher.dispatched_dcs);
        assert_eq!(dispatcher.params.len(), MAX_PARAMS);
        for param in dispatcher.params.iter() {
            assert_eq!(*param, 1);
        }
    }

    #[test]
    fn osc_bell_terminated() {
        static INPUT: &[u8] = b"\x1b]11;ff/00/ff\x07";
        let mut dispatcher = OscDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert!(dispatcher.dispatched_osc);
        assert!(dispatcher.bell_terminated);
    }

    #[test]
    fn osc_c0_st_terminated() {
        static INPUT: &[u8] = b"\x1b]11;ff/00/ff\x1b\\";
        let mut dispatcher = OscDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert!(dispatcher.dispatched_osc);
        assert!(!dispatcher.bell_terminated);
    }

    #[test]
    fn parse_csi_max_params() {
        // This will build a list of repeating '1;'s
        // The length is MAX_PARAMS - 1 because the last semicolon is interpreted
        // as an implicit zero, making the total number of parameters MAX_PARAMS
        let params = std::iter::repeat("1;").take(MAX_PARAMS - 1).collect::<String>();
        let input = format!("\x1b[{}p", &params[..]).into_bytes();

        let mut dispatcher = CsiDispatcher::default();
        let mut parser = Parser::new();

        for byte in input {
            parser.advance(&mut dispatcher, byte);
        }

        // Check that flag is set and thus csi_dispatch assertions ran.
        assert!(dispatcher.dispatched_csi);
        assert_eq!(dispatcher.params.len(), MAX_PARAMS);
        assert!(!dispatcher.ignore);
    }

    #[test]
    fn parse_csi_params_ignore_long_params() {
        // This will build a list of repeating '1;'s
        // The length is MAX_PARAMS because the last semicolon is interpreted
        // as an implicit zero, making the total number of parameters MAX_PARAMS + 1
        let params = std::iter::repeat("1;").take(MAX_PARAMS).collect::<String>();
        let input = format!("\x1b[{}p", &params[..]).into_bytes();

        let mut dispatcher = CsiDispatcher::default();
        let mut parser = Parser::new();

        for byte in input {
            parser.advance(&mut dispatcher, byte);
        }

        // Check that flag is set and thus csi_dispatch assertions ran.
        assert!(dispatcher.dispatched_csi);
        assert_eq!(dispatcher.params.len(), MAX_PARAMS);
        assert!(dispatcher.ignore);
    }

    #[test]
    fn parse_csi_params_trailing_semicolon() {
        let mut dispatcher = CsiDispatcher::default();
        let mut parser = Parser::new();

        for byte in b"\x1b[4;m" {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.params, &[4, 0]);
    }

    #[test]
    fn parse_semi_set_underline() {
        // Create dispatcher and check state
        let mut dispatcher = CsiDispatcher::default();
        let mut parser = Parser::new();

        for byte in b"\x1b[;4m" {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert_eq!(dispatcher.params, &[0, 4]);
    }

    #[test]
    fn parse_long_csi_param() {
        // The important part is the parameter, which is (i64::MAX + 1)
        static INPUT: &[u8] = b"\x1b[9223372036854775808m";
        let mut dispatcher = CsiDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.params, &[i64::MAX as i64]);
    }

    #[test]
    fn csi_reset() {
        static INPUT: &[u8] = b"\x1b[3;1\x1b[?1049h";
        let mut dispatcher = CsiDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert!(dispatcher.dispatched_csi);
        assert!(!dispatcher.ignore);
        assert_eq!(dispatcher.intermediates, &[b'?']);
        assert_eq!(dispatcher.params, &[1049]);
    }

    #[test]
    fn dcs_reset() {
        static INPUT: &[u8] = b"\x1b[3;1\x1bP1$tx\x9c";
        let mut dispatcher = DcsDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert!(dispatcher.dispatched_dcs);
        assert!(!dispatcher.ignore);
        assert_eq!(dispatcher.intermediates, &[b'$']);
        assert_eq!(dispatcher.params, &[1]);
    }

    #[test]
    fn esc_reset() {
        static INPUT: &[u8] = b"\x1b[3;1\x1b(A";
        let mut dispatcher = EscDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert!(dispatcher.dispatched_esc);
        assert!(!dispatcher.ignore);
        assert_eq!(dispatcher.intermediates, &[b'(']);
    }

    #[test]
    fn parse_osc_with_utf8_arguments() {
        static INPUT: &[u8] = &[
            0x0d, 0x1b, 0x5d, 0x32, 0x3b, 0x65, 0x63, 0x68, 0x6f, 0x20, 0x27, 0xc2, 0xaf, 0x5c,
            0x5f, 0x28, 0xe3, 0x83, 0x84, 0x29, 0x5f, 0x2f, 0xc2, 0xaf, 0x27, 0x20, 0x26, 0x26,
            0x20, 0x73, 0x6c, 0x65, 0x65, 0x70, 0x20, 0x31, 0x07,
        ];
        let mut dispatcher = OscDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert_eq!(dispatcher.params[0], &[b'2']);
        assert_eq!(dispatcher.params[1], &INPUT[5..(INPUT.len() - 1)]);
    }

    #[test]
    fn osc_containing_string_terminator() {
        static INPUT: &[u8] = b"\x1b]2;\xe6\x9c\xab\x1b\\";
        let mut dispatcher = OscDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.params[1], &INPUT[4..(INPUT.len() - 2)]);
    }

    #[test]
    fn parse_dcs() {
        static INPUT: &[u8] =
            &[0x1b, 0x50, 0x30, 0x3b, 0x31, 0x7c, 0x31, 0x37, 0x2f, 0x61, 0x62, 0x9c];
        let mut dispatcher = DcsDispatcher::default();
        let mut parser = Parser::new();

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert!(dispatcher.dispatched_dcs);
        assert_eq!(dispatcher.params, vec![0, 1]);
        assert_eq!(dispatcher.c, Some('|'));
        assert_eq!(dispatcher.s, b"17/ab".to_vec());
    }

    #[test]
    fn dcs_disabled() {
        static INPUT: &[u8] =
            &[0x1b, 0x50, 0x30, 0x3b, 0x31, 0x7c, 0x31, 0x37, 0x2f, 0x61, 0x62, 0x9c];
        let mut dispatcher = EscDispatcher::default();
        let mut parser = Parser::new();
        parser.set_dcs_sos_pm_apc(false);

        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert!(dispatcher.dispatched_esc);
        assert!(dispatcher.intermediates.is_empty());
        assert!(!dispatcher.ignore);
        assert_eq!(dispatcher.byte, 0x50);
    }

    #[test]
    fn exceed_max_buffer_size() {
        static NUM_BYTES: usize = MAX_OSC_RAW + 100;
        static INPUT_START: &[u8] = &[0x1b, b']', b'5', b'2', b';', b's'];
        static INPUT_END: &[u8] = &[b'\x07'];

        let mut dispatcher = OscDispatcher::default();
        let mut parser = Parser::new();

        // Create valid OSC escape
        for byte in INPUT_START {
            parser.advance(&mut dispatcher, *byte);
        }

        // Exceed max buffer size
        for _ in 0..NUM_BYTES {
            parser.advance(&mut dispatcher, b'a');
        }

        // Terminate escape for dispatch
        for byte in INPUT_END {
            parser.advance(&mut dispatcher, *byte);
        }

        assert!(dispatcher.dispatched_osc);

        assert_eq!(dispatcher.params.len(), 2);
        assert_eq!(dispatcher.params[0], b"52");

        #[cfg(not(feature = "no_std"))]
        assert_eq!(dispatcher.params[1].len(), NUM_BYTES + INPUT_END.len());

        #[cfg(feature = "no_std")]
        assert_eq!(dispatcher.params[1].len(), MAX_OSC_RAW - dispatcher.params[0].len());
    }

    #[derive(Default)]
    struct InvalidUtf8ByteDispatcher {
        num_invalid: u8,
    }

    impl Perform for InvalidUtf8ByteDispatcher {
        fn print(&mut self, c: char) {
            assert_eq!(c, '�');
            self.num_invalid += 1;
        }

        fn execute(&mut self, _: u8) {}

        fn hook(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn put(&mut self, _: u8) {}

        fn unhook(&mut self) {}

        fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}

        fn csi_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
    }

    #[test]
    fn parse_invalid_utf8_byte() {
        let mut dispatcher = InvalidUtf8ByteDispatcher::default();
        let mut parser = Parser::new();

        for byte in 0x80..0xc2 {
            parser.advance(&mut dispatcher, byte);
        }
        for byte in 0xf5..=0xff {
            parser.advance(&mut dispatcher, byte);
        }

        // Continuation bytes, overlong bytes, invalid code points, invalid code units.
        assert_eq!(dispatcher.num_invalid, 64 + 2 + 9 + 2);
    }

    #[derive(Default)]
    struct PrintDispatcher {
        printed: String,
    }

    impl Perform for PrintDispatcher {
        fn print(&mut self, c: char) {
            assert!(c.is_whitespace() || !c.is_ascii_control() || c == '\u{007f}');
            self.printed.push(c);
        }

        fn execute(&mut self, b: u8) {
            assert!(b.is_ascii_control() && b != b'\x7f');
            self.printed.push(b as char)
        }

        fn hook(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn put(&mut self, _: u8) {}

        fn unhook(&mut self) {}

        fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}

        fn csi_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: char) {}

        fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
    }

    fn test_print(bytes: &[u8], expected: &str) {
        // Double-check that the expected bytes match `String::from_utf8_lossy`.
        #[cfg(not(feature = "no_std"))]
        assert_eq!(expected, String::from_utf8_lossy(bytes), "input bytes: {:#x?}", bytes);

        let mut dispatcher = PrintDispatcher::default();
        let mut parser = Parser::new();

        for byte in bytes {
            parser.advance(&mut dispatcher, *byte);
        }
        parser.end(&mut dispatcher);

        assert_eq!(dispatcher.printed, expected, "input bytes: {:#x?}", bytes);
    }

    #[test]
    fn parse_misc_invalid_utf8() {
        test_print(b"\xc2A\xe1\x80B\xf1\x80\x80C", "�A�B�C");
        test_print(b"\xf4\x90", "��");
        test_print(b"\xed\xa0", "��");
        test_print(b"\xc2\xc3\x20", "�� ");
        test_print(b"\x80\xc2\xc2.\xc0\x80", "���.��");
        test_print(b"\x90\xfd\xfe\xff\x80", "�����");
        test_print(b"\x10\x75\xf8\xc4\x19\x04", "\u{10}u��\u{19}\u{4}");
        test_print(
            b"\x22\x6e\x35\x3d\x84\x34\x25\xe5\x2d\x49\xf6\x4e\xce\xfa\x06\xb3",
            "\"n5=�4%�-I�N��\u{6}�",
        );
        test_print(b"\xfe\x19\xdb\xf5", "�\u{19}��");
        test_print(b"\xfe\x19\xdb\xf5", "�\u{19}��");
        test_print(b"\x80\xc2\x80\x9f\xc2\x9f", "�\u{80}�\u{9f}");
        test_print(b"\xc2\x18\xc2\x00\xc2\x1a", "�\u{18}�\u{0}�\u{1a}");
        test_print(b"\xdd\xdd\xfa\x2a\x47\xd9\xd8\x9b\x9c\x97\xa1\x9a\x9b", "���*G�؛�����");
        test_print(
            b"\x9b\x9c\x97\xa1\x9a\x9b\xfd\x44\xaa\x14\x52\x5f\x33\x5f\x22\x6e\x35\x3d\x84\x34",
            "�������D�\u{14}R_3_\"n5=�4",
        );
    }

    // Tests derived from https://hsivonen.fi/broken-utf-8/test.html
    // See https://hsivonen.fi/broken-utf-8/ for discussion.
    #[test]
    fn how_many_replacement_characters() {
        test_print(b"\xc0\x80", "��");
        test_print(b"\xe0\x80\x80", "���");
        test_print(b"\xf0\x80\x80\x80", "����");
        test_print(b"\xf8\x80\x80\x80\x80", "�����");
        test_print(b"\xfc\x80\x80\x80\x80\x80", "������");
        test_print(b"\xc1\xbf", "��");
        test_print(b"\xe0\x81\xbf", "���");
        test_print(b"\xf0\x80\x81\xbf", "����");
        test_print(b"\xf8\x80\x80\x81\xbf", "�����");
        test_print(b"\xfc\x80\x80\x80\x81\xbf", "������");
        test_print(b"\xe0\x82\x80", "���");
        test_print(b"\xf0\x80\x82\x80", "����");
        test_print(b"\xf8\x80\x80\x82\x80", "�����");
        test_print(b"\xfc\x80\x80\x80\x82\x80", "������");
        test_print(b"\xe0\x9f\xbf", "���");
        test_print(b"\xf0\x80\x9f\xbf", "����");
        test_print(b"\xf8\x80\x80\x9f\xbf", "�����");
        test_print(b"\xfc\x80\x80\x80\x9f\xbf", "������");
        test_print(b"\xf0\x80\xa0\x80", "����");
        test_print(b"\xf8\x80\x80\xa0\x80", "�����");
        test_print(b"\xfc\x80\x80\x80\xa0\x80", "������");
        test_print(b"\xf0\x8f\xbf\xbf", "����");
        test_print(b"\xf8\x80\x8f\xbf\xbf", "�����");
        test_print(b"\xfc\x80\x80\x8f\xbf\xbf", "������");
        test_print(b"\xf8\x80\x90\x80\x80", "�����");
        test_print(b"\xfc\x80\x80\x90\x80\x80", "������");
        test_print(b"\xf8\x84\x8f\xbf\xbf", "�����");
        test_print(b"\xfc\x80\x84\x8f\xbf\xbf", "������");
        test_print(b"\xf4\x90\x80\x80", "����");
        test_print(b"\xfb\xbf\xbf\xbf\xbf", "�����");
        test_print(b"\xfd\xbf\xbf\xbf\xbf\xbf", "������");
        test_print(b"\xed\xa0\x80", "���");
        test_print(b"\xed\xbf\xbf", "���");
        test_print(b"\xed\xa0\xbd\xed\xb2\xa9", "������");
        test_print(b"\xf8\x84\x90\x80\x80", "�����");
        test_print(b"\xfc\x80\x84\x90\x80\x80", "������");
        test_print(b"\xf0\x8d\xa0\x80", "����");
        test_print(b"\xf0\x8d\xbf\xbf", "����");
        test_print(b"\xf0\x8d\xa0\xbd\xf0\x8d\xb2\xa9", "��������");
        test_print(b"\x80", "�");
        test_print(b"\x80\x80", "��");
        test_print(b"\x80\x80\x80", "���");
        test_print(b"\x80\x80\x80\x80", "����");
        test_print(b"\x80\x80\x80\x80\x80", "�����");
        test_print(b"\x80\x80\x80\x80\x80\x80", "������");
        test_print(b"\x80\x80\x80\x80\x80\x80\x80", "�������");
        test_print(b"\xc2\xb6\x80", "¶�");
        test_print(b"\xe2\x98\x83\x80", "☃�");
        test_print(b"\xf0\x9f\x92\xa9\x80", "💩�");
        test_print(b"\xfb\xbf\xbf\xbf\xbf\x80", "������");
        test_print(b"\xfd\xbf\xbf\xbf\xbf\xbf\x80", "�������");
        test_print(b"\xc2", "�");
        test_print(b"\xe2", "�");
        test_print(b"\xe2\x98", "�");
        test_print(b"\xf0", "�");
        test_print(b"\xf0\x9f", "�");
        test_print(b"\xf0\x9f\x92", "�");
        test_print(b"\xfe", "�");
        test_print(b"\xfe\x80", "��");
        test_print(b"\xff", "�");
        test_print(b"\xff\x80", "��");
    }
}

#[cfg(all(feature = "nightly", test))]
mod bench {
    extern crate std;
    extern crate test;

    use super::*;

    use test::{black_box, Bencher};

    static VTE_DEMO: &[u8] = include_bytes!("../tests/demo.vte");

    struct BenchDispatcher;
    impl Perform for BenchDispatcher {
        fn print(&mut self, c: char) {
            black_box(c);
        }

        fn execute(&mut self, byte: u8) {
            black_box(byte);
        }

        fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
            black_box((params, intermediates, ignore, c));
        }

        fn put(&mut self, byte: u8) {
            black_box(byte);
        }

        fn unhook(&mut self) {}

        fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
            black_box((params, bell_terminated));
        }

        fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
            black_box((params, intermediates, ignore, c));
        }

        fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
            black_box((intermediates, ignore, byte));
        }
    }

    #[bench]
    fn testfile(b: &mut Bencher) {
        b.iter(|| {
            let mut dispatcher = BenchDispatcher;
            let mut parser = Parser::new();

            for byte in VTE_DEMO {
                parser.advance(&mut dispatcher, *byte);
            }
        });
    }
}
