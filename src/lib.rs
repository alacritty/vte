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
//! * Only supports 7-bit codes. Some 8-bit codes are still supported, but they
//!   no longer work in all states.
//!
//! [`Parser`]: struct.Parser.html
//! [`Perform`]: trait.Perform.html
//! [Paul Williams' ANSI parser state machine]: https://vt100.net/emu/dec_ansi_parser
#![cfg_attr(feature = "no_std", no_std)]

#[cfg(feature = "no_std")]
extern crate arrayvec;
#[cfg(not(feature = "no_std"))]
extern crate core;
extern crate utf8parse as utf8;

use core::mem::{self, MaybeUninit};

#[cfg(feature = "no_std")]
use arrayvec::ArrayVec;

mod definitions;
mod table;

use definitions::{unpack, Action, State};

use table::{ENTRY_ACTIONS, EXIT_ACTIONS, STATE_CHANGE};

impl State {
    /// Get exit action for this state
    #[inline(always)]
    pub fn exit_action(&self) -> Action {
        unsafe { *EXIT_ACTIONS.get_unchecked(*self as usize) }
    }

    /// Get entry action for this state
    #[inline(always)]
    pub fn entry_action(&self) -> Action {
        unsafe { *ENTRY_ACTIONS.get_unchecked(*self as usize) }
    }
}

const MAX_INTERMEDIATES: usize = 2;
#[cfg(any(feature = "no_std", test))]
const MAX_OSC_RAW: usize = 1024;
const MAX_PARAMS: usize = 16;

struct VtUtf8Receiver<'a, P: Perform + 'a>(&'a mut P, &'a mut State);

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
}

impl Parser {
    /// Create a new Parser
    pub fn new() -> Parser {
        Parser {
            state: State::Ground,
            intermediates: [0u8; MAX_INTERMEDIATES],
            intermediate_idx: 0,
            params: [0i64; MAX_PARAMS],
            param: 0,
            num_params: 0,
            #[cfg(feature = "no_std")]
            osc_raw: ArrayVec::new(),
            #[cfg(not(feature = "no_std"))]
            osc_raw: Vec::new(),
            osc_params: [(0, 0); MAX_PARAMS],
            osc_num_params: 0,
            ignoring: false,
            utf8_parser: utf8::Parser::new(),
        }
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
        let mut change = STATE_CHANGE[State::Anywhere as usize][byte as usize];

        if change == 0 {
            change = STATE_CHANGE[self.state as usize][byte as usize];
        }

        // Unpack into a state and action
        let (state, action) = unpack(change);

        self.perform_state_change(performer, state, action, byte);
    }

