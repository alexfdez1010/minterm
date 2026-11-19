# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools[woff]==4.68.0", "skia-pathops==0.9.2"]
# ///

"""Build both Minterm Mono faces from their hand-drawn paths in font units."""

from pathlib import Path
import unicodedata

import pathops
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.svgLib.path import parse_path

from minterm_symbols import FILLS, SYMBOLS

EM = 1000
CELL = 700
CAP_HEIGHT = 760
X_HEIGHT = 568
ASCENT = 1000
DESCENT = -200
FAMILY = "Minterm Mono"

# Coordinates are y-up. The oblique shoulders and open terminals carry the
# same angle across letters while bowls keep long, mostly vertical sides.
CAPS = {
    "A": "M118 0 L307 713 Q310 718 321 718 H379 Q390 718 393 707 L582 0 M198 300 H502",
    "B": "M170 0 V718 H380 C483 718 540 661 540 560 V523 C540 440 482 387 384 387 H170 "
         "M384 387 C494 387 557 333 557 236 V197 C557 98 497 43 390 43 H170",
    "C": "M539 616 L463 691 Q435 720 382 720 H286 Q231 720 193 681 L163 649 Q137 622 137 562 "
         "V196 Q137 144 165 115 L202 76 Q234 40 286 40 H385 Q436 40 472 76 L550 152",
    "D": "M170 43 V718 H358 Q419 718 459 678 L522 615 Q545 592 545 536 V210 Q545 162 518 135 "
         "L462 81 Q424 43 363 43 Z",
    "E": "M545 718 H173 V43 H550 M173 387 H478",
    "F": "M543 718 H173 V0 M173 387 H476",
    "G": "M540 608 L460 690 Q431 720 382 720 H286 Q231 720 193 681 L163 649 Q137 622 137 562 "
         "V196 Q137 144 165 115 L202 76 Q234 40 286 40 H386 Q442 40 482 77 L552 143 V365 H372",
    "H": "M161 0 V760 M539 0 V760 M161 387 H539",
    "I": "M186 718 H514 M350 718 V43 M173 43 H527",
    "J": "M244 718 H533 V192 Q533 136 502 105 L468 72 Q435 40 386 40 H287 "
         "Q236 40 198 79 L138 140",
    "K": "M161 0 V760 M557 733 L161 316 M307 460 L577 22",
    "L": "M169 760 V43 H558",
    "M": "M136 0 V716 H179 L350 420 L521 716 H564 V0",
    "N": "M151 0 V716 H180 L516 44 H549 V760",
    "O": "M283 720 H417 Q465 720 495 688 L534 648 Q559 624 559 565 V201 Q559 143 534 117 "
         "L491 74 Q459 40 411 40 H289 Q241 40 209 74 L166 117 Q141 143 141 201 V559 "
         "Q141 622 166 648 L205 688 Q235 720 283 720 Z",
    "P": "M168 0 V718 H380 Q460 718 505 673 Q544 634 544 564 V508 Q544 434 505 395 "
         "Q459 349 379 349 H168",
    "Q": "M283 720 H417 Q465 720 495 688 L534 648 Q559 624 559 565 V201 Q559 143 534 117 "
         "L491 74 Q459 40 411 40 H289 Q241 40 209 74 L166 117 Q141 143 141 201 V559 "
         "Q141 622 166 648 L205 688 Q235 720 283 720 Z M363 238 L584 7",
    "R": "M168 0 V718 H380 Q460 718 505 673 Q544 634 544 564 V528 Q544 454 505 415 "
         "Q459 369 379 369 H168 M352 369 L566 20",
    "S": "M540 618 L475 682 Q438 720 381 720 H290 Q239 720 200 691 Q152 658 152 596 "
         "V541 Q152 471 220 435 L469 326 Q552 284 552 215 V174 Q552 101 503 70 Q458 40 405 40 "
         "H300 Q241 40 199 75 L137 128",
    "T": "M117 718 H583 M350 718 V0",
    "U": "M148 760 V208 Q148 147 177 118 L225 70 Q260 40 307 40 H393 "
         "Q440 40 475 70 L523 118 Q552 147 552 208 V760",
    "V": "M119 749 L304 45 H396 L581 749",
    "W": "M108 752 L202 45 H243 L350 374 L457 45 H498 L592 752",
    "X": "M122 740 L578 20 M578 740 L122 20",
    "Y": "M112 742 L350 375 L588 742 M350 375 V0",
    "Z": "M146 718 H554 L143 43 H563",
}

