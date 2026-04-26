#!/usr/bin/env python3

# This script was AI generated, I'm too lazy to write it myself. It extracts keyboard layout presets
# from cyanophage's index.html and generates a presets.yaml file.

from __future__ import annotations

import argparse
import re
import sys
import urllib.parse
from pathlib import Path

PLAYGROUND_COLS = 12
PLAYGROUND_ROWS = 4
LETTER_SLOTS = 34
DEFAULT_LAYOUT = r"qwertyuiop-asdfghjkl;'zxcvbnm,./\^"
PLAYGROUND_BASE_URL = "https://cyanophage.github.io/playground.html"

THUMB_ROW = 3
THUMB_COL_LEFT = 4
THUMB_COL_RIGHT = 7
SHIFT_SLOT = (1, 0)
RIGHT_THUMB_SLOT = (3, 7)

TOKEN_RE = re.compile(
    r"""
    <h1\s+id="([^"]*)">\s*([^<]+)
    |
    <a\s+class="link"\s+href="([^"]+)"
    """,
    re.IGNORECASE | re.VERBOSE,
)


class ParseError(Exception):
    pass


def thumb_col(thumb_side: str) -> int:
    return THUMB_COL_RIGHT if thumb_side == "r" else THUMB_COL_LEFT


def extract_layout_entries(path: Path) -> list[tuple[str, str]]:
    text = path.read_bytes().decode("utf-8", errors="surrogateescape")
    entries: list[tuple[str, str]] = []
    current_name: str | None = None

    for match in TOKEN_RE.finditer(text):
        heading_id = match.group(1)
        heading_text = match.group(2)
        href = match.group(3)

        if heading_id is not None:
            name = heading_id.strip() or (heading_text or "").strip()
            if name:
                current_name = name
            continue

        if href is None or current_name is None:
            continue

        entries.append((current_name, href))
        current_name = None

    return entries


def normalize_url(href: str) -> str:
    if href.startswith(("http://", "https://")):
        return href
    return urllib.parse.urljoin(f"{PLAYGROUND_BASE_URL}/../", href)


def parse_url_parts(url: str) -> tuple[str, str]:
    if "?" not in url:
        raise ParseError("invalid link URL: missing query string")

    parsed = urllib.parse.urlparse(url)
    pairs = urllib.parse.parse_qsl(parsed.query, keep_blank_values=True)

    raw_layout = next((v for k, v in pairs if k == "layout"), None)
    if raw_layout is None:
        raise ParseError("missing layout param in URL")

    raw_thumb = next((v for k, v in pairs if k == "thumb"), None)
    thumb_side = "r" if raw_thumb == "r" else "l"

    return raw_layout, thumb_side


def fully_decode(raw: str) -> str:
    value = raw
    while "%" in value:
        decoded = urllib.parse.unquote(value)
        if decoded == value:
            break
        value = decoded
    return value


def default_slot(index: int) -> tuple[int, int]:
    if 0 <= index <= 10:
        return 0, index + 1
    if 11 <= index <= 21:
        return 1, index - 10
    if 22 <= index <= 31:
        return 2, index - 21
    if index == 32:
        return 2, 0
    if index == 33:
        return 3, 4
    raise AssertionError(f"invalid default slot index: {index}")


def decode_compact_layout(chars: list[str], thumb_side: str) -> list[list[str]]:
    rows = [["_"] * PLAYGROUND_COLS for _ in range(PLAYGROUND_ROWS)]

    for index, ch in enumerate(chars[:11]):
        rows[0][index + 1] = ch
    for index, ch in enumerate(chars[11:22]):
        rows[1][index + 1] = ch

    rows[2][0] = "\\"
    for index, ch in enumerate(chars[22:32]):
        rows[2][index + 1] = ch

    rows[THUMB_ROW][thumb_col(thumb_side)] = thumb_side
    return rows


