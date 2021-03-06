# gapbuf-rs

[![Crates.io](https://img.shields.io/crates/v/gapbuf.svg)](https://crates.io/crates/gapbuf)
[![Docs.rs](https://docs.rs/gapbuf/badge.svg)](https://docs.rs/gapbuf)
[![Actions Status](https://github.com/frozenlib/gapbuf-rs/workflows/CI/badge.svg)](https://github.com/frozenlib/gapbuf-rs/actions)

Generic gap buffer implementation in Rust.

This crate provides the type `GapBuffer`.
This type has methods similar to `Vec`.

## Examples

```rust
use gapbuf::gap_buffer;

let mut b = gap_buffer![1, 2, 3];
b.insert(1, 10);
assert_eq!(b, [1, 10, 2, 3]);

b.remove(2);
assert_eq!(b, [1, 10, 3]);
```

## License

This project is dual licensed under Apache-2.0/MIT. See the two LICENSE-\* files for details.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