    #[inline]
    fn process_utf8<P>(&mut self, performer: &mut P, byte: u8)
    where
        P: Perform,
    {
        let mut receiver = VtUtf8Receiver(performer, &mut self.state);
        let utf8_parser = &mut self.utf8_parser;
        utf8_parser.advance(&mut receiver, byte);
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
                    }
                }
            };
        }

        match state {
            State::Anywhere => {
                // Just run the action
                self.perform_action(performer, action, byte);
            }
            state => {
                // Exit action for previous state
                let exit_action = self.state.exit_action();
                maybe_action!(exit_action, byte);

                // Transition action
                maybe_action!(action, byte);

                // Entry action for new state
                maybe_action!(state.entry_action(), byte);

                // Assume the new state
                self.state = state;
            }
        }
    }

    /// Separate method for osc_dispatch that borrows self as read-only
    ///
    /// The aliasing is needed here for multiple slices into self.osc_raw
    #[inline]
    fn osc_dispatch<P: Perform>(&self, performer: &mut P) {
        let mut slices: [MaybeUninit<&[u8]>; MAX_PARAMS] =
            unsafe { MaybeUninit::uninit().assume_init() };

        for i in 0..self.osc_num_params {
            let indices = self.osc_params[i];
            slices[i] = MaybeUninit::new(&self.osc_raw[indices.0..indices.1]);
        }

        unsafe {
            performer.osc_dispatch(mem::transmute::<_, &[&[u8]]>(
                &slices[..self.osc_num_params],
            ));
        }
    }

    #[inline]
    fn perform_action<P: Perform>(&mut self, performer: &mut P, action: Action, byte: u8) {
        match action {
            Action::Print => performer.print(byte as char),
            Action::Execute => performer.execute(byte),
            Action::Hook => {
                self.params[self.num_params] = self.param;
                self.num_params += 1;

                performer.hook(
                    self.params(),
                    self.intermediates(),
                    self.ignoring,
                    byte as char,
                );
            }
            Action::Put => performer.put(byte),
            Action::OscStart => {
                self.osc_raw.clear();
                self.osc_num_params = 0;
            }
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
                        }

                        // All other params depend on previous indexing
                        _ => {
                            let prev = self.osc_params[param_idx - 1];
                            let begin = prev.1;
                            self.osc_params[param_idx] = (begin, idx);
                        }
                    }

                    self.osc_num_params += 1;
                } else {
                    self.osc_raw.push(byte);
                }
            }
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
                    }

                    // All other params depend on previous indexing
                    _ => {
                        let prev = self.osc_params[param_idx - 1];
                        let begin = prev.1;
                        self.osc_params[param_idx] = (begin, idx);
                        self.osc_num_params += 1;
                    }
                }
                self.osc_dispatch(performer);
            }
            Action::Unhook => performer.unhook(),
            Action::CsiDispatch => {
                self.params[self.num_params] = self.param;
                self.num_params += 1;

                performer.csi_dispatch(
                    self.params(),
                    self.intermediates(),
                    self.ignoring,
                    byte as char,
                );

                self.num_params = 0;
                self.param = 0;
            }
            Action::EscDispatch => {
                performer.esc_dispatch(self.params(), self.intermediates(), self.ignoring, byte);
            }
            Action::Ignore | Action::None => (),
            Action::Collect => {
                if self.intermediate_idx == MAX_INTERMEDIATES {
                    self.ignoring = true;
                } else {
                    self.intermediates[self.intermediate_idx] = byte;
                    self.intermediate_idx += 1;
                }
            }
            Action::Param => {
                if byte == b';' {
                    // Completed a param
                    let idx = self.num_params;

                    if idx == MAX_PARAMS - 1 {
                        return;
                    }

                    self.params[idx] = self.param;
                    self.param = 0;
                    self.num_params += 1;
                } else {
                    // Continue collecting bytes into param
                    self.param = self.param.saturating_mul(10);
                    self.param = self.param.saturating_add((byte - b'0') as i64);
                }
            }
            Action::Clear => {
                self.intermediate_idx = 0;
                self.num_params = 0;
                self.ignoring = false;
            }
            Action::BeginUtf8 => {
                self.process_utf8(performer, byte);
            }
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
    /// Draw a character to the screen and update states
    fn print(&mut self, char);

    /// Execute a C0 or C1 control function
    fn execute(&mut self, byte: u8);

    /// Invoked when a final character arrives in first part of device control string
    ///
    /// The control function should be determined from the private marker, final character, and
    /// execute with a parameter list. A handler should be selected for remaining characters in the
    /// string; the handler function should subsequently be called by `put` for every character in
    /// the control string.
    ///
    /// The `ignore` flag indicates that more than two intermediates arrived and
    /// subsequent characters were ignored.
    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, char);

    /// Pass bytes as part of a device control string to the handle chosen in `hook`. C0 controls
    /// will also be passed to the handler.
    fn put(&mut self, byte: u8);

    /// Called when a device control string is terminated
    ///
    /// The previously selected handler should be notified that the DCS has
    /// terminated.
    fn unhook(&mut self);

    /// Dispatch an operating system command
    fn osc_dispatch(&mut self, params: &[&[u8]]);

    /// A final character has arrived for a CSI sequence
    ///
    /// The `ignore` flag indicates that more than two intermediates arrived and
    /// subsequent characters were ignored.
    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, char);

    /// The final character of an escape sequence has arrived.
    ///
    /// The `ignore` flag indicates that more than two intermediates arrived and
    /// subsequent characters were ignored.
    fn esc_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, byte: u8);
}

