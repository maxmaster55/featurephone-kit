# featurephone-kit

Small, dependency-free, `no_std`-friendly Rust crates for building keypad
feature phones and other tiny devices.

They were extracted from **WeaverOS**, an OS for a keypad feature phone, where
they drive the on-screen UI, the cellular modem's SMS path and the bitmap-font
text renderer.

| Crate | What it does |
| --- | --- |
| [`t9-input`](crates/t9-input) | Multi-tap and predictive (T9) keypad text input, with Latin and Arabic keypad layouts. |
| [`smspdu`](crates/smspdu) | SMS PDU encode/decode (3GPP TS 23.040/23.038): GSM 7-bit with extension table, UCS-2, 160/153 and 70/67 concatenation. |
| [`arabic-shape`](crates/arabic-shape) | Arabic shaping (joining forms, lam-alef) and a bidi subset for bitmap-font renderers. |

All three are `no_std` + `alloc` when the default `std` feature is disabled,
have no dependencies, and need Rust 1.74 or newer.

```toml
[dependencies]
smspdu = { version = "0.1", default-features = false }
```

## Development

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --no-default-features
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
