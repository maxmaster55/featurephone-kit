//! Keypad text input for feature phones: classic multi-tap and a small
//! predictive ("T9"-style) dictionary.
//!
//! * [`MultiTap`] turns key events into [`Edit`]s: `2` types `a`, tapping `2`
//!   again within the timeout replaces it with `b`, then `c`, then `2`, then
//!   wraps. A long press types the digit; `#` cycles the input [`Mode`].
//! * [`Dict`] is predictive completion: one key press per letter, and the
//!   words whose key sequence starts with what was typed are returned ranked
//!   (whole-length matches first, then by score).
//!
//! The crate is `no_std` (with `alloc`) when the default `std` feature is
//! turned off. It never reads a clock: every time-sensitive call takes a
//! monotonic `now_ms` timestamp from the caller, which also makes the logic
//! trivially testable.
//!
//! # Example
//!
//! ```
//! use t9_input::{apply, Edit, Event, Key, Layout, Mode, MultiTap, Outcome};
//!
//! let mut tap = MultiTap::new(Mode::Lower, Layout::Text);
//! let mut text = String::new();
//!
//! // "2" then "2" again 300 ms later: a -> b
//! for (t, key) in [(0, Key::Digit(2)), (300, Key::Digit(2))] {
//!     if let Outcome::Edit(e) = tap.key(Event::Press(key), t) {
//!         apply(&mut text, e, 160);
//!     }
//! }
//! assert_eq!(text, "b");
//!
//! // 2 seconds later the tap has timed out: a new letter starts
//! if let Outcome::Edit(e) = tap.key(Event::Press(Key::Digit(2)), 2300) {
//!     assert_eq!(e, Edit::Insert('a'));
//! }
//! ```
//!
//! Minimum supported Rust version: 1.74.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Letters per key for ordinary text (index = digit key).
pub const TEXT: [&str; 10] = [" 0+", ".,?!-'@1", "abc2", "def3", "ghi4", "jkl5", "mno6", "pqrs7", "tuv8", "wxyz9"];

/// Letters per key for command-line style input (punctuation useful in
/// paths and options on key 1).
pub const SHELL: [&str; 10] = [" 0+", ".-/_:=~,1", "abc2", "def3", "ghi4", "jkl5", "mno6", "pqrs7", "tuv8", "wxyz9"];

/// The Arabic phone keypad (the Nokia layout sold in Egypt): 2 ب ت ث ة,
/// 3 ا أ إ آ ء ؤ ئ ى, 4 س ش ص ض, 5 د ذ ر ز, 6 ج ح خ, 7 ن ه و ي,
/// 8 ف ق ك ل م, 9 ط ظ ع غ; 1 punctuation, 0 space.
pub const ARABIC: [&str; 10] = [
    " 0",
    ".\u{060C}\u{061F}!:-'@1",
    "\u{0628}\u{062A}\u{062B}\u{0629}2",
    "\u{0627}\u{0623}\u{0625}\u{0622}\u{0621}\u{0624}\u{0626}\u{0649}3",
    "\u{0633}\u{0634}\u{0635}\u{0636}4",
    "\u{062F}\u{0630}\u{0631}\u{0632}5",
    "\u{062C}\u{062D}\u{062E}6",
    "\u{0646}\u{0647}\u{0648}\u{064A}7",
    "\u{0641}\u{0642}\u{0643}\u{0644}\u{0645}8",
    "\u{0637}\u{0638}\u{0639}\u{063A}9",
];

/// Symbols cycled by the `*` key.
pub const SYMBOLS: &str = "|>&;$\"'()<!?#%*+@\\[]{}^`";

/// Default multi-tap timeout: a repeat press within this many milliseconds
/// cycles the letter instead of starting a new one.
pub const DEFAULT_TIMEOUT_MS: u64 = 1000;

/// A key on the keypad, as far as text input cares.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    /// A digit key, `0..=9`.
    Digit(u8),
    /// The `*` key (symbols).
    Star,
    /// The `#` key (cycles the mode).
    Hash,
    /// Any other key (navigation, soft keys, call/end...). It ends an open tap.
    Other,
}

/// A key event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// Key went down.
    Press(Key),
    /// Auto-repeat while held (ignored).
    Repeat(Key),
    /// The key was held past the long-press threshold.
    Long(Key),
    /// Key went up (ignored).
    Release(Key),
}

/// The input mode, cycled with `#`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Lower-case letters.
    Lower,
    /// Upper-case letters.
    Upper,
    /// Digits only.
    Digit,
    /// The Arabic keypad ([`ARABIC`]).
    Arabic,
}

