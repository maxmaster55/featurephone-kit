//! SMS PDUs (3GPP TS 23.040 / TS 23.038) for modems that only speak PDU mode.
//!
//! Many cellular modems reject text-mode SMS (`AT+CMGS` answers
//! `+CMS ERROR: 303`, `AT+CMGL` answers `302`), so the host has to produce
//! SMS-SUBMIT PDUs and parse SMS-DELIVER / stored PDUs itself. This crate does
//! that:
//!
//! * [`encode`] builds SMS-SUBMIT PDUs: GSM 7-bit (default alphabet plus the
//!   escape/extension table, so `€ [ ] { } \ ^ ~ |` work) when every character
//!   fits, UCS-2 (UTF-16 big endian) otherwise, split into concatenated parts
//!   with a UDH (160/153 septets, 70/67 UTF-16 units) when too long.
//! * [`count`] is the compose counter ("57/160 - 1 part").
//! * [`decode`] reads SMS-DELIVER and SMS-SUBMIT PDUs back (with the SMSC
//!   field, as returned by `AT+CMGL`/`AT+CMGR`/`+CMT`), including 7-bit,
//!   8-bit and UCS-2 data, alphanumeric senders and concatenation headers.
//!
//! Splitting never cuts an escape pair or a surrogate pair in half. Text
//! outside the BMP (emoji) is sent as UTF-16 surrogate pairs, which modern
//! handsets display correctly although strict UCS-2 has no such characters.
//!
//! The crate is `no_std` (with `alloc`) when the default `std` feature is off.
//! Minimum supported Rust version: 1.74.
//!
//! # Example
//!
//! ```
//! let parts = smspdu::encode("+15555550123", "Hello [world] €5", 0);
//! assert_eq!(parts.len(), 1);
//! // `AT+CMGS=<tpdu_len>` then the hex string:
//! let (len, hex) = (parts[0].tpdu_len, &parts[0].hex);
//! assert_eq!(len * 2 + 2, hex.len());
//!
//! let back = smspdu::decode(hex).unwrap();
//! assert!(back.sent);
//! assert_eq!(back.number, "+15555550123");
//! assert_eq!(back.text, "Hello [world] €5");
//!
//! let c = smspdu::count("Hello");
//! assert_eq!((c.units, c.capacity, c.parts, c.unicode), (5, 160, 1, false));
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// The GSM 03.38 default alphabet; index = septet value. Position 0x1B is the
/// escape and is never produced as a character.
const GSM: &str = "@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞ\u{1b}ÆæßÉ !\"#¤%&'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà";
/// The extension table: character, septet after the escape (0x1B).
const EXT: [(char, u8); 10] = [
    ('^', 0x14),
    ('{', 0x28),
    ('}', 0x29),
    ('\\', 0x2F),
    ('[', 0x3C),
    ('~', 0x3D),
    (']', 0x3E),
    ('|', 0x40),
    ('€', 0x65),
    ('\u{c}', 0x0A),
];

const ESC: u8 = 0x1B;
/// Septets in a single-part GSM 7-bit message.
const SEPT_SINGLE: usize = 160;
/// Septets per part of a concatenated GSM 7-bit message (after a 6-byte UDH).
const SEPT_PART: usize = 153;
/// UTF-16 units in a single-part UCS-2 message.
const UCS2_SINGLE: usize = 70;
/// UTF-16 units per part of a concatenated UCS-2 message.
const UCS2_PART: usize = 67;

/// Append the septets for `c`; false when `c` is not in the GSM alphabet.
fn push_gsm(c: char, out: &mut Vec<u8>) -> bool {
    if let Some(i) = GSM.chars().position(|g| g == c) {
        if i as u8 == ESC {
            return false;
        }
        out.push(i as u8);
        return true;
    }
    match EXT.iter().find(|e| e.0 == c) {
        Some(e) => {
            out.extend_from_slice(&[ESC, e.1]);
            true
        }
        None => false,
    }
}

/// The text as GSM septets, or None if any character needs UCS-2.
fn septets(text: &str) -> Option<Vec<u8>> {
    let mut v = Vec::with_capacity(text.len());
    text.chars().all(|c| push_gsm(c, &mut v)).then_some(v)
}

