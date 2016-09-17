mod table;
mod definitions;
mod utf8;

pub use definitions::{Action, State, unpack};

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

struct VtUtf8Receiver<'a, P: Parser + 'a>(&'a mut P, &'a mut State);

impl<'a, P: Parser> utf8::Receiver for VtUtf8Receiver<'a, P> {
    fn codepoint(&mut self, c: char) {
        self.0.print(c);
        *self.1 = State::Ground;
    }

    fn invalid_sequence(&mut self) {
        self.0.print('�');
        *self.1 = State::Ground;
    }
}

/// ANSI VTE Parser
///
/// As described in http://vt100.net/emu/dec_ansi_parser
///
/// TODO: utf8 support
pub struct StateMachine {
    state: State,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_idx: usize,
    params: [i64; MAX_PARAMS],
    num_params: usize,
    ignoring: bool,
    utf8_parser: utf8::Parser,
}

impl StateMachine {
    pub fn new() -> StateMachine {
        StateMachine {
            state: State::Ground,
            intermediates: [0u8; MAX_INTERMEDIATES],
            intermediate_idx: 0,
            params: [0i64; MAX_PARAMS],
            num_params: 0,
            ignoring: false,
            utf8_parser: utf8::Parser::new(),
        }
    }

    pub fn params(&self) -> &[i64] {
        &self.params[..self.num_params]
    }

    pub fn intermediates(&self) -> &[u8] {
        &self.intermediates[..self.intermediate_idx]
    }

    pub fn advance<P: Parser>(&mut self, parser: &mut P, byte: u8) {
        // Utf8 characters are handled out-of-band.
        if let State::Utf8 = self.state {
            self.process_utf8(parser, byte);
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

        self.perform_state_change(parser, state, action, byte);
    }

    #[inline]
    fn process_utf8<P>(&mut self, parser: &mut P, byte: u8)
        where P: Parser
    {
        let mut receiver = VtUtf8Receiver(parser, &mut self.state);
        let utf8_parser = &mut self.utf8_parser;
        utf8_parser.advance(&mut receiver, byte);
    }

    fn perform_state_change<P>(&mut self, parser: &mut P, state: State, action: Action, byte: u8)
        where P: Parser
    {
        macro_rules! maybe_action {
            ($action:expr, $arg:expr) => {
                match $action {
                    Action::None => (),
                    action => {
                        self.perform_action(parser, action, $arg);
                    },
                }
            }
        }

        match state {
            State::Anywhere => {
                // Just run the action
                self.perform_action(parser, action, byte);
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

    fn perform_action<P: Parser>(&mut self, parser: &mut P, action: Action, byte: u8) {
        match action {
            Action::Print => parser.print(byte as char),
            Action::Execute => parser.execute(self, byte),
            Action::Hook => parser.hook(self, byte),
            Action::Put => parser.put(self, byte),
            Action::OscStart => parser.osc_start(self, byte),
            Action::OscPut => parser.osc_put(self, byte),
            Action::OscEnd => parser.osc_end(self, byte),
            Action::Unhook => parser.unhook(self, byte),
            Action::CsiDispatch => parser.csi_dispatch(self, byte as char),
            Action::EscDispatch => parser.esc_dispatch(self, byte),
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
                // if byte == ';'
                if byte == 0x3b {
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
                    self.params[idx] += (byte - ('0' as u8)) as i64;
                }
            },
            Action::Clear => {
                self.intermediate_idx = 0;
                self.num_params = 0;
                self.ignoring = false;
            },
            Action::BeginUtf8 => {
                self.process_utf8(parser, byte);
            },
        }
    }
}

pub trait Parser {
    fn print(&mut self, c: char);
    fn execute(&mut self, &StateMachine, byte: u8);
    fn hook(&mut self, &StateMachine, byte: u8);
    fn put(&mut self, &StateMachine, byte: u8);
    fn osc_start(&mut self, &StateMachine, byte: u8);
    fn osc_put(&mut self, &StateMachine, byte: u8);
    fn osc_end(&mut self, &StateMachine, byte: u8);
    fn unhook(&mut self, &StateMachine, byte: u8);
    fn csi_dispatch(&mut self, &StateMachine, c: char);
    fn esc_dispatch(&mut self, &StateMachine, byte: u8);
}