LOWER = {
    "a": "M153 443 L205 495 Q234 525 281 525 H388 Q530 525 530 396 V0 "
         "M530 308 H263 Q147 308 147 200 V157 Q147 41 273 41 H381 Q455 41 530 118",
    "b": "M157 0 V795 M157 442 L208 493 Q245 528 298 528 H388 Q437 528 473 493 L517 449 "
         "Q541 424 541 380 V186 Q541 140 517 115 L473 72 Q437 38 388 38 H298 "
         "Q245 38 208 72 L157 122",
    "c": "M528 450 L482 495 Q449 528 399 528 H297 Q246 528 211 493 L166 448 Q143 425 143 375 "
         "V191 Q143 141 166 118 L211 73 Q246 38 297 38 H398 Q448 38 480 70 L537 126",
    "d": "M543 0 V795 M543 442 L492 493 Q455 528 402 528 H312 Q263 528 227 493 L183 449 "
         "Q159 424 159 380 V186 Q159 140 183 115 L227 72 Q263 38 312 38 H402 "
         "Q455 38 492 72 L543 122",
    "e": "M149 288 H545 V376 Q545 425 520 451 L478 493 Q442 528 391 528 H301 "
         "Q250 528 215 493 L172 450 Q149 427 149 377 V191 Q149 141 174 116 L216 74 "
         "Q251 38 302 38 H410 Q460 38 492 70 L544 119",
    "f": "M238 0 V644 Q238 711 276 746 Q312 779 379 779 H429 Q475 779 517 744 L550 713 "
         "M126 525 H506",
    "g": "M547 568 V-40 Q547 -94 506 -118 Q477 -134 416 -134 H283 Q224 -134 188 -107 L150 -78 "
         "M547 442 L497 493 Q460 528 407 528 H315 Q266 528 230 493 L186 449 "
         "Q162 424 162 380 V213 Q162 160 186 135 L230 92 Q266 58 315 58 H407 "
         "Q460 58 497 92 L547 142",
    "h": "M158 0 V795 M158 419 L235 496 Q269 528 319 528 H397 Q540 528 540 376 V0",
    "i": "M210 525 H357 V43 M188 43 H515",
    "j": "M219 525 H452 V-33 Q452 -94 411 -121 Q381 -134 332 -134 H270 Q215 -134 176 -99 L138 -62",
    "k": "M158 0 V795 M537 547 L158 196 M313 339 L559 25",
    "l": "M207 795 H332 V189 Q332 43 452 43 H530",
    "m": "M113 0 V568 M113 456 L149 501 Q171 528 212 528 H247 Q350 528 350 407 V0 "
         "M350 446 L390 501 Q413 528 455 528 H486 Q587 528 587 408 V0",
    "n": "M158 0 V568 M158 419 L235 496 Q269 528 319 528 H397 Q540 528 540 376 V0",
    "o": "M302 528 H398 Q448 528 483 494 L525 452 Q551 425 551 379 V188 Q551 139 525 114 "
         "L483 72 Q448 38 398 38 H302 Q252 38 217 72 L175 114 Q149 139 149 188 V379 "
         "Q149 425 175 452 L217 494 Q252 528 302 528 Z",
    "p": "M157 -179 V568 M157 442 L208 493 Q245 528 298 528 H388 Q437 528 473 493 L517 449 "
         "Q541 424 541 380 V186 Q541 140 517 115 L473 72 Q437 38 388 38 H298 "
         "Q245 38 208 72 L157 122",
    "q": "M543 -179 V568 M543 442 L492 493 Q455 528 402 528 H312 Q263 528 227 493 L183 449 "
         "Q159 424 159 380 V186 Q159 140 183 115 L227 72 Q263 38 312 38 H402 "
         "Q455 38 492 72 L543 122",
    "r": "M175 0 V568 M175 409 L252 487 Q290 528 343 528 H399 Q458 528 487 498 L548 437",
    "s": "M528 447 L481 493 Q445 529 390 529 H293 Q165 529 165 415 "
         "Q165 348 234 323 L466 251 Q535 228 535 160 Q535 38 400 38 H289 "
         "Q233 38 195 73 L152 113",
    "t": "M255 697 V177 Q255 39 383 39 H430 Q476 39 514 77 L553 116 M136 525 H535",
    "u": "M158 568 V188 Q158 38 301 38 H381 Q431 38 465 70 L542 147 M542 568 V0",
    "v": "M130 554 L307 43 H393 L570 554",
    "w": "M109 562 L211 43 H251 L350 349 L449 43 H489 L591 562",
    "x": "M134 547 L566 21 M566 547 L134 21",
    "y": "M128 553 L307 43 H390 M570 555 L349 -68 Q323 -134 252 -134 H150",
    "z": "M149 525 H552 L149 43 H555",
    "ß": "M151 0 V585 Q151 678 194 728 Q233 768 300 768 H363 Q483 768 483 653 V611 "
         "Q483 551 452 518 L367 423 L493 365 Q552 337 552 267 V187 Q552 43 405 43 H333",
    "Æ": "M85 0 L290 718 H594 M368 718 V43 H596 M368 381 H555 M156 252 H368",
    "æ": "M126 446 L174 496 Q204 528 247 528 H276 Q369 528 369 411 V167 Q369 40 484 40 H502 "
         "Q557 40 599 101 M369 308 H207 Q112 308 112 201 V154 Q112 42 215 42 H242 "
         "Q298 42 369 118 M369 293 H592 V406 Q592 528 493 528 H469 Q369 528 369 408",
    "Œ": "M325 717 H287 Q235 717 205 686 L162 643 Q132 613 132 564 V198 Q132 148 162 118 "
         "L205 75 Q235 43 287 43 H325 M590 718 H325 V43 H594 M325 385 H558",
    "œ": "M361 416 Q361 528 261 528 H239 Q121 528 121 389 V178 Q121 38 239 38 H261 "
         "Q361 38 361 151 M361 293 H594 V404 Q594 528 493 528 H469 Q361 528 361 402 "
         "V165 Q361 38 471 38 H500 Q551 38 596 103",
    "Þ": "M164 0 V760 M164 575 H380 Q545 575 545 428 V359 Q545 213 380 213 H164",
    "þ": "M157 -179 V795 M157 442 L208 493 Q245 528 298 528 H388 Q437 528 473 493 L517 449 "
         "Q541 424 541 380 V186 Q541 140 517 115 L473 72 Q437 38 388 38 H298 "
         "Q245 38 208 72 L157 122",
    "Ð": "M170 43 V718 H358 Q419 718 459 678 L522 615 Q545 592 545 536 V210 Q545 162 518 135 "
         "L462 81 Q424 43 363 43 Z M91 386 H322",
    "ð": "M214 774 L396 664 Q552 569 552 392 V197 Q552 43 398 43 H299 Q148 43 148 186 V311 "
         "Q148 454 298 454 H396 Q501 454 547 361 M237 598 L464 763",
    "ı": "M210 525 H357 V43 M188 43 H515",
    "Ł": "M169 760 V43 H558 M99 288 L395 490",
    "ł": "M207 795 H332 V189 Q332 43 452 43 H530 M207 313 L466 496",
}