/// Which key-to-letters table the Latin modes use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    /// Ordinary text ([`TEXT`]); `#` cycles abc, ABC, Arabic, 123.
    Text,
    /// Command-line style ([`SHELL`]); `#` cycles abc, ABC, 123 (no Arabic).
    Shell,
}

/// A change to apply to the text buffer.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Edit {
    /// Append a character.
    Insert(char),
    /// Replace the character typed last (the next letter on the same key).
    Replace(char),
}

/// What a key event did.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Outcome {
    /// Nothing to do.
    None,
    /// The mode changed (`#`); refresh the mode label.
    ModeChanged,
    /// Edit the text.
    Edit(Edit),
}

/// Classic multi-tap state machine.
#[derive(Clone, Debug)]
pub struct MultiTap {
    /// The current input mode.
    pub mode: Mode,
    last: Option<Key>,
    idx: usize,
    at: u64,
    layout: Layout,
    timeout_ms: u64,
}

impl MultiTap {
    /// A new input in `mode` using `layout`, with [`DEFAULT_TIMEOUT_MS`].
    pub fn new(mode: Mode, layout: Layout) -> Self {
        MultiTap {
            mode,
            last: None,
            idx: 0,
            at: 0,
            layout,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    /// Use a different multi-tap timeout (milliseconds).
    #[must_use]
    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    /// A short label for the mode: `abc`, `ABC`, `123` or `عربي`.
    pub fn label(&self) -> &'static str {
        match self.mode {
            Mode::Lower => "abc",
            Mode::Upper => "ABC",
            Mode::Digit => "123",
            Mode::Arabic => "\u{0639}\u{0631}\u{0628}\u{064A}",
        }
    }

    /// Set the mode (an app asking for Arabic or Latin input). Ends any open tap.
    pub fn set_mode(&mut self, m: Mode) {
        self.mode = m;
        self.last = None;
    }

    fn set(&self, k: Key) -> Option<&'static str> {
        match k {
            Key::Digit(d) if d > 9 => None,
            Key::Digit(d) if self.mode == Mode::Arabic => Some(ARABIC[d as usize]),
            Key::Digit(d) => Some(match self.layout {
                Layout::Shell => SHELL[d as usize],
                Layout::Text => TEXT[d as usize],
            }),
            Key::Star => Some(SYMBOLS),
            _ => None,
        }
    }

    fn case(&self, c: char) -> char {
        if self.mode == Mode::Upper {
            c.to_ascii_uppercase()
        } else {
            c
        }
    }

    fn open(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.at) < self.timeout_ms
    }

    /// The characters of the key being tapped and the index of the one
    /// currently typed, while a tap is still open (for a hint row).
    pub fn pending(&self, now_ms: u64) -> Option<(Vec<char>, usize)> {
        let k = self.last?;
        if now_ms.saturating_sub(self.at) > self.timeout_ms || self.mode == Mode::Digit {
            return None;
        }
        Some((self.set(k)?.chars().map(|c| self.case(c)).collect(), self.idx))
    }

    /// Close the open tap (another input device or a cursor move intervened).
    pub fn settle(&mut self) {
        self.last = None;
    }

    /// Feed a key event at time `now_ms` (any monotonic millisecond clock).
    pub fn key(&mut self, e: Event, now_ms: u64) -> Outcome {
        match e {
            Event::Long(Key::Digit(d)) if d <= 9 && self.mode != Mode::Digit && self.last == Some(Key::Digit(d)) => {
                self.last = None;
                Outcome::Edit(Edit::Replace((b'0' + d) as char))
            }
            Event::Press(Key::Hash) => {
                self.mode = match (self.mode, self.layout) {
                    (Mode::Lower, _) => Mode::Upper,
                    (Mode::Upper, Layout::Shell) => Mode::Digit,
                    (Mode::Upper, Layout::Text) => Mode::Arabic,
                    (Mode::Arabic, _) => Mode::Digit,
                    (Mode::Digit, _) => Mode::Lower,
                };
                self.last = None;
                Outcome::ModeChanged
            }
            Event::Press(k @ (Key::Digit(0..=9) | Key::Star)) => {
                if self.mode == Mode::Digit {
                    self.last = None;
                    return Outcome::Edit(Edit::Insert(match k {
                        Key::Digit(d) => (b'0' + d) as char,
                        _ => '*',
                    }));
                }
                let Some(set) = self.set(k) else { return Outcome::None };
                let set: Vec<char> = set.chars().collect();
                if self.last == Some(k) && self.open(now_ms) && set.len() > 1 {
                    self.idx = (self.idx + 1) % set.len();
                    self.at = now_ms;
                    return Outcome::Edit(Edit::Replace(self.case(set[self.idx])));
                }
                self.last = Some(k);
                self.idx = 0;
                self.at = now_ms;
                Outcome::Edit(Edit::Insert(self.case(set[0])))
            }
            Event::Long(Key::Digit(_)) | Event::Release(_) | Event::Repeat(_) => Outcome::None,
            _ => {
                self.last = None;
                Outcome::None
            }
        }
    }
}

