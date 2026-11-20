//! Shapes an Arabic word and prints each stage as code points.
//!
//! `assets/make_assets.py` draws `assets/arabic-shaping.png` from this output.
//! Lines: `logical`, `shaped` (presentation forms, still logical order) and
//! `visual` (the order a left-to-right bitmap renderer draws them in).

use arabic_shape::{shape_logical, visual};

fn hex(cs: impl IntoIterator<Item = char>) -> String {
    cs.into_iter().map(|c| format!("{:04X}", c as u32)).collect::<Vec<_>>().join(" ")
}

fn main() {
    let word = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "\u{0627}\u{0644}\u{0633}\u{0644}\u{0627}\u{0645}".into()); // "السلام"
    println!("logical {}", hex(word.chars()));
    println!("shaped {}", hex(shape_logical(&word)));
    println!("visual {}", hex(visual(&word).chars()));
}