ACCENTS = {
    "\N{COMBINING ACUTE ACCENT}": "M298 0 L424 113",
    "\N{COMBINING GRAVE ACCENT}": "M280 113 L406 0",
    "\N{COMBINING CIRCUMFLEX ACCENT}": "M224 0 L350 91 L476 0",
    "\N{COMBINING CARON}": "M224 91 L350 0 L476 91",
    "\N{COMBINING TILDE}": "M183 9 Q230 88 281 68 L403 20 Q460 -1 517 82",
    "\N{COMBINING MACRON}": "M204 49 H496",
    "\N{COMBINING BREVE}": "M223 94 Q259 0 350 0 Q441 0 477 94",
    "\N{COMBINING RING ABOVE}": "M350 0 C306 0 282 21 282 57 C282 93 306 114 350 114 "
                               "C394 114 418 93 418 57 C418 21 394 0 350 0 Z",
    "\N{COMBINING DOUBLE ACUTE ACCENT}": "M220 0 L325 113 M393 0 L498 113",
    "\N{COMBINING CEDILLA}": "M355 8 L310 -51 Q422 -44 422 -96 Q422 -150 321 -150 H276",
    "\N{COMBINING OGONEK}": "M449 43 Q390 -48 390 -92 Q390 -148 459 -148 H501",
}

DOT = "\N{COMBINING DOT ABOVE}"
DIAERESIS = "\N{COMBINING DIAERESIS}"
RING = "\N{COMBINING RING ABOVE}"
BELOW = {"\N{COMBINING CEDILLA}", "\N{COMBINING OGONEK}"}
STROKES = CAPS | LOWER | SYMBOLS
CHARS = {" ", "\N{NO-BREAK SPACE}", "\N{SOFT HYPHEN}"} | STROKES.keys() | FILLS.keys()


def can_draw(char):
    parts = unicodedata.normalize("NFD", char)
    return len(parts) == 2 and parts[0] in STROKES and parts[1] in ACCENTS.keys() | {DOT, DIAERESIS}


