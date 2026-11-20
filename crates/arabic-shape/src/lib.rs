//! Arabic text shaping and a small bidi subset for bitmap-font renderers.
//!
//! Small displays and bitmap fonts usually cannot do OpenType shaping. This
//! crate converts logical-order Arabic text into the Unicode *Arabic
//! Presentation Forms* (isolated, initial, medial, final, including the
//! lam-alef ligatures; harakat and joiner controls are dropped) and reorders a
//! line into visual order with a subset of the Unicode bidi algorithm (one
//! paragraph, no explicit embeddings: strong L/R, European and Arabic numbers,
//! separators, neutrals, mirrored brackets). Mixed Arabic, Latin and digits
//! come out ready to draw left to right with a font that has the presentation
//! forms (FE70-FEFF).
//!
//! The crate is `no_std` (with `alloc`) when the default `std` feature is off.
//! Minimum supported Rust version: 1.74.
//!
//! # Example
//!
//! ```
//! use arabic_shape::{shape_logical, visual};
//!
//! // beh-yeh-teh: initial, medial, final forms (logical order)
//! let shaped: String = shape_logical("بيت").into_iter().collect();
//! assert_eq!(shaped, "\u{FE91}\u{FEF4}\u{FE96}");
//!
//! // a lone lam + alef becomes the ligature
//! assert_eq!(shape_logical("لا"), ['\u{FEFB}']);
//!
//! // visual order for drawing left to right: reversed, digits keep order
//! assert_eq!(visual("بيت"), "\u{FE96}\u{FEF4}\u{FE91}");
//! assert!(visual("رقم 123").starts_with("123 "));
//! assert_eq!(visual("hello"), "hello");
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::borrow::Cow;
use alloc::vec::Vec;

