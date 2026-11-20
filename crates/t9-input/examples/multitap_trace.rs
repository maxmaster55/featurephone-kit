//! Types "hello" with multi-tap and prints the input state every 50 ms.
//!
//! `assets/make_assets.py` turns this trace into `assets/t9-multitap.gif`, so
//! the animation shows exactly what the crate does. Columns (tab separated):
//! time in ms, the key pressed at that instant (or `-`), the text buffer, the
//! letters of the open tap (or `-`), the index of the one being typed and the
//! edit the key produced (or `-`).

use t9_input::{apply, Event, Key, Layout, Mode, MultiTap, Outcome, DEFAULT_TIMEOUT_MS};

/// (time in ms, digit key). The second run of `5` waits out the timeout so the
/// first `l` is committed before the next one starts.
const PRESSES: [(u64, u8); 13] = [
    (400, 4),
    (700, 4),
    (1300, 3),
    (1600, 3),
    (2200, 5),
    (2500, 5),
    (2800, 5),
    (4000, 5),
    (4300, 5),
    (4600, 5),
    (5200, 6),
    (5500, 6),
    (5800, 6),
];
const END_MS: u64 = 7600;
const STEP_MS: u64 = 50;

fn main() {
    let mut tap = MultiTap::new(Mode::Lower, Layout::Text);
    let mut text = String::new();
    println!("# timeout_ms={DEFAULT_TIMEOUT_MS} mode={}", tap.label());
    for t in (0..=END_MS).step_by(STEP_MS as usize) {
        let (mut key, mut edit) = (String::from("-"), String::from("-"));
        for &(_, d) in PRESSES.iter().filter(|p| p.0 == t) {
            if let Outcome::Edit(e) = tap.key(Event::Press(Key::Digit(d)), t) {
                apply(&mut text, e, 160);
                edit = format!("{e:?}");
            }
            key = d.to_string();
        }
        let (set, idx) = match tap.pending(t) {
            Some((set, idx)) => (set.into_iter().collect::<String>(), idx.to_string()),
            None => ("-".into(), "-".into()),
        };
        println!("{t}\t{key}\t{text}\t{set}\t{idx}\t{edit}");
    }
}
