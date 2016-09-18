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
//! ```ignore
//! cargo build --release --example parselog
//! vim | target/release/examples/parselog
//! ```
//!
//! Just type `:q` to exit.
//!
//! [`Parser`]: struct.Parser.html
//! [`Perform`]: trait.Perform.html
//! [Paul Williams' ANSI parser state machine]: http://vt100.net/emu/dec_ansi_parser
extern crate utf8parse as utf8;

mod table;
mod definitions;

use definitions::{Action, State, unpack};

use table::{EXIT_ACTIONS, ENTRY_ACTIONS, STATE_CHANGE};

impl State {
    /// Get exit action for this state
    #[inline(always)]
    pub fn exit_action(&self) -> Action {
        unsafe {
            *EXIT_ACTIONS.get_unchecked(*self as usize)
        }
    }

    /// Get entry action for this state
    #[inline(always)]
    pub fn entry_action(&self) -> Action {
        unsafe {
            *ENTRY_ACTIONS.get_unchecked(*self as usize)
        }
    }
}


const MAX_INTERMEDIATES: usize = 2;
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
    num_params: usize,
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
            num_params: 0,
            ignoring: false,
            utf8_parser: utf8::Parser::new(),
        }
    }

    fn params(&self) -> &[i64] {
        &self.params[..self.num_params]
    }

    fn intermediates(&self) -> &[u8] {
        &self.intermediates[..self.intermediate_idx]
    }

    /// Advance the parser state
    ///
    /// Requires a [`Perform`] in case `byte` triggers an action
    ///
    /// [`Perform`]: trait.Perform.html
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
        where P: Perform
    {
        let mut receiver = VtUtf8Receiver(performer, &mut self.state);
        let utf8_parser = &mut self.utf8_parser;
        utf8_parser.advance(&mut receiver, byte);
    }

    fn perform_state_change<P>(&mut self, performer: &mut P, state: State, action: Action, byte: u8)
        where P: Perform
    {
        macro_rules! maybe_action {
            ($action:expr, $arg:expr) => {
                match $action {
                    Action::None => (),
                    action => {
                        self.perform_action(performer, action, $arg);
                    },
                }
            }
        }

        match state {
            State::Anywhere => {
                // Just run the action
                self.perform_action(performer, action, byte);
            },
            state => {
                // Exit action for previous state
                let exit_action = self.state.exit_action();
                maybe_action!(exit_action, 0);

                // Transition action
                maybe_action!(action, byte);

                // Entry action for new state
                maybe_action!(state.entry_action(), 0);

                // Assume the new state
                self.state = state;
            }
        }
    }

    fn perform_action<P: Perform>(&mut self, performer: &mut P, action: Action, byte: u8) {
        match action {
            Action::Print => performer.print(byte as char),
            Action::Execute => performer.execute(byte),
            Action::Hook => {
                performer.hook(
                    self.params(),
                    self.intermediates(),
                    self.ignoring,
                    byte
                );
            },
            Action::Put => performer.put(byte),
            Action::OscStart => performer.osc_start(),
            Action::OscPut => performer.osc_put(byte),
            Action::OscEnd => performer.osc_end(byte),
            Action::Unhook => performer.unhook(byte),
            Action::CsiDispatch => {
                performer.csi_dispatch(
                    self.params(),
                    self.intermediates(),
                    self.ignoring,
                    byte as char
                );
            }
            Action::EscDispatch => {
                performer.esc_dispatch(
                    self.params(),
                    self.intermediates(),
                    self.ignoring,
                    byte
                );
            },
            Action::Ignore | Action::None => (),
            Action::Collect => {
                if self.intermediate_idx == MAX_INTERMEDIATES {
                    self.ignoring = true;
                } else {
                    self.intermediates[self.intermediate_idx] = byte;
                    self.intermediate_idx += 1;
                }
            },
            Action::Param => {
                if byte == b';' {
                    // end of param; advance to next
                    self.num_params += 1;
                    let idx = self.num_params - 1; // borrowck
                    self.params[idx] = 0;
                } else {
                    if self.num_params == 0 {
                        self.num_params = 1;
                        self.params[0] = 0;
                    }

                    let idx = self.num_params - 1;
                    self.params[idx] *= 10;
                    self.params[idx] += (byte - b'0') as i64;
                }
            },
            Action::Clear => {
                self.intermediate_idx = 0;
                self.num_params = 0;
                self.ignoring = false;
            },
            Action::BeginUtf8 => {
                self.process_utf8(performer, byte);
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
    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, byte: u8);

    /// Pass bytes as part of a device control string to the handle chosen in `hook`. C0 controls
    /// will also be passed to the handler.
    fn put(&mut self, byte: u8);

    /// Called when a device control string is terminated
    ///
    /// The previously selected handler should be notified that the DCS has
    /// terminated.
    fn unhook(&mut self, byte: u8);

    /// Notifies the start of an Operating System Command
    fn osc_start(&mut self);

    /// Receives characters for the OSC control string
    ///
    /// Apparently characters don't need buffering here.
    fn osc_put(&mut self, byte: u8);

    /// Called when the OSC has terminated
    fn osc_end(&mut self, byte: u8);

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
