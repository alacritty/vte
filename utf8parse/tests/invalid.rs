use utf8parse::{Parser, Receiver};

#[derive(Debug, PartialEq)]
struct StringWrapper(String);

impl Receiver for StringWrapper {
    fn codepoint(&mut self, c: char) {
        self.0.push(c);
    }

    fn invalid_sequence(&mut self) {
        self.0.push('�');
    }
}

#[test]
fn multiple_invalid_continuations() {
    let mut parser = Parser::new();

    // utf8parse implementation
    let mut actual = StringWrapper(String::new());

    let input = b"\xdd\xdd*";

    for byte in input {
        while !parser.advance(&mut actual, *byte) {}
    }

    // standard library implementation
    let expected = String::from_utf8_lossy(input).to_string();

    assert_eq!(actual.0, expected);
}
