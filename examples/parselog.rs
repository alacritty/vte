//! Parse input from stdin and log actions on stdout
extern crate vtparse;

use std::io::{self, Read};

use vtparse::{StateMachine, Parser};

/// A type implementing Parser that just logs actions
struct Log;

impl Parser for Log {
    fn print(&mut self, _machine: &StateMachine, c: char) {
        println!("[print] {:?}", c);
    }
    fn execute(&mut self, _machine: &StateMachine, byte: u8) {
        println!("[execute] byte={:02x}", byte);
    }
    fn hook(&mut self, _machine: &StateMachine, byte: u8) {
        println!("[hook] byte={:02x}", byte);
    }
    fn put(&mut self, _machine: &StateMachine, byte: u8) {
        println!("[put] byte={:02x}", byte);
    }
    fn osc_start(&mut self, _machine: &StateMachine, byte: u8) {
        println!("[osc_start] byte={:02x}", byte);
    }
    fn osc_put(&mut self, _machine: &StateMachine, byte: u8) {
        println!("[osc_put] byte={:02x}", byte);
    }
    fn osc_end(&mut self, _machine: &StateMachine, byte: u8) {
        println!("[osc_end] byte={:02x}", byte);
    }
    fn unhook(&mut self, _machine: &StateMachine, byte: u8) {
        println!("[unhook] byte={:02x}", byte);
    }
    fn csi_dispatch(&mut self, machine: &StateMachine, c: char) {
        println!("[csi_dispatch] params={:?}, intermediates={:?}, action={:?}",
                 machine.params(), machine.intermediates(), c);
    }
    fn esc_dispatch(&mut self, machine: &StateMachine, byte: u8) {
        println!("[csi_dispatch] params={:?}, intermediates={:?}, action={:?}",
                 machine.params(), machine.intermediates(), byte as char);
    }
}

fn main() {
    let input = io::stdin();
    let mut handle = input.lock();

    let mut statemachine = StateMachine::new();
    let mut parser = Log;

    let mut buf: [u8; 2048] = unsafe { std::mem::uninitialized() };

    loop {
        match handle.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for byte in &buf[..n] {
                    statemachine.advance(&mut parser, *byte);
                }
            },
            Err(err) => {
                println!("err: {}", err);
                break;
            }
        }
    }
}