# Latin-1 and Latin Extended-A keep names, paths and commands legible in
# common European languages. WebKit supplies the rest of Unicode.
CHARS |= {chr(code) for code in range(0xA1, 0x180) if can_draw(chr(code))}


def svg_path(outline):
    path = pathops.Path()
    parse_path(outline, path.getPen())
    return path


def stroke(outline, width):
    path = svg_path(outline)
    path.stroke(width, pathops.LineCap.BUTT_CAP, pathops.LineJoin.ROUND_JOIN, 4)
    path.convertConicsToQuads(0.5)
    return path


def dot(x, y, size):
    left, right = x - size / 2, x + size / 2
    bottom, top = y - size / 2, y + size / 2
    corner = size * 0.22
    return svg_path(
        f"M{left + corner} {bottom} H{right - corner} L{right} {bottom + corner} "
        f"V{top - corner} L{right - corner} {top} H{left + corner} "
        f"L{left} {top - corner} V{bottom + corner} Z"
    )


def outline_for(char, width):
    path = pathops.Path()
    if char in STROKES or char in FILLS:
        outlines = STROKES.get(char, [])
        if isinstance(outlines, str):
            outlines = [outlines]
        for outline in outlines:
            path.addPath(stroke(outline, width))
        if char in FILLS:
            fill = FILLS[char]
            path.addPath(svg_path(fill))
        if char in "ij":
            path.addPath(dot(357 if char == "i" else 452, 711, width + 19))
    elif can_draw(char):
        base, mark = unicodedata.normalize("NFD", char)
        if base == "j":
            path = stroke(LOWER["j"], width)
        else:
            path = outline_for("ı" if base == "i" else base, width)

        origin = 836 if base.isupper() else 654
        if base in "bdhklt":
            origin = 836
        offset = {"i": 7, "j": 102}.get(base, 0)
        if mark == DOT:
            path.addPath(dot(350 + offset, origin + 53, width * 0.88))
        elif mark == DIAERESIS:
            for x in (243, 457):
                path.addPath(dot(x + offset, origin + 53, width * 0.85))
        else:
            accent_width = width * {RING: 0.55}.get(mark, 0.74 if mark in BELOW else 0.68)
            accent = stroke(ACCENTS[mark], accent_width)
            path.addPath(accent.transform(1, 0, 0, 1, offset, 0 if mark in BELOW else origin))
    return path


def glyph_for(path):
    pen = TTGlyphPen(None)
    if len(path):
        outline = pathops.simplify(path, clockwise=True)
        outline.draw(Cu2QuPen(pen, max_err=0.75))
    glyph = pen.glyph()
    glyph.recalcBounds(None)
    return glyph


def build(style, weight, width):
    cmap = {ord(char): f"uni{ord(char):04X}" for char in sorted(CHARS)}
    glyphs = {".notdef": glyph_for(pathops.Path())}
    for code, name in cmap.items():
        glyphs[name] = glyph_for(outline_for(chr(code), width))

    font = FontBuilder(EM)
    font.setupGlyphOrder(list(glyphs))
    font.setupCharacterMap(cmap)
    font.setupGlyf(glyphs)
    font.setupHorizontalMetrics({name: (CELL, glyph.xMin) for name, glyph in glyphs.items()})
    font.setupHorizontalHeader(ascent=ASCENT, descent=DESCENT)
    font.setupNameTable({
        "familyName": FAMILY,
        "styleName": style,
        "uniqueFontIdentifier": f"Minterm Mono {style} 1.000",
        "fullName": f"{FAMILY} {style}",
        "psName": f"MintermMono-{style}",
        "version": "Version 1.000",
    })
    font.setupOS2(
        version=4,
        sTypoAscender=ASCENT,
        sTypoDescender=DESCENT,
        usWinAscent=ASCENT,
        usWinDescent=-DESCENT,
        usWeightClass=weight,
        sxHeight=X_HEIGHT,
        sCapHeight=CAP_HEIGHT,
        fsSelection=0x80 | (0x40 if weight == 400 else 0x20),
    )
    font.setupPost(isFixedPitch=1)
    font.font["head"].macStyle = 1 if weight == 700 else 0
    # Use the OpenType epoch offset for a repeatable binary.
    font.font["head"].created = font.font["head"].modified = 2082844800
    font.font.recalcTimestamp = False
    font.font.flavor = "woff2"
    destination = Path(__file__).resolve().parent / f"Minterm-Mono-{style}.woff2"
    font.save(destination)


if __name__ == "__main__":
    build("Regular", 400, 88)
    build("Bold", 700, 126)