/// Split septets: one part up to 160, else 153 per part, never between an
/// escape and its character.
fn split7(s: &[u8]) -> Vec<&[u8]> {
    if s.len() <= SEPT_SINGLE {
        return vec![s];
    }
    let mut chunks = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let mut e = (i + SEPT_PART).min(s.len());
        if e < s.len() && s[e - 1] == ESC {
            e -= 1;
        }
        chunks.push(&s[i..e]);
        i = e;
    }
    chunks
}

/// Split UTF-16 units: one part up to 70, else 67 per part, never between the
/// two halves of a surrogate pair.
fn split16(u: &[u16]) -> Vec<&[u16]> {
    if u.len() <= UCS2_SINGLE {
        return vec![u];
    }
    let mut chunks = Vec::new();
    let mut i = 0;
    while i < u.len() {
        let mut e = (i + UCS2_PART).min(u.len());
        if e < u.len() && (0xD800..0xDC00).contains(&u[e - 1]) {
            e -= 1;
        }
        chunks.push(&u[i..e]);
        i = e;
    }
    chunks
}

/// Result of [`count`]: the compose counter for a text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Count {
    /// Units used: GSM septets (an extension character counts 2) or UTF-16
    /// units.
    pub units: usize,
    /// Units that fit in the parts used so far (160 or `parts * 153` for
    /// GSM 7-bit, 70 or `parts * 67` for UCS-2). Pad bytes lost to avoiding a
    /// split escape/surrogate pair are not counted.
    pub capacity: usize,
    /// Number of SMS parts [`encode`] will produce (at least 1).
    pub parts: usize,
    /// True when the text needs UCS-2 (any character outside GSM 7-bit).
    pub unicode: bool,
}

/// The compose counter: GSM 7-bit allows 160 units in one part and 153 per
/// part after that; UCS-2 (Arabic, emoji, ...) 70, then 67 per part.
///
/// ```
/// let c = smspdu::count(&"a".repeat(161));
/// assert_eq!((c.parts, c.capacity, c.unicode), (2, 306, false));
/// assert!(smspdu::count("مرحبا").unicode);
/// ```
pub fn count(text: &str) -> Count {
    match septets(text) {
        Some(s) => {
            let parts = split7(&s).len();
            Count {
                units: s.len(),
                capacity: if parts == 1 { SEPT_SINGLE } else { parts * SEPT_PART },
                parts,
                unicode: false,
            }
        }
        None => {
            let u: Vec<u16> = text.encode_utf16().collect();
            let parts = split16(&u).len();
            Count {
                units: u.len(),
                capacity: if parts == 1 { UCS2_SINGLE } else { parts * UCS2_PART },
                parts,
                unicode: true,
            }
        }
    }
}

/// An encoded SMS-SUBMIT part, ready for `AT+CMGS` in PDU mode.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pdu {
    /// The TPDU length for `AT+CMGS=<len>` (octets after the SMSC field).
    pub tpdu_len: usize,
    /// The PDU as upper-case hex, with an empty SMSC field (`00`) so the
    /// SIM's service centre is used.
    pub hex: String,
}

/// Address field: (type of address, `len`+`toa`+swapped BCD digits as hex).
fn address(num: &str) -> String {
    let intl = num.starts_with('+');
    let d: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
    let mut s = d.clone();
    if s.len() % 2 == 1 {
        s.push('F');
    }
    let b: Vec<char> = s.chars().collect();
    let swapped: String = b.chunks(2).map(|p| format!("{}{}", p[1], p[0])).collect();
    format!("{:02X}{:02X}{swapped}", d.len(), if intl { 0x91 } else { 0x81 })
}

