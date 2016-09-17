extern crate syntex;
extern crate syntex_syntax;

mod ext;

#[path="../../src/definitions.rs"]
pub mod definitions;

use std::path::Path;

fn main() {
    let src = &Path::new("../src/table.rs.in");
    let dst = &Path::new("../src/table.rs");

    let mut registry = syntex::Registry::new();
    ext::register(&mut registry);
    registry.expand("state_table", src, dst).expect("expand stable_table ok");
}
