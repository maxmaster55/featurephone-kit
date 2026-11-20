//! Encodes a short and a long message and prints the PDUs and compose counters.
//!
//! `assets/make_assets.py` reads this output to draw the PDU anatomy and the
//! 160/153 split diagrams. For each message: `text <text>`, then
//! `count <units> <capacity> <parts> <unicode>`, then one
//! `pdu <tpdu_len> <hex>` line per part.

const NUMBER: &str = "+15555550123"; // fictional (555-01xx range)

fn show(text: &str) {
    println!("text {text}");
    let c = smspdu::count(text);
    println!("count {} {} {} {}", c.units, c.capacity, c.parts, c.unicode);
    for p in smspdu::encode(NUMBER, text, 0x2A) {
        println!("pdu {} {}", p.tpdu_len, p.hex);
    }
}

fn main() {
    show("Hello [world] €5");
    show("Lorem ipsum dolor sit amet. ".repeat(7).trim_end());
}
