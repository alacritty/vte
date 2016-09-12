codegen
=======

Depends on libsyntex and generates table.rs from table.rs.in. This code is
separate from the main vtparse crate since compiling libsyntex takes ~1
eternity.

## Usage

`cargo run` in the codegen folder will process `table.rs.in` and output
`table.rs`. The latter file should be committed back into the repo.