#[cfg(all(test, feature = "no_std"))]
#[macro_use]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    use std::vec::Vec;
    use core::i64;

    static OSC_BYTES: &'static [u8] = &[
        0x1b, 0x5d, // Begin OSC
        b'2', b';', b'j', b'w', b'i', b'l', b'm', b'@', b'j', b'w', b'i', b'l', b'm', b'-', b'd',
        b'e', b's', b'k', b':', b' ', b'~', b'/', b'c', b'o', b'd', b'e', b'/', b'a', b'l', b'a',
        b'c', b'r', b'i', b't', b't', b'y', 0x07, // End OSC
    ];

    #[derive(Default)]
    struct OscDispatcher {
        dispatched_osc: bool,
        params: Vec<Vec<u8>>,
    }

    // All empty bodies except osc_dispatch
    impl Perform for OscDispatcher {
        fn print(&mut self, _: char) {}
        fn execute(&mut self, _byte: u8) {}
        fn hook(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool, _: char) {}
        fn put(&mut self, _byte: u8) {}
        fn unhook(&mut self) {}
        fn osc_dispatch(&mut self, params: &[&[u8]]) {
            // Set a flag so we know these assertions all run
            self.dispatched_osc = true;
            self.params = params.iter().map(|p| p.to_vec()).collect();
        }
        fn csi_dispatch(
            &mut self,
            _params: &[i64],
            _intermediates: &[u8],
            _ignore: bool,
            _c: char,
        ) {
        }
        fn esc_dispatch(
            &mut self,
            _params: &[i64],
            _intermediates: &[u8],
            _ignore: bool,
            _byte: u8,
        ) {
        }
    }

    #[derive(Default)]
    struct CsiDispatcher {
        dispatched_csi: bool,
        params: Vec<Vec<i64>>,
    }

    impl Perform for CsiDispatcher {
        fn print(&mut self, _: char) {}
        fn execute(&mut self, _byte: u8) {}
        fn hook(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool, _: char) {}
        fn put(&mut self, _byte: u8) {}
        fn unhook(&mut self) {}
        fn osc_dispatch(&mut self, _params: &[&[u8]]) {}
        fn csi_dispatch(&mut self, params: &[i64], _intermediates: &[u8], _ignore: bool, _c: char) {
            self.dispatched_csi = true;
            self.params.push(params.to_vec());
        }
        fn esc_dispatch(
            &mut self,
            _params: &[i64],
            _intermediates: &[u8],
            _ignore: bool,
            _byte: u8,
        ) {
        }
    }

    #[derive(Default)]
    struct DcsDispatcher {
        dispatched_dcs: bool,
        params: Vec<i64>,
        c: Option<char>,
        s: Vec<u8>,
    }

    impl Perform for DcsDispatcher {
        fn print(&mut self, _: char) {}
        fn execute(&mut self, _byte: u8) {}
        fn hook(&mut self, params: &[i64], _intermediates: &[u8], _ignore: bool, c: char) {
            self.c = Some(c);
            self.params = params.to_vec();
        }
        fn put(&mut self, byte: u8) {
            self.s.push(byte);
        }
        fn unhook(&mut self) {
            self.dispatched_dcs = true;
        }
        fn osc_dispatch(&mut self, _params: &[&[u8]]) {}
        fn csi_dispatch(
            &mut self,
            _params: &[i64],
            _intermediates: &[u8],
            _ignore: bool,
            _c: char,
        ) {
        }
        fn esc_dispatch(
            &mut self,
            _params: &[i64],
            _intermediates: &[u8],
            _ignore: bool,
            _byte: u8,
        ) {
        }
    }

    #[test]
    fn parse_osc() {
        // Create dispatcher and check state
        let mut dispatcher = OscDispatcher::default();
        assert_eq!(dispatcher.dispatched_osc, false);

        // Run parser using OSC_BYTES
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
        // Create dispatcher and check state
        let mut dispatcher = OscDispatcher::default();
        assert_eq!(dispatcher.dispatched_osc, false);

        // Run parser using OSC_BYTES
        let mut parser = Parser::new();
        for byte in &[0x1b, 0x5d, 0x07] {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert!(dispatcher.dispatched_osc);
    }

    #[test]
    fn parse_osc_max_params() {
        use MAX_PARAMS;

        static INPUT: &'static [u8] = b"\x1b];;;;;;;;;;;;;;;;;\x1b";

        // Create dispatcher and check state
        let mut dispatcher = OscDispatcher::default();
        assert_eq!(dispatcher.dispatched_osc, false);

        // Run parser using OSC_BYTES
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
    fn parse_csi_max_params() {
        use MAX_PARAMS;

        static INPUT: &'static [u8] = b"\x1b[1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;1;p";

        // Create dispatcher and check state
        let mut dispatcher = CsiDispatcher::default();
        assert!(!dispatcher.dispatched_csi);

        // Run parser using OSC_BYTES
        let mut parser = Parser::new();
        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus csi_dispatch assertions ran.
        assert!(dispatcher.dispatched_csi);
        assert_eq!(dispatcher.params.len(), 1);
        assert_eq!(dispatcher.params[0].len(), MAX_PARAMS);
    }

    #[test]
    fn parse_csi_params_trailing_semicolon() {
        let mut dispatcher = CsiDispatcher::default();
        let mut parser = Parser::new();

        for byte in b"\x1b[4;m" {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.params.len(), 1);
        assert_eq!(dispatcher.params[0], &[4, 0]);
    }

    #[test]
    fn parse_semi_set_underline() {
        // Create dispatcher and check state
        let mut dispatcher = CsiDispatcher::default();

        // Run parser using OSC_BYTES
        let mut parser = Parser::new();
        for byte in b"\x1b[;4m" {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert_eq!(dispatcher.params[0], &[0, 4]);
    }

    #[test]
    fn parse_long_csi_param() {
        // The important part is the parameter, which is (i64::MAX + 1)
        static INPUT: &'static [u8] = b"\x1b[9223372036854775808m";

        let mut dispatcher = CsiDispatcher::default();

        let mut parser = Parser::new();
        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        assert_eq!(dispatcher.params[0], &[i64::MAX as i64]);
    }

    #[test]
    fn parse_osc_with_utf8_arguments() {
        static INPUT: &'static [u8] = &[
            0x0d, 0x1b, 0x5d, 0x32, 0x3b, 0x65, 0x63, 0x68, 0x6f, 0x20, 0x27, 0xc2, 0xaf, 0x5c,
            0x5f, 0x28, 0xe3, 0x83, 0x84, 0x29, 0x5f, 0x2f, 0xc2, 0xaf, 0x27, 0x20, 0x26, 0x26,
            0x20, 0x73, 0x6c, 0x65, 0x65, 0x70, 0x20, 0x31, 0x07,
        ];

        // Create dispatcher and check state
        let mut dispatcher = OscDispatcher {
            params: vec![],
            dispatched_osc: false,
        };

        // Run parser using OSC_BYTES
        let mut parser = Parser::new();
        for byte in INPUT {
            parser.advance(&mut dispatcher, *byte);
        }

        // Check that flag is set and thus osc_dispatch assertions ran.
        assert_eq!(dispatcher.params[0], &[b'2']);
        assert_eq!(dispatcher.params[1], &INPUT[5..(INPUT.len() - 1)]);
    }

    #[test]
    fn parse_dcs() {
        static INPUT: &'static [u8] = &[
            0x1b, 0x50, 0x30, 0x3b, 0x31, 0x7c, 0x31, 0x37, 0x2f, 0x61, 0x62, 0x9c,
        ];

        // Create dispatcher and check state
        let mut dispatcher = DcsDispatcher::default();

        // Run parser using OSC_BYTES
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
    fn exceed_max_buffer_size() {
        static NUM_BYTES: usize = MAX_OSC_RAW + 100;
        static INPUT_START: &'static [u8] = &[
            0x1b, b']', b'5', b'2', b';', b's'
        ];
        static INPUT_END: &'static [u8] = &[b'\x07'];

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
}
