#!/usr/bin/env python3
"""Regenerate tests/fonts/TyportSlantTest[wght,slnt].ttf.

Derives a synthetic `slnt`-axis variable font from the vendored
tests/fonts/Karla[wght].ttf, for the fixture that exercises italic detection
on variable fonts whose italics ride an axis (`ital`/`slnt`) instead of a
separate italic face — see crates/typort-core/src/convert/page.rs
`effective_style` and tests/fixtures/variable_font_style.typ.

Why a *derived* font is needed: the vendored Karla[wght].ttf has only a
`wght` axis (its STAT table carries a vestigial `ital` axis record for
grouping with the separate Karla-Italic family member, but no `ital` axis
exists in *this* file's `fvar`, and it has no `slnt` axis at all). To test
`effective_style`'s slnt-coordinate path we need a variable font that
actually exposes one.

The added `slnt` axis carries NO glyph deltas — glyphs do not actually
slant. That's fine: typort's `effective_style` reads the *resolved
variation coordinate* Typst assigns at shape time
(`FontVariations::resolve`, typst-library text/font/variations.rs), not the
rendered glyph outlines, so a nominal axis with no visual effect is
sufficient to exercise the code path under test.

OFL compliance: this file's copyright header (Karla[wght].ttf `name` table,
nameID 0) does not carry an explicit "Reserved Font Name" clause, but the
family name "Karla" identifies the original project and per OFL section 2
must not be reused to label a modified version distributed separately. This
script renames every family/subfamily/full/unique/PostScript name in the
`name` table to "Typort Slant Test" / "TyportSlantTest" so the derivative
cannot be mistaken for an unmodified or re-branded copy of Karla. The
original copyright notice is intentionally left untouched in nameID 0 (OFL
requires *retaining* the original copyright notice in derivative works) and
duplicated into tests/fonts/OFL-TyportSlantTest.txt alongside a one-line
provenance note.

Usage:
    python3 scripts/make_slant_test_font.py
"""

from fontTools.otlLib.builder import buildStatTable
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._f_v_a_r import Axis

SRC = "tests/fonts/Karla[wght].ttf"
DST = "tests/fonts/TyportSlantTest[wght,slnt].ttf"

NEW_FAMILY = "Typort Slant Test"
NEW_PS = "TyportSlantTest"

# name table IDs that identify the font by name and must not read "Karla".
# 1=family, 3=unique id, 4=full name, 6=PostScript name, 16=typographic
# family, 25=variations PostScript name prefix (used to build instance PS
# names). (2/17 are subfamily names like "Regular" — not renamed.)
RENAMED_IDS = {1, 3, 4, 6, 16, 25}
PS_IDS = {6, 25}


def rename(font: TTFont) -> None:
    name = font["name"]
    # nameID 7 is a trademark statement about "Karla" specifically ("Karla is
    # a trademark of ..."); it does not apply to a font no longer named
    # Karla, so drop it rather than let it leak into the derivative.
    name.removeNames(nameID=7)
    for record in list(name.names):
        if record.nameID not in RENAMED_IDS:
            continue
        if record.nameID == 3:
            value = f"2.004;TYPORT;{NEW_PS}"
        elif record.nameID in PS_IDS:
            value = NEW_PS
        else:
            value = NEW_FAMILY
        name.setName(value, record.nameID, record.platformID, record.platEncID, record.langID)


def add_slnt_axis(font: TTFont) -> None:
    fvar = font["fvar"]
    slnt = Axis()
    slnt.axisTag = "slnt"
    # Upright (0) is the default; -15 is the most-slanted end. Typst's
    # oblique-fallback rule (FontVariations::resolve) picks axis.min when it
    # is negative, so #text(style: "italic") on this font resolves slnt to
    # -15 — a nonzero coordinate our fix must read as oblique/italic.
    slnt.minValue = -15.0
    slnt.defaultValue = 0.0
    slnt.maxValue = 0.0
    slnt.flags = 0
    slnt.axisNameID = font["name"].addName("Slant")
    fvar.axes.append(slnt)
    # Every named instance must carry a coordinate for every axis; pin the
    # existing wght instances to the new axis' default (upright).
    for instance in fvar.instances:
        instance.coordinates.setdefault("slnt", 0.0)


def rebuild_stat(font: TTFont) -> None:
    # Drop the original STAT table (it references the vestigial `ital` axis
    # from Karla's family grouping, which has no corresponding fvar axis in
    # this derivative) and rebuild with just wght + the new slnt axis.
    fvar = font["fvar"]
    wght_axis = next(a for a in fvar.axes if a.axisTag == "wght")
    axes = [
        dict(
            tag="wght",
            name="Weight",
            values=[
                dict(value=wght_axis.minValue, name="Light"),
                dict(value=wght_axis.defaultValue, name="Regular", flags=0x2),
                dict(value=wght_axis.maxValue, name="Bold"),
            ],
        ),
        dict(
            tag="slnt",
            name="Slant",
            values=[
                dict(value=0.0, name="Upright", flags=0x2),
                dict(value=-15.0, name="Slanted"),
            ],
        ),
    ]
    buildStatTable(font, axes)


def main() -> None:
    font = TTFont(SRC)
    add_slnt_axis(font)
    rebuild_stat(font)
    rename(font)
    font.save(DST)
    print(f"wrote {DST}")


if __name__ == "__main__":
    main()
