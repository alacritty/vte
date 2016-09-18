vte
===

Parser for implementing virtual terminal emulators in Rust.

The parser is implemented according to [Paul Williams' ANSI parser state
machine]. The state machine doesn't assign meaning to the parsed data and is
thus not itself sufficient for writing a terminal emulator. Instead, it is
expected that an implementation of the `Perform` trait which does something
useful with the parsed data. The [`Parser`] handles the book keeping, and the
[`Perform`] gets to simply handle actions.

See the [docs] for more info.

[Paul Williams' ANSI parser state machine]: http://vt100.net/emu/dec_ansi_parser
[docs]: https://docs.rs/crate/vte/
