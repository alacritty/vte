CHANGELOG
=========

## 0.5.0

- Support for dynamically sized escape buffers without feature `no_std`
- Migration to Rust 2018
- Improved UTF8 parser performance

## 0.4.0

- Fix handling of DCS escapes

## 0.3.3

- Fix off-by-one error in CSI parsing when params list was at max length
  (previously caused a panic).
- Support no_std

## 0.2.0

- Removes `osc_start`, `osc_put`, and `osc_end`
- Adds `osc_dispatch` which simply receives a list of parameters
- Removes `byte: u8` parameter from `hook` and `unhook` because it's always
  zero.