/// Apply an edit to a string buffer, keeping at most `max` characters
/// (`Insert` is dropped when the buffer is full; `Replace` always applies).
pub fn apply(s: &mut String, e: Edit, max: usize) {
    match e {
        Edit::Insert(c) => {
            if s.chars().count() < max {
                s.push(c)
            }
        }
        Edit::Replace(c) => {
            s.pop();
            s.push(c);
        }
    }
}

// ---------------------------------------------------------------- predictive

/// The digit key a character is on (letters and digits, same on [`TEXT`] and [`SHELL`]);
/// everything else (symbols) is treated as key 1 for prediction.
///
/// ```
/// assert_eq!(t9_input::key_of('s'), 7);
/// assert_eq!(t9_input::key_of('/'), 1);
/// ```
pub fn key_of(c: char) -> u8 {
    let c = c.to_ascii_lowercase();
    for (d, set) in SHELL.iter().enumerate() {
        if c != ' ' && set.contains(c) {
            return d as u8;
        }
    }
    1
}

/// Whether the key sequence `seq` fits the start of `w` (one key per letter).
pub fn fits(w: &str, seq: &[u8]) -> bool {
    w.chars().count() >= seq.len() && w.chars().zip(seq).all(|(c, &k)| key_of(c) == k)
}

/// A scored word list for predictive input.
///
/// Words are ranked: exact-length matches first, then by score (use counts),
/// then shorter, then alphabetical. The dictionary starts empty; fill it from
/// your own word list, command history, contacts, etc.
///
/// ```
/// use t9_input::Dict;
/// let mut d = Dict::new();
/// d.add_many(["home", "good", "gone", "hello"], 1);
/// d.add("good", 5); // used more often
/// // 4 6 6 3 = h-o-m-e / g-o-o-d / g-o-n-e
/// assert_eq!(d.candidates(&[4, 6, 6, 3], 3), ["good", "gone", "home"]);
/// ```
#[derive(Default, Clone, Debug)]
pub struct Dict {
    words: Vec<(String, u32)>,
}

impl Dict {
    /// An empty dictionary.
    pub fn new() -> Dict {
        Dict::default()
    }

    /// Add `w` with `score`, or raise the score if it is already known.
    /// Empty words and words over 60 bytes are ignored.
    pub fn add(&mut self, w: &str, score: u32) {
        if w.is_empty() || w.len() > 60 {
            return;
        }
        match self.words.iter_mut().find(|(x, _)| x == w) {
            Some(e) => e.1 = e.1.saturating_add(score),
            None => self.words.push((w.to_string(), score)),
        }
    }