/// Septets packed LSB-first after `fill` padding bits (fill > 0 after a UDH).
fn pack7(sept: &[u8], fill: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut n) = (0u32, fill);
    for &s in sept {
        acc |= (s as u32 & 0x7f) << n;
        n += 7;
        while n >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            n -= 8;
        }
    }
    if n > 0 {
        out.push(acc as u8);
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

/// Encode `text` for `num` as SMS-SUBMIT PDUs: one per part, in order.
///
/// `num` is a phone number, international when it starts with `+` (non-digits
/// other than the leading `+` are ignored). `mref` is the concatenation
/// reference shared by all parts of a long message (use a different value for
/// each long message you send to the same recipient). The status-report
/// request is off and validity is the relative value `AA` (4 days).
pub fn encode(num: &str, text: &str, mref: u8) -> Vec<Pdu> {
    let da = address(num);
    // (user data bytes, UDL, DCS)
    let mut parts: Vec<(Vec<u8>, usize, u8)> = Vec::new();
    match septets(text) {
        Some(s) => {
            let chunks = split7(&s);
            if chunks.len() == 1 {
                parts.push((pack7(chunks[0], 0), chunks[0].len(), 0x00));
            } else {
                let n = chunks.len() as u8;
                for (k, c) in chunks.iter().enumerate() {
                    let mut ud = vec![5u8, 0, 3, mref, n, k as u8 + 1];
                    ud.extend(pack7(c, 1)); // 6 header bytes = 48 bits: 1 fill bit
                    parts.push((ud, 7 + c.len(), 0x00));
                }
            }
        }
        None => {
            let u: Vec<u16> = text.encode_utf16().collect();
            let chunks = split16(&u);
            let multi = chunks.len() > 1;
            let n = chunks.len() as u8;
            for (k, c) in chunks.iter().enumerate() {
                let mut ud = if multi { vec![5u8, 0, 3, mref, n, k as u8 + 1] } else { Vec::new() };
                ud.extend(c.iter().flat_map(|x| x.to_be_bytes()));
                let l = ud.len();
                parts.push((ud, l, 0x08));
            }
        }
    }
    let multi = parts.len() > 1;
    parts
        .into_iter()
        .map(|(ud, udl, dcs)| {
            // first octet: SMS-SUBMIT, relative validity, UDHI when split
            let fo = 0x11 | if multi { 0x40 } else { 0 };
            let tp = format!("{fo:02X}00{da}00{dcs:02X}AA{udl:02X}{}", hex(&ud));
            Pdu {
                tpdu_len: tp.len() / 2,
                hex: format!("00{tp}"),
            }
        })
        .collect()
}

// ---------------------------------------------------------------- reading

/// Concatenation info of one part of a split message.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Part {
    /// Reference number shared by all parts (8 or 16 bit).
    pub reference: u16,
    /// Total number of parts.
    pub total: u8,
    /// This part, starting at 1.
    pub index: u8,
}

/// A decoded PDU (see [`decode`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Decoded {
    /// The other party: destination for SMS-SUBMIT, sender for SMS-DELIVER
    /// (`+` prefixed when international; text for alphanumeric senders).
    pub number: String,
    /// Service-centre timestamp as `YY/MM/DD,hh:mm:ss` (time-zone ignored);
    /// empty for SMS-SUBMIT.
    pub date: String,
    /// The message text (of this part only, for split messages).
    pub text: String,
    /// Concatenation info, when the PDU is one part of a split message.
    pub part: Option<Part>,
    /// True for SMS-SUBMIT (outgoing), false for SMS-DELIVER (incoming).
    pub sent: bool,
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok())
        .collect()
}

fn unpack7(b: &[u8], fill: u32, n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut it = b.iter();
    let mut skip = fill;
    while out.len() < n {
        if bits < 7 + skip {
            match it.next() {
                Some(&x) => {
                    acc |= (x as u32) << bits;
                    bits += 8;
                    continue;
                }
                None => break,
            }
        }
        if skip > 0 {
            acc >>= skip;
            bits -= skip;
            skip = 0;
            continue;
        }
        out.push((acc & 0x7f) as u8);
        acc >>= 7;
        bits -= 7;
    }
    out
}

fn gsm_text(s: &[u8]) -> String {
    let tab: Vec<char> = GSM.chars().collect();
    let mut out = String::new();
    let mut esc = false;
    for &c in s {
        if esc {
            out.push(EXT.iter().find(|e| e.1 == c).map_or(' ', |e| e.0));
            esc = false;
        } else if c == ESC {
            esc = true;
        } else {
            out.push(*tab.get(c as usize).unwrap_or(&'?'));
        }
    }
    out
}

fn bcd_num(b: &[u8], digits: usize, toa: u8) -> String {
    if toa & 0x70 == 0x50 {
        // alphanumeric sender (e.g. "Vodafone")
        return gsm_text(&unpack7(b, 0, digits * 4 / 7));
    }
    let mut s = String::new();
    if toa & 0x70 == 0x10 {
        s.push('+');
    }
    for x in b {
        for d in [x & 15, x >> 4] {
            if s.trim_start_matches('+').len() < digits && d < 10 {
                s.push((b'0' + d) as char);
            }
        }
    }
    s
}

