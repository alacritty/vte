#![allow(dead_code)]
extern crate syntex;
extern crate syntex_syntax;

mod ext;

use std::path::Path;

fn main() {
    // Expand VT parser state table
    let mut registry = syntex::Registry::new();
    ext::vt::register(&mut registry);
    let src = &Path::new("../src/table.rs.in");
    let dst = &Path::new("../src/table.rs");
    registry.expand("vt_state_table", src, dst).expect("expand vt_stable_table ok");

    // Expand UTF8 parser state table
    let mut registry = syntex::Registry::new();
    ext::utf8::register(&mut registry);
    let src = &Path::new("../src/utf8/table.rs.in");
    let dst = &Path::new("../src/utf8/table.rs");
    registry.expand("utf8_state_table", src, dst).expect("expand utf8_stable_table ok");
}