    /// [`add`](Self::add) many words with the same score.
    pub fn add_many<'a>(&mut self, ws: impl IntoIterator<Item = &'a str>, score: u32) {
        for w in ws {
            self.add(w, score);
        }
    }

    /// Number of words.
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Whether the dictionary has no words.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Up to `limit` words whose key sequence starts with `seq`, best first.
    pub fn candidates(&self, seq: &[u8], limit: usize) -> Vec<String> {
        if seq.is_empty() {
            return Vec::new();
        }
        let mut m: Vec<(&str, u32, bool)> = self
            .words
            .iter()
            .filter(|(w, _)| fits(w, seq))
            .map(|(w, s)| (w.as_str(), *s, w.chars().count() == seq.len()))
            .collect();
        m.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)).then(a.0.len().cmp(&b.0.len())).then(a.0.cmp(b.0)));
        m.into_iter().take(limit).map(|x| x.0.to_string()).collect()
    }

    /// What to show when nothing matches: the first letter of each key.
    pub fn fallback(seq: &[u8]) -> String {
        seq.iter()
            .map(|&k| SHELL.get(k as usize).and_then(|s| s.chars().next()).unwrap_or('?'))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Event::{Long, Press};

    fn tap(t: &mut MultiTap, d: u8, now: u64) -> Outcome {
        t.key(Press(Key::Digit(d)), now)
    }

    #[test]
    fn keys_of_letters() {
        assert_eq!(key_of('a'), 2);
        assert_eq!(key_of('S'), 7);
        assert_eq!(key_of('/'), 1);
        assert_eq!(key_of('3'), 3);
        assert_eq!(key_of(' '), 1);
    }

    #[test]
    fn predicts_in_score_order() {
        let mut d = Dict::new();
        assert!(d.is_empty());
        d.add_many(["ls", "lp"], 1);
        assert_eq!(d.len(), 2);
        // same keys, same score: alphabetical
        assert_eq!(d.candidates(&[5, 7], 3), ["lp", "ls"]);
        d.add("ls", 5);
        assert_eq!(d.candidates(&[5, 7], 3), ["ls", "lp"]);
        // a whole-length match beats a longer word with more use
        d.add("pkill", 600);
        d.add("phone", 500);
        d.add("pi", 1);
        assert_eq!(d.candidates(&[7, 4], 3), ["pi", "phone"]);
        assert_eq!(d.candidates(&[7], 1)[0], "pkill");
        assert_eq!(d.candidates(&[7, 4, 6, 6, 3], 3)[0], "phone");
        assert!(d.candidates(&[], 3).is_empty());
        assert!(d.candidates(&[9, 9, 9], 3).is_empty());
        d.add("", 1);
        d.add(&"x".repeat(61), 1);
        assert_eq!(d.len(), 5);
        assert_eq!(Dict::fallback(&[5, 7, 9]), "jpw");
    }

    #[test]
    fn multitap_cycles_and_holds() {
        let mut t = MultiTap::new(Mode::Lower, Layout::Shell);
        assert_eq!(tap(&mut t, 2, 0), Outcome::Edit(Edit::Insert('a')));
        assert_eq!(tap(&mut t, 2, 100), Outcome::Edit(Edit::Replace('b')));
        assert_eq!(t.key(Long(Key::Digit(2)), 700), Outcome::Edit(Edit::Replace('2')));
        assert_eq!(tap(&mut t, 0, 800), Outcome::Edit(Edit::Insert(' ')));
    }

    #[test]
    fn cycling_wraps_around() {
        let mut t = MultiTap::new(Mode::Lower, Layout::Text);
        let got: Vec<Outcome> = (0..5).map(|i| tap(&mut t, 2, i * 100)).collect();
        assert_eq!(
            got,
            [
                Outcome::Edit(Edit::Insert('a')),
                Outcome::Edit(Edit::Replace('b')),
                Outcome::Edit(Edit::Replace('c')),
                Outcome::Edit(Edit::Replace('2')),
                Outcome::Edit(Edit::Replace('a')),
            ]
        );
    }

    #[test]
    fn timeout_starts_a_new_letter() {
        let mut t = MultiTap::new(Mode::Lower, Layout::Text);
        tap(&mut t, 2, 0);
        // just inside the window cycles, a full timeout later does not
        assert_eq!(tap(&mut t, 2, 999), Outcome::Edit(Edit::Replace('b')));
        assert_eq!(tap(&mut t, 2, 999 + 1000), Outcome::Edit(Edit::Insert('a')));
        // a custom timeout
        let mut t = MultiTap::new(Mode::Lower, Layout::Text).with_timeout(200);
        tap(&mut t, 3, 0);
        assert_eq!(tap(&mut t, 3, 250), Outcome::Edit(Edit::Insert('d')));
        // a clock that goes backwards never panics
        assert_eq!(tap(&mut t, 3, 10), Outcome::Edit(Edit::Replace('e')));
    }

    #[test]
    fn different_key_and_other_keys_end_the_tap() {
        let mut t = MultiTap::new(Mode::Lower, Layout::Text);
        tap(&mut t, 2, 0);
        assert_eq!(tap(&mut t, 3, 50), Outcome::Edit(Edit::Insert('d')));
        assert_eq!(t.key(Press(Key::Other), 60), Outcome::None);
        assert_eq!(tap(&mut t, 3, 70), Outcome::Edit(Edit::Insert('d')));
        t.settle();
        assert_eq!(tap(&mut t, 3, 80), Outcome::Edit(Edit::Insert('d')));
    }

    #[test]
    fn pending_hint_row() {
        let mut t = MultiTap::new(Mode::Upper, Layout::Text);
        assert_eq!(t.pending(0), None);
        tap(&mut t, 2, 0);
        tap(&mut t, 2, 10);
        assert_eq!(t.pending(500), Some((alloc::vec!['A', 'B', 'C', '2'], 1)));
        assert_eq!(t.pending(2000), None);
    }

    #[test]
    fn upper_case_and_digit_mode() {
        let mut t = MultiTap::new(Mode::Upper, Layout::Text);
        assert_eq!(tap(&mut t, 9, 0), Outcome::Edit(Edit::Insert('W')));
        t.set_mode(Mode::Digit);
        assert_eq!(tap(&mut t, 9, 10), Outcome::Edit(Edit::Insert('9')));
        assert_eq!(tap(&mut t, 9, 20), Outcome::Edit(Edit::Insert('9')));
        assert_eq!(t.key(Press(Key::Star), 30), Outcome::Edit(Edit::Insert('*')));
        assert_eq!(t.key(Long(Key::Digit(9)), 40), Outcome::None);
    }

    #[test]
    fn long_press_needs_an_open_tap_on_that_key() {
        let mut t = MultiTap::new(Mode::Lower, Layout::Text);
        assert_eq!(t.key(Long(Key::Digit(2)), 0), Outcome::None);
        tap(&mut t, 3, 0);
        assert_eq!(t.key(Long(Key::Digit(2)), 10), Outcome::None);
        assert_eq!(t.key(Long(Key::Digit(3)), 10), Outcome::Edit(Edit::Replace('3')));
    }

    #[test]
    fn star_cycles_symbols_and_bad_digits_are_ignored() {
        let mut t = MultiTap::new(Mode::Lower, Layout::Text);
        assert_eq!(t.key(Press(Key::Star), 0), Outcome::Edit(Edit::Insert('|')));
        assert_eq!(t.key(Press(Key::Star), 1), Outcome::Edit(Edit::Replace('>')));
        assert_eq!(tap(&mut t, 12, 2), Outcome::None);
        assert_eq!(t.key(Event::Release(Key::Star), 3), Outcome::None);
        assert_eq!(t.key(Event::Repeat(Key::Star), 3), Outcome::None);
    }

    #[test]
    fn arabic_keypad() {
        let mut t = MultiTap::new(Mode::Lower, Layout::Text);
        assert_eq!(t.key(Press(Key::Hash), 0), Outcome::ModeChanged);
        t.key(Press(Key::Hash), 0);
        assert_eq!(t.mode, Mode::Arabic);
        assert_eq!(t.label(), "\u{0639}\u{0631}\u{0628}\u{064A}");
        // 8 8 8 8 = lam
        assert_eq!(tap(&mut t, 8, 0), Outcome::Edit(Edit::Insert('\u{0641}')));
        tap(&mut t, 8, 1);
        tap(&mut t, 8, 2);
        assert_eq!(tap(&mut t, 8, 3), Outcome::Edit(Edit::Replace('\u{0644}')));
        t.key(Press(Key::Hash), 4);
        assert_eq!(t.mode, Mode::Digit);
        t.key(Press(Key::Hash), 5);
        assert_eq!(t.mode, Mode::Lower);
        // the shell layout never offers Arabic
        let mut s = MultiTap::new(Mode::Lower, Layout::Shell);
        s.key(Press(Key::Hash), 0);
        s.key(Press(Key::Hash), 0);
        assert_eq!(s.mode, Mode::Digit);
    }

    #[test]
    fn apply_respects_max() {
        let mut s = String::new();
        apply(&mut s, Edit::Insert('a'), 2);
        apply(&mut s, Edit::Insert('b'), 2);
        apply(&mut s, Edit::Insert('c'), 2);
        assert_eq!(s, "ab");
        apply(&mut s, Edit::Replace('z'), 2);
        assert_eq!(s, "az");
        let mut e = String::new();
        apply(&mut e, Edit::Replace('q'), 2);
        assert_eq!(e, "q");
    }

    #[test]
    fn typing_a_word_end_to_end() {
        // "hi": 4 4 -> h, then (after the timeout) 4 4 4 -> i
        let mut t = MultiTap::new(Mode::Lower, Layout::Text);
        let mut s = String::new();
        for (now, d) in [(0, 4), (100, 4), (2000, 4), (2100, 4), (2200, 4)] {
            if let Outcome::Edit(e) = tap(&mut t, d, now) {
                apply(&mut s, e, 20);
            }
        }
        assert_eq!(s, "hi");
    }
}
