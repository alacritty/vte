//! Parse input from stdin and log actions on stdout
use std::io::{self, Read};

use vte::{Params, Parser, Perform};

/// A type implementing Perform that just logs actions
struct Log;

impl Perform for Log {
    fn print(&mut self, c: char) {
        println!("[print] {:?}", c);
    }

    fn execute(&mut self, byte: u8) {
        println!("[execute] {:02x}", byte);
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: char) {
        println!(
            "[hook] params={:?}, intermediates={:?}, ignore={:?}, char={:?}",
            params, intermediates, ignore, c
        );
    }

    fn put(&mut self, byte: u8) {
        println!("[put] {:02x}", byte);
    }

    fn unhook(&mut self) {
        println!("[unhook]");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        println!("[osc_dispatch] params={:?} bell_terminated={}", params, bell_terminated);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: char) {
        println!(
            "[csi_dispatch] params={:#?}, intermediates={:?}, ignore={:?}, char={:?}",
            params, intermediates, ignore, c
        );
        for param in params.iter() {
            dbg!(param);
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        println!(
            "[esc_dispatch] intermediates={:?}, ignore={:?}, byte={:02x}",
            intermediates, ignore, byte
        );
    }

    fn apc_begin(&mut self) {
        println!("[apc_begin]");
    }

    fn apc_end(&mut self) {
        println!("[apc_end]");
    }

    fn apc_put(&mut self, byte: u8) {
        println!("[apc_end] {:?}", byte as char);
    }
}

fn main() {
    let mut statemachine = Parser::new();
    let mut performer = Log;

    let buf = include_bytes!("apc.vte");

    for byte in buf {
        statemachine.advance(&mut performer, *byte);
    }
}
