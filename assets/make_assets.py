#!/usr/bin/env python3
"""Regenerate the README images in this directory.

    python3 assets/make_assets.py

Every image is drawn from the crates' real output: the script runs the
examples below with `cargo run` and draws what they print.

  t9-multitap.gif      crates/t9-input/examples/multitap_trace.rs
  arabic-shaping.png   crates/arabic-shape/examples/shape_word.rs
  sms-pdu-anatomy.svg  crates/smspdu/examples/pdu_fields.rs
  sms-concat.svg       crates/smspdu/examples/pdu_fields.rs

Needs Python 3 with Pillow, cargo, and the DejaVu Sans and Noto Naskh Arabic
fonts (found with fc-match; the fonts are not part of this repository).
"""

import subprocess
import sys
import unicodedata
from html import escape
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets"

# One dark palette for every image, so they sit well on GitHub's light and
# dark themes alike (each image carries its own solid background).
BG = "#0f172a"
PANEL = "#1e293b"
LINE = "#334155"
MUTED = "#94a3b8"
TEXT = "#e2e8f0"
ACCENT = "#38bdf8"
SANS = "system-ui,-apple-system,'Segoe UI',Helvetica,Arial,sans-serif"
MONO = "ui-monospace,SFMono-Regular,Menlo,Consolas,'DejaVu Sans Mono',monospace"


