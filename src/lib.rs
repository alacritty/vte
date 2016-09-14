mod table;
mod definitions;

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


pub struct StateMachine<P: Parser> {
    state: State,
    parser: P,
}

impl<P: Parser> StateMachine<P> {
    pub fn advance(&mut self, byte: u8) {
        // Handle state changes in the anywhere state before evaluating changes
        // for current state.
        let mut change = STATE_CHANGE[State::Anywhere as usize][byte as usize];
        if change == 0 {
            change = STATE_CHANGE[self.state as usize][byte as usize];
        }

        // Unpack into a state and action
        let (state, action) = unpack(change);

        self.perform_state_change(state, action, byte);
    }

    fn perform_state_change(&mut self, state: State, action: Action, byte: u8) {
        macro_rules! maybe_action {
            ($action:expr, $arg:expr) => {
                match $action {
                    Action::None | Action::Unused__ => (),
                    action => {
                        self.perform_action(action, $arg);
                    },
                }
            }
        }

        match state {
            State::Anywhere | State::Unused__ => {
                // Just run the action
                self.perform_action(action, byte);
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

    /// XXX I don't think this handles UTF-8 properly. Hmm...
    fn perform_action(&mut self, action: Action, byte: u8) {
        unimplemented!();

        match action {
            Action::Execute => self.parser.execute(byte),
            Action::Hook => self.parser.hook(byte),
            Action::Put => self.parser.put(byte),
            Action::OscStart => self.parser.osc_start(byte),
            Action::OscPut => self.parser.osc_put(byte),
            Action::OscEnd => self.parser.osc_end(byte),
            Action::Unhook => self.parser.unhook(byte),
            Action::CsiDispatch => self.parser.csi_dispatch(byte),
            Action::EscDispatch => self.parser.esc_dispatch(byte),
            Action::Ignore | Action::None | Action::Unused__=> (),
            Action::Collect => {
                unimplemented!();
            },
            Action::Param => {
                unimplemented!();
            },
            Action::Clear => {
                unimplemented!();
            }
        }
    }
}

pub trait Parser {
    fn csi_entry(&mut self, byte: u8);
    fn csi_param(&mut self, byte: u8);
}

// 
// struct Foo;
// 
// impl Parser for Foo {
//     fn csi_entry(&mut self, c: char) {
//         println!("csi_entry char={:?}", c);
//     }
//     fn csi_param(&mut self, c: char) {
//         println!("csi_param char={:?}", c);
//     }
// }
// 
// #[test]
// fn it_works() {
//     let table: u8 = &[Parser::csi_entry, Parser::csi_param];
//     let mut foo = Foo;
//     table[0](&mut foo, 'b');
// }