def decode_permutation_layout(chars: list[str], thumb_side: str) -> list[list[str]]:
    letters = list(DEFAULT_LAYOUT)
    shift = "$"
    right_thumb = "\0"

    if len(chars) == 35:
        letters[33] = "*"
        shift = "="

    for index in range(LETTER_SLOTS):
        if index >= len(chars):
            break
        ch = chars[index]
        try:
            target = letters.index(ch)
        except ValueError:
            continue
        letters[index], letters[target] = letters[target], letters[index]

    if len(chars) == 35:
        last = chars[34]
        if last != shift:
            try:
                target = letters.index(last)
            except ValueError:
                letters[0], shift = shift, letters[0]
            else:
                letters[target], shift = shift, letters[target]
    else:
        try:
            target = letters.index("^")
        except ValueError:
            pass
        else:
            letters[target] = "="

    if thumb_side == "r":
        letters[33], right_thumb = right_thumb, letters[33]

    rows = [["_"] * PLAYGROUND_COLS for _ in range(PLAYGROUND_ROWS)]
    for index, ch in enumerate(letters):
        row, col = default_slot(index)
        if ch not in {"^", "\0"}:
            rows[row][col] = ch

    if shift != "$":
        rows[SHIFT_SLOT[0]][SHIFT_SLOT[1]] = shift
    if right_thumb not in {"\0", "^"}:
        rows[RIGHT_THUMB_SLOT[0]][RIGHT_THUMB_SLOT[1]] = right_thumb

    return rows


def decode_layout(param: str, thumb_side: str) -> list[list[str]]:
    chars = list(param)
    if len(chars) < 32:
        raise ParseError(f"layout param too short (len={len(chars)})")
    if len(chars) == 33 and chars[32] == "\t":
        return decode_compact_layout(chars[:32], thumb_side)
    return decode_permutation_layout(chars, thumb_side)


def preset_from_url(url: str) -> str:
    raw_layout, thumb_side = parse_url_parts(url)
    param = fully_decode(raw_layout)
    rows = decode_layout(param, thumb_side)
    return "".join("_" if ch == "\\" else ch for row in rows for ch in row)


def format_layout(preset: str) -> str:
    rows = [
        preset[index : index + PLAYGROUND_COLS]
        for index in range(0, 48, PLAYGROUND_COLS)
    ]
    rendered = []
    for row in rows:
        left = " ".join(row[:6])
        right = " ".join(row[6:])
        rendered.append(f"    {left}   {right}")
    return "\n".join(rendered)


def quote_yaml_key(key: str) -> str:
    if re.fullmatch(r"[A-Za-z0-9_-]+", key):
        return key
    escaped = key.replace("'", "''")
    return f"'{escaped}'"


def render_presets(entries: list[tuple[str, str]]) -> str:
    chunks: list[str] = []
    for name, preset in entries:
        chunks.append(f"{quote_yaml_key(name)}: |\n")
        for line in format_layout(preset).splitlines():
            chunks.append(f"  {line}\n")
        chunks.append("\n")
    return "".join(chunks).rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate presets.yaml from cyanophage index.html",
    )
    parser.add_argument(
        "-i",
        "--input",
        help="Path to cyanophage index.html",
    )
    parser.add_argument(
        "-o",
        "--output",
        help="Output YAML file",
    )
    args = parser.parse_args()

    input_path = Path(args.input)
    output_path = Path(args.output)

    parsed_entries: list[tuple[str, str]] = []
    failed = 0

    for name, href in extract_layout_entries(input_path):
        try:
            preset = preset_from_url(normalize_url(href))
        except Exception as exc:
            failed += 1
            print(f"[skip] {name}: {exc}", file=sys.stderr)
            continue
        parsed_entries.append((name, preset))

    output_path.write_text(render_presets(parsed_entries), encoding="utf-8")
    print(f"wrote {len(parsed_entries)} presets to {output_path}")
    if failed:
        print(f"skipped {failed} entries", file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