def cargo_example(package, example, *args):
    cmd = ["cargo", "run", "-q", "-p", package, "--example", example, "--", *args]
    return subprocess.run(cmd, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def font_file(pattern):
    path = subprocess.run(
        ["fc-match", "-f", "%{file}", pattern], check=True, capture_output=True, text=True
    ).stdout
    if not path:
        sys.exit(f"font not found: {pattern}")
    return path


def font(pattern, size):
    # BASIC layout: draw code points one glyph each, like a bitmap-font
    # renderer (no OpenType shaping behind our back).
    return ImageFont.truetype(font_file(pattern), size, layout_engine=ImageFont.Layout.BASIC)


# --------------------------------------------------------------- t9 multitap

KEYS = [
    ("1", ".,?!"), ("2", "abc"), ("3", "def"),
    ("4", "ghi"), ("5", "jkl"), ("6", "mno"),
    ("7", "pqrs"), ("8", "tuv"), ("9", "wxyz"),
    ("*", "sym"), ("0", "space"), ("#", "mode"),
]


def t9_gif():
    lines = cargo_example("t9-input", "multitap_trace").splitlines()
    meta = dict(kv.split("=") for kv in lines[0].lstrip("# ").split())
    timeout = int(meta["timeout_ms"])
    rows = [ln.split("\t") for ln in lines[1:]]

    W, H = 640, 400
    sans = font("DejaVu Sans", 15)
    sans_s = font("DejaVu Sans", 12)
    sans_b = font("DejaVu Sans:style=Bold", 15)
    big = font("DejaVu Sans", 22)
    mono = font("DejaVu Sans Mono", 12)
    key_d = font("DejaVu Sans:style=Bold", 20)
    hint = font("DejaVu Sans Mono:style=Bold", 18)

    lcd_bg, lcd_ink, lcd_dim, lcd_sel = "#f8fafc", "#0f172a", "#64748b", "#2563eb"
    sx, sy = 36, 40  # the 240x320 screen

    last_press, log, frames = None, [], []
    for t, key, text, pend, idx, edit in rows:
        t = int(t)
        if key != "-":
            last_press = (t, key)
            log.append(f"{t / 1000:4.2f}s  key {key}  {edit}")
        elif last_press and pend == "-" and log and not log[-1].endswith("commit"):
            log.append(f"{t / 1000:4.2f}s  timeout   commit")

        im = Image.new("RGB", (W, H), BG)
        d = ImageDraw.Draw(im)

        # phone body and screen
        d.rounded_rectangle((sx - 16, sy - 24, sx + 256, sy + 336), 22, fill=PANEL, outline=LINE)
        d.rectangle((sx, sy, sx + 239, sy + 319), fill=lcd_bg)
        d.rectangle((sx, sy, sx + 239, sy + 19), fill="#e2e8f0")
        d.text((sx + 6, sy + 3), "12:00", font=sans_s, fill=lcd_ink)
        for i in range(4):  # signal bars
            d.rectangle((sx + 200 + i * 5, sy + 14 - i * 3, sx + 202 + i * 5, sy + 15), fill=lcd_ink)
        d.rectangle((sx + 224, sy + 5, sx + 234, sy + 14), outline=lcd_ink)
        d.text((sx + 10, sy + 28), "New message", font=sans_b, fill=lcd_ink)
        d.text((sx + 10, sy + 50), "To: +1 555 555 0123", font=sans_s, fill=lcd_dim)
        d.line((sx + 10, sy + 70, sx + 229, sy + 70), fill="#cbd5e1")

        # text with the open tap's letter highlighted, then the cursor
        x, y = sx + 12, sy + 82
        committed = text[:-1] if pend != "-" else text
        d.text((x, y), committed, font=big, fill=lcd_ink)
        x += d.textlength(committed, font=big)
        if pend != "-":
            w = d.textlength(text[-1], font=big)
            d.rectangle((x, y + 1, x + w + 1, y + 28), fill=lcd_sel)
            d.text((x + 1, y), text[-1], font=big, fill="white")
            x += w + 2
        d.rectangle((x + 1, y + 2, x + 2, y + 28), fill=lcd_ink)
        counter = f"{len(text)}/160"
        d.text((sx + 228 - d.textlength(counter, font=sans_s), sy + 55), counter, font=sans_s, fill=lcd_dim)

        # hint row: the letters of the open tap and the remaining timeout
        hy = sy + 236
        if pend != "-":
            cw = 30
            x0 = sx + 120 - cw * len(pend) / 2
            for i, ch in enumerate(pend):
                bx = x0 + i * cw
                sel = i == int(idx)
                d.rounded_rectangle((bx + 2, hy, bx + cw - 2, hy + 28), 5,
                                    fill=lcd_sel if sel else "#e2e8f0")
                d.text((bx + cw / 2, hy + 14), ch, font=hint, anchor="mm",
                       fill="white" if sel else lcd_ink)
            left = max(0.0, 1 - (t - last_press[0]) / timeout)
            d.rectangle((sx + 20, hy + 36, sx + 220, hy + 39), fill="#e2e8f0")
            d.rectangle((sx + 20, hy + 36, sx + 20 + 200 * left, hy + 39), fill=lcd_sel)
        # soft-key bar with the mode label
        d.rectangle((sx, sy + 296, sx + 239, sy + 319), fill="#e2e8f0")
        d.text((sx + 8, sy + 300), "Options", font=sans_s, fill=lcd_ink)
        d.text((sx + 120, sy + 307), meta["mode"], font=sans_b, anchor="mm", fill=lcd_sel)
        d.text((sx + 232, sy + 300), "Back", font=sans_s, anchor="ra", fill=lcd_ink)

        # keypad
        kx, ky, kw, kh = 340, 40, 84, 50
        down = last_press and t - last_press[0] < 200
        for i, (dig, letters) in enumerate(KEYS):
            x0 = kx + (i % 3) * (kw + 8)
            y0 = ky + (i // 3) * (kh + 8)
            active = last_press and dig == last_press[1] and pend != "-"
            fill = ACCENT if (down and dig == last_press[1]) else PANEL
            d.rounded_rectangle((x0, y0, x0 + kw, y0 + kh), 10, fill=fill,
                                outline=ACCENT if active else LINE, width=2 if active else 1)
            ink = BG if fill == ACCENT else TEXT
            d.text((x0 + 14, y0 + kh / 2), dig, font=key_d, anchor="lm", fill=ink)
            d.text((x0 + 36, y0 + kh / 2), letters, font=sans_s, anchor="lm",
                   fill=BG if fill == ACCENT else MUTED)

        # event log
        ly = ky + 4 * (kh + 8) + 8
        d.text((kx, ly), "MultiTap events", font=sans_s, fill=MUTED)
        for i, entry in enumerate(log[-4:]):
            d.text((kx, ly + 20 + i * 17), entry, font=mono, fill=TEXT if i == len(log[-4:]) - 1 else MUTED)

        d.text((W / 2, H - 14), f"t9-input  ·  multi-tap, {timeout} ms timeout", font=sans_s,
               anchor="mm", fill=MUTED)
        frames.append(im)

    # Merge identical consecutive frames; hold the last one before looping.
    step = int(rows[1][0]) - int(rows[0][0])
    out, durations = [], []
    for im in frames:
        if out and im.tobytes() == out[-1].tobytes():
            durations[-1] += step
        else:
            out.append(im)
            durations.append(step)
    durations[-1] += 1500
    # one palette for all frames (no flicker), built from a sample of them
    sample = out[::4]
    sheet = Image.new("RGB", (W, H * len(sample)))
    for i, im in enumerate(sample):
        sheet.paste(im, (0, H * i))
    pal = sheet.quantize(colors=64, method=Image.Quantize.MEDIANCUT)
    out = [im.quantize(palette=pal, dither=Image.Dither.NONE) for im in out]
    path = OUT / "t9-multitap.gif"
    out[0].save(path, save_all=True, append_images=out[1:], duration=durations, loop=0, optimize=True)
    return path


# ------------------------------------------------------------ arabic shaping

def form_name(cp):
    name = unicodedata.name(chr(cp), f"U+{cp:04X}")
    if name.startswith("ARABIC LETTER "):
        return name[len("ARABIC LETTER "):].lower().removesuffix(" form")
    if name.startswith("ARABIC LIGATURE LAM WITH ALEF"):
        return "lam-alef " + name.split()[-2].lower()  # ... FINAL FORM -> final
    return name.lower()


def arabic_png():
    stages = {}
    for line in cargo_example("arabic-shape", "shape_word").splitlines():
        k, *cps = line.split()
        stages[k] = [int(c, 16) for c in cps]

    S = 2  # draw at 2x, downsample for smooth edges
    W, H = 900, 470
    im = Image.new("RGB", (W * S, H * S), BG)
    d = ImageDraw.Draw(im)
    ar = font("Noto Naskh Arabic", 34 * S)
    ar_big = font("Noto Naskh Arabic", 64 * S)
    lab = font("DejaVu Sans", 12 * S)
    lab_b = font("DejaVu Sans:style=Bold", 14 * S)
    code = font("DejaVu Sans Mono", 11 * S)

    def boxes(y, cps, title, sub):
        d.text((24 * S, y * S), title, font=lab_b, fill=TEXT)
        d.text((24 * S, (y + 20) * S), sub, font=lab, fill=MUTED)
        bw, gap = 106, 8
        x0 = 210
        centers = []
        for i, cp in enumerate(cps):
            x = x0 + i * (bw + gap)
            d.rounded_rectangle((x * S, y * S, (x + bw) * S, (y + 92) * S), 10 * S, fill=PANEL, outline=LINE)
            d.text(((x + bw / 2) * S, (y + 36) * S), chr(cp), font=ar, anchor="mm", fill=TEXT)
            d.text(((x + bw / 2) * S, (y + 66) * S), f"U+{cp:04X}", font=code, anchor="mm", fill=ACCENT)
            d.text(((x + bw / 2) * S, (y + 82) * S), form_name(cp), font=code, anchor="mm", fill=MUTED)
            centers.append(x + bw / 2)
        return centers

    top = boxes(24, stages["logical"], "Logical text", "as typed and stored")
    bot = boxes(176, stages["shaped"], "shape_logical()", "presentation forms")
    # connect each input letter to the form it became (lam + alef -> one ligature)
    j = 0
    for i, cp in enumerate(stages["shaped"]):
        srcs = [j, j + 1] if "lam-alef" in form_name(cp) else [j]
        for s in srcs:
            d.line(((top[s]) * S, 118 * S, bot[i] * S, 174 * S), fill=LINE, width=2 * S)
        j += len(srcs)

    # the visual line, drawn glyph by glyph left to right like a bitmap renderer
    y = 330
    d.text((24 * S, y * S), "visual()", font=lab_b, fill=TEXT)
    d.text((24 * S, (y + 20) * S), "drawn left to right", font=lab, fill=MUTED)
    d.rounded_rectangle((210 * S, (y - 6) * S, (W - 24) * S, (y + 112) * S), 10 * S, fill=PANEL, outline=LINE)
    glyphs = [chr(c) for c in stages["visual"]]
    total = sum(ar_big.getlength(g) for g in glyphs)
    x = (210 + (W - 24 - 210) / 2) * S - total / 2
    for g in glyphs:
        d.text((x, (y + 66) * S), g, font=ar_big, anchor="ls", fill=TEXT)
        x += ar_big.getlength(g)
    order = "  ".join(f"{c:04X}" for c in stages["visual"])
    d.text(((210 + (W - 24 - 210) / 2) * S, (y + 96) * S), order, font=code, anchor="mm", fill=ACCENT)

    path = OUT / "arabic-shaping.png"
    im.resize((W, H), Image.LANCZOS).save(path, optimize=True)
    return path


# -------------------------------------------------------------------- smspdu

def sms_data():
    """[(text, count fields, [(tpdu_len, hex), ...]), ...] from the example."""
    msgs = []
    for line in cargo_example("smspdu", "pdu_fields").splitlines():
        k, rest = line.split(" ", 1)
        if k == "text":
            msgs.append((rest, None, []))
        elif k == "count":
            msgs[-1] = (msgs[-1][0], rest.split(), [])
        else:
            n, h = rest.split()
            msgs[-1][2].append((int(n), h))
    return msgs


def svg(w, h, body):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" '
        f'font-family="{SANS}" font-size="13">\n'
        f'<rect width="{w}" height="{h}" rx="12" fill="{BG}"/>\n{body}</svg>\n'
    )


def split_submit(hexs):
    """Split an SMS-SUBMIT PDU (as encode() writes it) into its fields."""
    fields, i = [], 0

    def take(n_hex):
        nonlocal i
        s = hexs[i:i + n_hex]
        i += n_hex
        return s

    smsc = take(2)
    fo = take(2)
    mr = take(2)
    da_len = take(2)
    digits = int(da_len, 16)
    da_toa = take(2)
    da_num = take(digits + digits % 2)
    pid, dcs, vp, udl = take(2), take(2), take(2), take(2)
    ud = hexs[i:]
    number = "".join(da_num[k + 1] + da_num[k] for k in range(0, len(da_num), 2)).rstrip("F")
    plus = "+" if da_toa == "91" else ""
    fields = [
        ("SMSC", smsc, "#94a3b8", "service centre length 0: use the SIM's SMSC"),
        ("First octet", fo, "#fbbf24", "SMS-SUBMIT, relative validity period"
         + (", UDH present" if int(fo, 16) & 0x40 else "")),
        ("MR", mr, "#f87171", "message reference"),
        ("DA length", da_len, "#60a5fa", f"{digits} digits in the destination address"),
        ("DA type", da_toa, "#60a5fa", "international number (TON 1, ISDN plan)" if plus else "national"),
        ("DA digits", da_num, "#60a5fa", f"swapped BCD, F pad: {plus}{number}"),
        ("PID", pid, "#c084fc", "protocol identifier: plain SMS"),
        ("DCS", dcs, "#f472b6", "GSM 7-bit default alphabet" if dcs == "00" else "UCS-2"),
        ("VP", vp, "#2dd4bf", "validity period: relative, 4 days"),
        ("UDL", udl, "#a3e635", f"{int(udl, 16)} septets of user data"),
        ("UD", ud, "#fb923c", f"{len(ud) // 2} octets of packed 7-bit text"),
    ]
    return fields


def pdu_svg(text, count, pdu):
    tpdu_len, hexs = pdu
    fields = split_submit(hexs)
    assert "".join(f[1] for f in fields) == hexs
    W = 860
    rows = []
    rows.append(f'<text x="24" y="38" font-size="17" font-weight="600" fill="{TEXT}">SMS-SUBMIT PDU anatomy</text>')
    rows.append(
        f'<text x="24" y="60" fill="{MUTED}">smspdu::encode("+15555550123", "{escape(text)}", 0)'
        f'  then AT+CMGS={tpdu_len}</text>'
    )
    # the whole PDU, one cell per octet, coloured by field
    octets = [(h[k:k + 2], c) for _, h, c, _ in fields for k in range(0, len(h), 2)]
    cell = (W - 48) / len(octets)
    rows.append(f'<rect x="16" y="76" width="{W - 32}" height="40" rx="8" fill="{PANEL}" stroke="{LINE}"/>')
    for k, (o, c) in enumerate(octets):
        rows.append(f'<text x="{24 + (k + 0.5) * cell:.1f}" y="101" text-anchor="middle" '
                    f'font-family="{MONO}" font-size="14" fill="{c}">{o}</text>')
    # one row per field
    y = 146
    for name, h, colour, meaning in fields:
        rows.append(f'<rect x="24" y="{y - 13}" width="6" height="18" rx="2" fill="{colour}"/>')
        rows.append(f'<text x="40" y="{y}" font-weight="600" fill="{TEXT}">{name}</text>')
        shown = h if len(h) <= 26 else h[:24] + "…"
        rows.append(f'<text x="140" y="{y}" font-family="{MONO}" fill="{colour}">{shown}</text>')
        rows.append(f'<text x="380" y="{y}" fill="{MUTED}">{escape(meaning)}</text>')
        y += 27
    units = int(count[0])
    rows.append(
        f'<text x="24" y="{y + 6}" font-size="12" fill="{MUTED}">'
        f'{units} septets for {len(text)} characters: [ ] and € are escape pairs (ESC + code) '
        f'from the GSM 7-bit extension table.</text>'
    )
    return svg(W, y + 26, "\n".join(rows) + "\n")


def concat_svg(text, count, pdus):
    units, capacity, parts = int(count[0]), int(count[1]), int(count[2])
    W = 860
    x0, bar_w = 190, 640  # 140 octets = 1120 bits across bar_w
    px = bar_w / 1120
    out = [
        f'<text x="24" y="38" font-size="17" font-weight="600" fill="{TEXT}">'
        f'Every SMS carries 140 octets of user data</text>',
        f'<text x="24" y="60" fill="{MUTED}">GSM 7-bit: 160 septets fit one SMS. A split message spends '
        f'7 septets per part on its header, leaving 153.</text>',
    ]

    def seg(y, x, w, fill, label, ink=BG):
        out.append(f'<rect x="{x:.1f}" y="{y}" width="{w:.1f}" height="34" rx="4" fill="{fill}"/>')
        if label:
            out.append(f'<text x="{x + w / 2:.1f}" y="{y + 22}" text-anchor="middle" font-size="12" '
                       f'font-weight="600" fill="{ink}">{label}</text>')

    def row(y, title, sub):
        out.append(f'<text x="24" y="{y + 15}" font-weight="600" fill="{TEXT}">{title}</text>')
        out.append(f'<text x="24" y="{y + 31}" font-size="12" fill="{MUTED}">{sub}</text>')
        out.append(f'<rect x="{x0}" y="{y}" width="{bar_w}" height="34" rx="4" fill="{PANEL}" stroke="{LINE}"/>')

    udh_c, txt_c, fill_c = "#fbbf24", "#38bdf8", "#f87171"
    row(84, "Single SMS", "up to 160 septets")
    seg(84, x0, 160 * 7 * px, txt_c, "160 septets × 7 bits = 1120 bits")

    y = 138
    for k, (_, hexs) in enumerate(pdus):
        f = split_submit(hexs)
        udl = int(f[9][1], 16)
        septs = udl - 7
        udh = f[10][1][:12]
        row(y, f"Part {k + 1} of {parts}", f"UDL {udl} = 7 + {septs} septets")
        seg(y, x0, 48 * px, udh_c, "UDH")
        seg(y, x0 + 48 * px, 1 * px + 2, fill_c, "")
        seg(y, x0 + 49 * px + 2, septs * 7 * px - 2, txt_c, f"{septs} septets of text")
        y += 50

    # the header, byte by byte
    udh = split_submit(pdus[0][1])[10][1][:12]
    names = ["UDHL", "IEI", "IEL", "ref", "total", "part"]
    out.append(f'<text x="24" y="{y + 18}" font-weight="600" fill="{TEXT}">UDH of part 1</text>')
    bx = x0
    for i in range(6):
        b = udh[i * 2:i * 2 + 2]
        w = 40
        out.append(f'<rect x="{bx}" y="{y + 2}" width="{w}" height="26" rx="4" fill="{PANEL}" stroke="{udh_c}"/>')
        out.append(f'<text x="{bx + w / 2}" y="{y + 20}" text-anchor="middle" font-family="{MONO}" '
                   f'fill="{udh_c}">{b}</text>')
        out.append(f'<text x="{bx + w / 2}" y="{y + 44}" text-anchor="middle" font-size="11" '
                   f'fill="{MUTED}">{names[i]}</text>')
        bx += w + 6
    out.append(f'<text x="{bx + 10}" y="{y + 13}" font-size="12" fill="{MUTED}">'
               f'element 00: concatenated SMS</text>')
    out.append(f'<text x="{bx + 10}" y="{y + 30}" font-size="12" fill="{MUTED}">'
               f'ref 0x{udh[6:8]}, part {int(udh[10:12], 16)} of {int(udh[8:10], 16)}</text>')
    y += 64
    # legend + counter
    lx = 24
    for c, lab in ((txt_c, "text septets"), (udh_c, "concatenation header (6 octets)"), (fill_c, "fill bit")):
        out.append(f'<rect x="{lx}" y="{y}" width="12" height="12" rx="2" fill="{c}"/>')
        out.append(f'<text x="{lx + 18}" y="{y + 11}" font-size="12" fill="{MUTED}">{lab}</text>')
        lx += 18 + len(lab) * 6.6 + 24
    y += 30
    out.append(
        f'<text x="24" y="{y}" font-size="12" fill="{MUTED}">smspdu::count() for this {len(text)}-character '
        f'text: {units}/{capacity}, {parts} parts. UCS-2 (Arabic, emoji): 70 units, then 67 per part.</text>'
    )
    out.append(
        f'<text x="24" y="{y + 18}" font-size="12" fill="{MUTED}">A part never ends between ESC and its '
        f'character, or inside a UTF-16 surrogate pair.</text>'
    )
    return svg(W, y + 36, "\n".join(out) + "\n")


def sms_svgs():
    (short, short_count, short_pdus), (long_text, long_count, long_pdus) = sms_data()
    a = OUT / "sms-pdu-anatomy.svg"
    a.write_text(pdu_svg(short, short_count, short_pdus[0]))
    b = OUT / "sms-concat.svg"
    b.write_text(concat_svg(long_text, long_count, long_pdus))
    return a, b


if __name__ == "__main__":
    for p in (t9_gif(), arabic_png(), *sms_svgs()):
        print(f"{p.relative_to(ROOT)}  {p.stat().st_size / 1024:.0f} KiB")
