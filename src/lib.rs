mod table;
mod definitions;

pub use definitions::{Action, State, unpack};

use table::{EXIT_ACTIONS, ENTRY_ACTIONS, STATE_CHANGE};

impl State {
    /// Get exit action for this state
    #[inline(always)]
    pub fn exit_action(&self) -> Action {
        unsafe {
            *::table::EXIT_ACTIONS.get_unchecked(*self as usize)
        }
    }

    /// Get entry action for this state
    #[inline(always)]
    pub fn entry_action(&self) -> Action {
        unsafe {
            *::table::ENTRY_ACTIONS.get_unchecked(*self as usize)
        }
    }
}


// struct StateMachine<P: Parser> {
//     state: State,
// }
// 
// trait Parser {
//     fn csi_entry(&mut self, c: char);
//     fn csi_param(&mut self, c: char);
// }
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