/// True for characters that need shaping or reordering (the Hebrew and
/// Arabic blocks and their presentation forms).
#[inline]
pub fn is_rtl_char(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

/// True if `s` has any right-to-left character.
#[inline]
pub fn has_rtl(s: &str) -> bool {
    // ASCII fast path: every RTL char is 2+ bytes starting at 0xD6..
    s.bytes().any(|b| b >= 0xD6) && s.chars().any(is_rtl_char)
}

/// The paragraph direction: true if the first strong character is
/// right-to-left.
pub fn rtl_para(s: &str) -> bool {
    for c in s.chars() {
        match class(c) {
            Bc::R => return true,
            Bc::L => return false,
            _ => {}
        }
    }
    false
}

/// Harakat and other marks that are dropped (bitmap fonts have no room for them).
#[inline]
fn transparent(c: char) -> bool {
    matches!(c as u32, 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7 | 0x06E8 | 0x06EA..=0x06ED | 0x200C | 0x200D | 0x200E | 0x200F)
}

/// (isolated presentation form, dual-joining) for U+0621..U+064A.
fn forms(c: char) -> Option<(u32, bool)> {
    let f = match c as u32 {
        0x0621 => (0xFE80, false),
        0x0622 => (0xFE81, false),
        0x0623 => (0xFE83, false),
        0x0624 => (0xFE85, false),
        0x0625 => (0xFE87, false),
        0x0626 => (0xFE89, true),
        0x0627 => (0xFE8D, false),
        0x0628 => (0xFE8F, true),
        0x0629 => (0xFE93, false),
        0x062A => (0xFE95, true),
        0x062B => (0xFE99, true),
        0x062C => (0xFE9D, true),
        0x062D => (0xFEA1, true),
        0x062E => (0xFEA5, true),
        0x062F => (0xFEA9, false),
        0x0630 => (0xFEAB, false),
        0x0631 => (0xFEAD, false),
        0x0632 => (0xFEAF, false),
        0x0633 => (0xFEB1, true),
        0x0634 => (0xFEB5, true),
        0x0635 => (0xFEB9, true),
        0x0636 => (0xFEBD, true),
        0x0637 => (0xFEC1, true),
        0x0638 => (0xFEC5, true),
        0x0639 => (0xFEC9, true),
        0x063A => (0xFECD, true),
        0x0641 => (0xFED1, true),
        0x0642 => (0xFED5, true),
        0x0643 => (0xFED9, true),
        0x0644 => (0xFEDD, true),
        0x0645 => (0xFEE1, true),
        0x0646 => (0xFEE5, true),
        0x0647 => (0xFEE9, true),
        0x0648 => (0xFEED, false),
        0x0649 => (0xFEEF, false),
        0x064A => (0xFEF1, true),
        // Persian keheh / yeh: drawn as kaf / yeh
        0x06A9 => (0xFED9, true),
        0x06CC => (0xFEF1, true),
        _ => return None,
    };
    Some(f)
}

/// Joins to the character after it (in logical order).
fn joins_next(c: char) -> bool {
    c == '\u{0640}' || forms(c).is_some_and(|f| f.1)
}
/// Joins to the character before it.
fn joins_prev(c: char) -> bool {
    c == '\u{0640}' || (forms(c).is_some() && c != '\u{0621}')
}

/// Contextual forms and lam-alef ligatures, in logical order; harakat are
/// dropped. Characters outside the shaped Arabic letters pass through.
pub fn shape_logical(s: &str) -> Vec<char> {
    let v: Vec<char> = s.chars().filter(|&c| !transparent(c)).collect();
    let mut out = Vec::with_capacity(v.len());
    let mut i = 0;
    while i < v.len() {
        let c = v[i];
        let prev = i.checked_sub(1).map(|j| v[j]);
        let next = v.get(i + 1).copied();
        let pj = prev.is_some_and(joins_next) && joins_prev(c);
        if c == '\u{0644}' {
            let lig = match next {
                Some('\u{0622}') => Some(0xFEF5),
                Some('\u{0623}') => Some(0xFEF7),
                Some('\u{0625}') => Some(0xFEF9),
                Some('\u{0627}') => Some(0xFEFB),
                _ => None,
            };
            if let Some(l) = lig {
                out.push(char::from_u32(l + pj as u32).unwrap());
                i += 2;
                continue;
            }
        }
        match forms(c) {
            Some((iso, dual)) => {
                let nj = dual && next.is_some_and(joins_prev);
                let f = match (pj, nj) {
                    (false, false) => iso,
                    (true, false) => iso + 1,
                    (false, true) => iso + 2,
                    (true, true) => iso + 3,
                };
                // hamza, alef-madda.. have 2 forms; 0xFE80 only one
                let f = if c == '\u{0621}' { iso } else { f };
                out.push(char::from_u32(f).unwrap_or(c));
            }
            None => out.push(c),
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------- bidi

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bc {
    L,
    R,
    En,
    An,
    Es,
    Et,
    Cs,
    Ws,
    On,
}

fn class(c: char) -> Bc {
    let u = c as u32;
    match c {
        '0'..='9' | '\u{06F0}'..='\u{06F9}' => Bc::En,
        '\u{0660}'..='\u{0669}' | '\u{066B}' | '\u{066C}' => Bc::An,
        '+' | '-' => Bc::Es,
        ',' | '.' | ':' | '/' | '\u{060C}' | '\u{00A0}' => Bc::Cs,
        '#' | '$' | '%' | '\u{00B0}' | '\u{00A3}' | '\u{20AC}' | '\u{066A}' => Bc::Et,
        ' ' | '\t' => Bc::Ws,
        _ if is_rtl_char(c) => Bc::R,
        _ if c.is_alphabetic() || (u > 0x2E80 && c.is_alphanumeric()) => Bc::L,
        _ => Bc::On,
    }
}

fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '\u{00AB}' => '\u{00BB}',
        '\u{00BB}' => '\u{00AB}',
        '\u{2039}' => '\u{203A}',
        '\u{203A}' => '\u{2039}',
        _ => c,
    }
}

/// Embedding levels for one paragraph of `v` with base level `base`.
fn levels(v: &[char], rtl: bool) -> Vec<u8> {
    let n = v.len();
    let base: u8 = rtl as u8;
    let e = if rtl { Bc::R } else { Bc::L };
    let mut t: Vec<Bc> = v.iter().map(|&c| class(c)).collect();
    // W2: EN after Arabic (last strong R) is AN; W7: after L it is L
    let mut last = e;
    for x in t.iter_mut() {
        match *x {
            Bc::L | Bc::R => last = *x,
            Bc::En if last == Bc::R => *x = Bc::An,
            _ => {}
        }
    }
    // W4: a single separator between two numbers of the same kind
    for i in 1..n.saturating_sub(1) {
        let (a, b) = (t[i - 1], t[i + 1]);
        match t[i] {
            Bc::Es if a == Bc::En && b == Bc::En => t[i] = Bc::En,
            Bc::Cs if a == b && (a == Bc::En || a == Bc::An) => t[i] = a,
            _ => {}
        }
    }
    // W5: ET next to EN
    for i in 0..n {
        if t[i] == Bc::En {
            let mut j = i;
            while j > 0 && t[j - 1] == Bc::Et {
                j -= 1;
                t[j] = Bc::En;
            }
            let mut j = i + 1;
            while j < n && t[j] == Bc::Et {
                t[j] = Bc::En;
                j += 1;
            }
        }
    }
    // W6 + W7
    let mut last = e;
    for x in t.iter_mut() {
        match *x {
            Bc::Es | Bc::Et | Bc::Cs => *x = Bc::On,
            Bc::L | Bc::R => last = *x,
            Bc::En if last == Bc::L => *x = Bc::L,
            _ => {}
        }
    }
    // N1/N2: runs of neutrals take the direction around them (numbers count
    // as R), else the paragraph's
    let dir = |b: Bc| match b {
        Bc::L => Some(Bc::L),
        Bc::R | Bc::En | Bc::An => Some(Bc::R),
        _ => None,
    };
    let mut i = 0;
    while i < n {
        if dir(t[i]).is_some() {
            i += 1;
            continue;
        }
        let s = i;
        while i < n && dir(t[i]).is_none() {
            i += 1;
        }
        let before = if s == 0 { e } else { dir(t[s - 1]).unwrap() };
        let after = if i == n { e } else { dir(t[i]).unwrap() };
        let d = if before == after { before } else { e };
        for x in &mut t[s..i] {
            *x = d;
        }
    }
    // I1/I2
    let mut lv: Vec<u8> = t
        .iter()
        .map(|&x| match (base, x) {
            (0, Bc::R) => 1,
            (0, Bc::An) | (0, Bc::En) => 2,
            (1, Bc::L) | (1, Bc::An) | (1, Bc::En) => 2,
            (b, _) => b,
        })
        .collect();
    // L1: trailing white space at the paragraph level
    for i in (0..n).rev() {
        if v[i] == ' ' || v[i] == '\t' {
            lv[i] = base;
        } else {
            break;
        }
    }
    lv
}

/// Shape and reorder `s` (one line) for drawing left to right. Text without
/// right-to-left characters is returned borrowed and unchanged.
pub fn visual(s: &str) -> Cow<'_, str> {
    if !has_rtl(s) {
        return Cow::Borrowed(s);
    }
    let rtl = rtl_para(s);
    Cow::Owned(visual_dir(s, rtl).into_iter().collect())
}

/// [`visual`] with the paragraph direction given (`rtl` = right-to-left).
pub fn visual_dir(s: &str, rtl: bool) -> Vec<char> {
    let mut v = shape_logical(s);
    let lv = levels(&v, rtl);
    // L2: reverse runs from the highest level down to the lowest odd one
    let hi = lv.iter().copied().max().unwrap_or(0);
    let lo_odd = lv.iter().copied().filter(|l| l % 2 == 1).min().unwrap_or(hi + 1);
    let mut lv2 = lv.clone();
    let mut l = hi;
    while l >= lo_odd && l > 0 {
        let mut i = 0;
        while i < v.len() {
            if lv2[i] >= l {
                let s = i;
                while i < v.len() && lv2[i] >= l {
                    i += 1;
                }
                v[s..i].reverse();
                lv2[s..i].reverse();
            } else {
                i += 1;
            }
        }
        l -= 1;
    }
    // L4: mirrored brackets in right-to-left runs
    for (c, &l) in v.iter_mut().zip(&lv2) {
        if l % 2 == 1 {
            *c = mirror(*c);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::String, vec, vec::Vec};

    fn hex(v: &str) -> Vec<u32> {
        v.chars().map(|c| c as u32).collect()
    }

    #[test]
    fn joins() {
        // سلام: seen initial, lam-alef final, meem final? (alef breaks it)
        let v = shape_logical("سلام");
        assert_eq!(v.iter().map(|&c| c as u32).collect::<Vec<_>>(), vec![0xFEB3, 0xFEFC, 0xFEE1]);
        // بيت: initial, medial, final
        assert_eq!(
            hex(&shape_logical("بيت").into_iter().collect::<String>()),
            vec![0xFE91, 0xFEF4, 0xFE96]
        );
        // harakat dropped
        assert_eq!(shape_logical("بَ").len(), 1);
        // a lone lam-alef is isolated
        assert_eq!(hex(&shape_logical("لا").into_iter().collect::<String>()), vec![0xFEFB]);
    }
    #[test]
    fn order() {
        // pure Arabic is reversed
        let s: String = visual("بيت").into_owned();
        assert_eq!(hex(&s), vec![0xFE96, 0xFEF4, 0xFE91]);
        // Latin stays
        assert_eq!(visual("hello"), "hello");
        // numbers keep their order inside Arabic
        let s = visual("رقم 123");
        assert!(s.starts_with("123 "), "{s}");
        // Latin inside an Arabic paragraph keeps its order
        let s = visual("ب abc ت");
        let t: String = s.chars().skip(2).take(3).collect();
        assert_eq!(t, "abc");
        // Arabic inside a Latin paragraph
        let s = visual("Hi بيت!");
        assert!(s.starts_with("Hi "));
        assert!(s.ends_with('!'));
        // brackets mirror in RTL
        let s = visual("(ب)");
        assert_eq!(s.chars().next(), Some('('));
    }

    fn shaped(s: &str) -> Vec<u32> {
        shape_logical(s).into_iter().map(|c| c as u32).collect()
    }

    #[test]
    fn lam_alef_ligatures() {
        // all four alef variants, isolated (nothing before)
        assert_eq!(shaped("\u{0644}\u{0622}"), [0xFEF5]);
        assert_eq!(shaped("\u{0644}\u{0623}"), [0xFEF7]);
        assert_eq!(shaped("\u{0644}\u{0625}"), [0xFEF9]);
        assert_eq!(shaped("\u{0644}\u{0627}"), [0xFEFB]);
        // joined to a previous letter: the final form is the next code point
        assert_eq!(shaped("\u{0628}\u{0644}\u{0627}"), [0xFE91, 0xFEFC]);
        assert_eq!(shaped("\u{0628}\u{0644}\u{0622}"), [0xFE91, 0xFEF6]);
        // after a non-joining letter (alef) the lam-alef stays isolated
        assert_eq!(shaped("\u{0627}\u{0644}\u{0627}"), [0xFE8D, 0xFEFB]);
        // lam followed by a non-alef is an ordinary lam
        assert_eq!(shaped("\u{0644}\u{0645}"), [0xFEDF, 0xFEE2]);
        // a lam at the end of the text does not ligate
        assert_eq!(shaped("\u{0628}\u{0644}"), [0xFE91, 0xFEDE]);
        // harakat between lam and alef are dropped first, so it still ligates
        assert_eq!(shaped("\u{0644}\u{064E}\u{0627}"), [0xFEFB]);
    }

    #[test]
    fn isolated_and_non_joining_letters() {
        // hamza never joins; dal/reh/waw join only to the previous letter
        assert_eq!(shaped("\u{0621}"), [0xFE80]);
        assert_eq!(shaped("\u{0628}\u{062F}\u{0628}"), [0xFE91, 0xFEAA, 0xFE8F]);
        assert_eq!(shaped("\u{0628}\u{0621}\u{0628}"), [0xFE8F, 0xFE80, 0xFE8F]);
        // tatweel (kashida) joins both sides and passes through
        assert_eq!(shaped("\u{0628}\u{0640}\u{062A}"), [0xFE91, 0x0640, 0xFE96]);
        // Persian kaf/yeh drawn as the Arabic ones; unknown text passes through
        assert_eq!(shaped("\u{06A9}"), [0xFED9]);
        assert_eq!(shaped("a1"), ['a' as u32, '1' as u32]);
        assert_eq!(shaped(""), Vec::<u32>::new());
    }

    #[test]
    fn rtl_detection() {
        assert!(!has_rtl("hello"));
        assert!(!has_rtl("caf\u{e9}"));
        assert!(has_rtl("a \u{0628}"));
        assert!(is_rtl_char('\u{05D0}'));
        assert!(!is_rtl_char('x'));
        assert!(rtl_para("\u{0628} abc"));
        assert!(!rtl_para("abc \u{0628}"));
        assert!(!rtl_para("123"));
    }

    #[test]
    fn plain_latin_is_borrowed() {
        assert!(matches!(visual("no arabic here"), Cow::Borrowed(_)));
        assert!(matches!(visual(""), Cow::Borrowed(_)));
    }

    #[test]
    fn visual_dir_forces_the_paragraph_direction() {
        // Latin text forced right-to-left: runs stay LTR but the order flips
        let v: String = visual_dir("ab \u{0628}", true).into_iter().collect();
        assert_eq!(v, "\u{FE8F} ab");
        let v: String = visual_dir("ab \u{0628}", false).into_iter().collect();
        assert_eq!(v, "ab \u{FE8F}");
    }

    #[test]
    fn lam_alef_word_in_visual_order() {
        // "سلام" shaped is 3 chars; reversed for drawing
        let s: Vec<u32> = visual("\u{0633}\u{0644}\u{0627}\u{0645}").chars().map(|c| c as u32).collect();
        assert_eq!(s, [0xFEE1, 0xFEFC, 0xFEB3]);
    }

    #[test]
    fn mirrored_brackets_and_trailing_space() {
        // brackets inside an RTL run are mirrored after reversal
        let s: Vec<char> = visual("\u{0628} (\u{0628})").chars().collect();
        assert_eq!(s.len(), 5);
        assert_eq!(s[0], '(');
        assert_eq!(s[4], '\u{FE8F}');
        // trailing whitespace stays at the paragraph's end
        let t = visual("\u{0628} ");
        assert!(t.ends_with(' ') || t.starts_with(' '));
    }
}