/// Decode a PDU as returned by `AT+CMGL` / `AT+CMGR` / `+CMT` (hex, starting
/// with the SMSC field) or as produced by [`encode`].
///
/// Handles SMS-DELIVER and SMS-SUBMIT; returns `None` when the hex is invalid
/// or the PDU is truncated. 8-bit data (DCS class 4) is read as UTF-8 (lossy).
pub fn decode(h: &str) -> Option<Decoded> {
    let b = unhex(h)?;
    let mut i = 1 + *b.first()? as usize; // skip the SMSC
    let fo = *b.get(i)?;
    i += 1;
    let sent = fo & 3 == 1;
    if sent {
        i += 1; // message reference
    }
    let digits = *b.get(i)? as usize;
    let toa = *b.get(i + 1)?;
    let nb = digits.div_ceil(2);
    let number = bcd_num(b.get(i + 2..i + 2 + nb)?, digits, toa);
    i += 2 + nb;
    i += 1; // PID
    let dcs = *b.get(i)?;
    i += 1;
    let mut date = String::new();
    if sent {
        // validity period when present
        match (fo >> 3) & 3 {
            2 => i += 1,
            1 | 3 => i += 7,
            _ => {}
        }
    } else {
        let ts = b.get(i..i + 7)?;
        let d = |x: u8| (x & 15) * 10 + (x >> 4);
        date = format!(
            "{:02}/{:02}/{:02},{:02}:{:02}:{:02}",
            d(ts[0]),
            d(ts[1]),
            d(ts[2]),
            d(ts[3]),
            d(ts[4]),
            d(ts[5])
        );
        i += 7;
    }
    let udl = *b.get(i)? as usize;
    i += 1;
    let ud = b.get(i..)?;
    let mut part = None;
    let mut hdr = 0usize;
    if fo & 0x40 != 0 {
        let l = *ud.first()? as usize;
        hdr = l + 1;
        let mut k = 1;
        while k + 1 < hdr {
            let (id, il) = (*ud.get(k)?, *ud.get(k + 1)? as usize);
            let v = ud.get(k + 2..k + 2 + il)?;
            if id == 0 && il == 3 {
                part = Some(Part {
                    reference: v[0] as u16,
                    total: v[1],
                    index: v[2],
                });
            } else if id == 8 && il == 4 {
                part = Some(Part {
                    reference: u16::from_be_bytes([v[0], v[1]]),
                    total: v[2],
                    index: v[3],
                });
            }
            k += 2 + il;
        }
    }
    let alpha = if dcs & 0xC0 == 0 {
        (dcs >> 2) & 3
    } else if dcs & 0xF0 == 0xF0 {
        (dcs >> 2) & 1
    } else {
        0
    };
    let text = match alpha {
        2 => {
            let u: Vec<u16> = ud
                .get(hdr..udl.min(ud.len()))?
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u)
        }
        1 => String::from_utf8_lossy(ud.get(hdr..udl.min(ud.len()))?).into_owned(),
        _ => {
            let fill = if hdr > 0 { ((7 - (hdr * 8) % 7) % 7) as u32 } else { 0 };
            let skip_sept = if hdr > 0 { (hdr * 8).div_ceil(7) } else { 0 };
            gsm_text(&unpack7(ud.get(hdr..)?, fill, udl.saturating_sub(skip_sept)))
        }
    };
    Some(Decoded {
        number,
        date,
        text,
        part,
        sent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NUM: &str = "+15555550123";

    /// Encode, decode every part, and join the text.
    fn round_trip(num: &str, t: &str) -> (Vec<Pdu>, String) {
        let parts = encode(num, t, 7);
        let mut s = String::new();
        for p in &parts {
            let g = decode(&p.hex).unwrap();
            assert!(g.sent);
            assert_eq!(g.number, num);
            s.push_str(&g.text);
        }
        (parts, s)
    }

    #[test]
    fn plain_matches_a_known_pdu() {
        // classic example from PDU primers: "hellohello" to +46708251358
        let p = encode("+46708251358", "hellohello", 0);
        assert_eq!(
            p,
            vec![Pdu {
                tpdu_len: 23,
                hex: "0011000B916407281553F80000AA0AE8329BFD4697D9EC37".into()
            }]
        );
    }

    #[test]
    fn plain_message_exact_pdu() {
        let p = encode(NUM, "WeaverOS test SMS: sending works.", 0);
        // 00 SMSC | 11 SUBMIT | 00 MR | 0B 91 +11 digits, nibble-swapped | 00 PID | 00 DCS | AA VP | 21 UDL | packed septets
        let want = "0011000B915155550521F30000AA21D772D85E963FA7207A794E074D9B531D685E7693D3EE33E8FE96AFE72E";
        assert_eq!(
            p,
            vec![Pdu {
                tpdu_len: 43,
                hex: want.into()
            }]
        );
    }

    #[test]
    fn round_trips() {
        for t in [
            "hello [world] €5",
            "مرحبا بك",
            &"long message ".repeat(30),
            &"عربي ".repeat(40),
            "",
            "😀",
        ] {
            let (_, s) = round_trip(NUM, t);
            assert_eq!(s, t);
        }
    }

    #[test]
    fn address_forms() {
        // national, odd digit count padded with F; even count
        let p = encode("5551234", "x", 0);
        assert!(p[0].hex.starts_with("0011000781551532F4"), "{}", p[0].hex);
        assert_eq!(round_trip("5551234", "x").1, "x");
        assert_eq!(round_trip("0123456789", "x").1, "x");
        let d = decode(&p[0].hex).unwrap();
        assert_eq!(d.number, "5551234");
    }

    #[test]
    fn gsm_extension_table() {
        for (c, code) in EXT {
            let t = format!("a{c}b");
            let c0 = count(&t);
            assert_eq!((c0.units, c0.unicode), (4, false), "{c:?}");
            let mut s = Vec::new();
            assert!(push_gsm(c, &mut s));
            assert_eq!(s, [ESC, code]);
            assert_eq!(round_trip(NUM, &t).1, t);
        }
        assert_eq!(count("€").units, 2);
        // ESC itself is not a sendable character: the text goes UCS-2
        assert!(count("a\u{1b}").unicode);
        // characters of the basic alphabet, including accents and Greek
        let all = "@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉ !\"#¤%&'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà";
        assert!(!count(all).unicode);
        assert_eq!(round_trip(NUM, all).1, all);
    }

    #[test]
    fn a_160_septet_message_is_one_part_161_is_two() {
        let c = count(&"a".repeat(160));
        assert_eq!((c.units, c.capacity, c.parts), (160, 160, 1));
        let c = count(&"a".repeat(161));
        assert_eq!((c.units, c.capacity, c.parts), (161, 306, 2));
        assert_eq!(count(&"a".repeat(306)).parts, 2);
        assert_eq!(count(&"a".repeat(307)).parts, 3);
        assert_eq!(count("").parts, 1);

        assert_eq!(encode(NUM, &"a".repeat(160), 1).len(), 1);
        let t = "b".repeat(307);
        let (parts, s) = round_trip(NUM, &t);
        assert_eq!(parts.len(), 3);
        assert_eq!(s, t);
        for (k, p) in parts.iter().enumerate() {
            let g = decode(&p.hex).unwrap();
            assert_eq!(
                g.part,
                Some(Part {
                    reference: 7,
                    total: 3,
                    index: k as u8 + 1
                })
            );
            assert!(g.text.len() <= 153);
        }
    }

    #[test]
    fn escape_pair_is_never_split() {
        // the euro sign would straddle the 153 boundary
        let t = format!("{}€{}", "a".repeat(152), "a".repeat(20));
        let (parts, s) = round_trip(NUM, &t);
        assert_eq!(s, t);
        assert_eq!(parts.len(), count(&t).parts);
        let first = decode(&parts[0].hex).unwrap();
        assert_eq!(first.text, "a".repeat(152));
        assert_eq!(decode(&parts[1].hex).unwrap().text, format!("€{}", "a".repeat(20)));
    }

    #[test]
    fn ucs2_split_70_and_67() {
        let c = count(&"ع".repeat(70));
        assert_eq!((c.units, c.capacity, c.parts, c.unicode), (70, 70, 1, true));
        let c = count(&"ع".repeat(71));
        assert_eq!((c.capacity, c.parts), (134, 2));
        let t = "ع".repeat(134);
        let (parts, s) = round_trip(NUM, &t);
        assert_eq!(parts.len(), 2);
        assert_eq!(s, t);
        assert_eq!(decode(&parts[0].hex).unwrap().text.chars().count(), 67);
        assert_eq!(count(&"ع".repeat(135)).parts, 3);
    }

    #[test]
    fn ucs2_surrogate_pair_is_never_split() {
        // an emoji is two UTF-16 units; put it on the 67-unit boundary
        let t = format!("{}😀{}", "ع".repeat(66), "ع".repeat(10));
        assert_eq!(count(&t).units, 78);
        let (parts, s) = round_trip(NUM, &t);
        assert_eq!(s, t);
        assert_eq!(parts.len(), 2);
        let first = decode(&parts[0].hex).unwrap();
        assert_eq!(first.text.chars().count(), 66); // the pair moved to part 2
        assert!(decode(&parts[1].hex).unwrap().text.starts_with('😀'));
        // a lone emoji message is 2 units, UCS-2
        let c = count("😀");
        assert_eq!((c.units, c.unicode), (2, true));
    }

    #[test]
    fn single_part_has_no_udh() {
        let p = &encode(NUM, "hi", 9)[0];
        assert_eq!(&p.hex[2..4], "11");
        let g = decode(&p.hex).unwrap();
        assert_eq!(g.part, None);
        assert_eq!(g.date, "");
    }

    #[test]
    fn decodes_a_deliver_pdu() {
        // SMS-DELIVER from the PDU primers: sender 27838890001, "hellohello"
        let g = decode("07917238010010F5040BC87238880900F10000993092516195800AE8329BFD4697D9EC37").unwrap();
        assert!(!g.sent);
        assert_eq!(g.number, "27838890001");
        assert_eq!(g.text, "hellohello");
        assert_eq!(g.date, "99/03/29,15:16:59");
    }

    /// A DELIVER PDU (SMSC 00) from `+15555550123`, timestamp 2024-02-03 04:05:06.
    fn deliver(fo: u8, dcs: u8, udl: usize, ud: &str) -> String {
        format!("00{fo:02X}0B915155550521F300{dcs:02X}42203040506000{udl:02X}{ud}")
    }

    #[test]
    fn decodes_ucs2_deliver_with_international_sender() {
        let body = hex(&"مرحبا".encode_utf16().flat_map(|u| u.to_be_bytes()).collect::<Vec<_>>());
        let g = decode(&deliver(0x04, 0x08, 10, &body)).unwrap();
        assert_eq!((g.number.as_str(), g.text.as_str()), (NUM, "مرحبا"));
        assert_eq!(g.date, "24/02/03,04:05:06");
    }

    #[test]
    fn decodes_an_alphanumeric_sender() {
        // sender "Acme": 4 septets = 28 bits = 7 semi-octets, type of address D0
        let name = hex(&pack7(&septets("Acme").unwrap(), 0));
        let text = hex(&pack7(&septets("Hi").unwrap(), 0));
        let g = decode(&format!("000407D0{name}000042203040506000{}{text}", "02")).unwrap();
        assert_eq!(g.number, "Acme");
        assert_eq!(g.text, "Hi");
    }

    #[test]
    fn sixteen_bit_concatenation_reference() {
        // UDH with IEI 8: reference 0x0102, 3 parts, this is part 2; data U+0639
        let g = decode(&deliver(0x44, 0x08, 9, "060804010203020639")).unwrap();
        assert_eq!(
            g.part,
            Some(Part {
                reference: 0x0102,
                total: 3,
                index: 2
            })
        );
        assert_eq!(g.text, "\u{0639}");
    }

    #[test]
    fn bad_input_is_none() {
        assert_eq!(decode(""), None);
        assert_eq!(decode("zz"), None);
        assert_eq!(decode("0"), None);
        assert_eq!(decode("00"), None);
        assert_eq!(decode("0011000C91"), None);
        let ok = &encode(NUM, &"é".repeat(5), 0)[0].hex;
        // every truncation either decodes or is None, but never panics
        for n in 0..ok.len() {
            let _ = decode(&ok[..n]);
        }
    }
}
