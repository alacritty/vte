CHANGELOG
=========

## 0.9.0

- Invalid UTF-8 sequences are now translated into replacement characters
  in a manner consistent with `Rust::from_utf8_lossy` and the resolution to
  ["How many replacement characters?"](https://hsivonen.fi/broken-utf-8/).
- Add a `Parser::end` function allowing users to mark the end of a stream,
  so that an incomplete UTF-8 encoding at the end of the stream can be
  reported.
- Remove 8-bit C1 support. 8-bit C1 codes are now interpreted as UTF-8
  continuation bytes.

## 0.8.0

- Remove C1 ST support in OSCs, fixing OSCs with ST in the payload

## 0.7.1

- Out of bounds when parsing a DCS with more than 16 parameters

## 0.7.0

- Fix params reset between escapes
- Removed unused parameter from `esc_dispatch`

## 0.6.0

- Fix build failure on Rust 1.36.0
- Add `bool_terminated` parameter to osc dispatch

## 0.5.0

- Support for dynamically sized escape buffers without feature `no_std`
- Improved UTF8 parser performance
- Migrate to Rust 2018

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
