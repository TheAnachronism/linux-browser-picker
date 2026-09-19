#!/usr/bin/env python3
"""Fail when source user-facing copy bypasses localization or the English catalog."""

from __future__ import annotations

import re
import sys
from pathlib import Path

PLACEHOLDERS = ("TODO", "FIXME", "TBD", "WIP")
PLACEHOLDER_COPY = ("lorem ipsum", "coming soon", "unfinished")

STRING_LITERAL = re.compile(
    r'"(?:\\.|[^"\\])*"',
)
I18N_CALL = re.compile(
    r"i18n::(?:text|text_with)\s*\(",
)
ACCESSIBLE_LITERAL = re.compile(
    r'Property::(?:Label|Description)\(\s*("(?:\\.|[^"\\])*")\s*,?\s*\)',
    re.S,
)
WIDGET_LITERAL = re.compile(
    r'(?:with_label|with_mnemonic|placeholder_text|set_tooltip_text|tooltip_text)\(\s*(?:Some\()?\s*("(?:\\.|[^"\\])*")',
    re.S,
)
TITLE_LITERAL = re.compile(
    r'\.title\(\s*("(?:\\.|[^"\\])*")\s*\)',
    re.S,
)
LABEL_BUILDER_LITERAL = re.compile(
    r'\.label\(\s*("(?:\\.|[^"\\])*")\s*\)',
    re.S,
)

def decode_rust_string(literal: str) -> str:
    body = literal[1:-1]
    escaped = []
    index = 0
    while index < len(body):
        char = body[index]
        if char != "\\":
            escaped.append(char)
            index += 1
            continue
        if body.startswith("\\u{", index):
            end = body.find("}", index)
            escaped.append(chr(int(body[index + 3 : end], 16)))
            index = end + 1
            continue
        nxt = body[index + 1] if index + 1 < len(body) else ""
        escaped.append(
            {"n": "\n", "t": "\t", "r": "\r", "0": "\0", '"': '"', "'": "'", "\\": "\\"}.get(
                nxt, nxt
            )
        )
        index += 2
    return "".join(escaped)


def rust_concatenated_string(text: str, start: int) -> tuple[str, int] | None:
    pieces: list[str] = []
    index = start
    length = len(text)
    while index < length:
        while index < length and text[index] in " \t\r\n":
            index += 1
        if index >= length or text[index] != '"':
            break
        match = STRING_LITERAL.match(text, index)
        if match is None:
            break
        pieces.append(decode_rust_string(match.group(0)))
        index = match.end()
    if not pieces:
        return None
    return "".join(pieces), index


def strip_test_modules(source: str) -> str:
    marker = "#[cfg(test)]"
    output: list[str] = []
    index = 0
    while True:
        start = source.find(marker, index)
        if start < 0:
            output.append(source[index:])
            break
        output.append(source[index:start])
        brace = source.find("{", start)
        if brace < 0:
            break
        depth = 0
        cursor = brace
        while cursor < len(source):
            char = source[cursor]
            if char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
                if depth == 0:
                    cursor += 1
                    break
            cursor += 1
        index = cursor
    return "".join(output)


def extract_i18n_messages(source: str) -> list[tuple[int, str]]:
    messages: list[tuple[int, str]] = []
    for match in I18N_CALL.finditer(source):
        extracted = rust_concatenated_string(source, match.end())
        if extracted is None:
            continue
        message, _ = extracted
        line = source.count("\n", 0, match.start()) + 1
        messages.append((line, message))
    return messages


def parse_po(text: str) -> dict[str, str]:
    entries: dict[str, str] = {}
    msgid: list[str] | None = None
    msgstr: list[str] | None = None
    collecting: str | None = None

    def finish() -> None:
        nonlocal msgid, msgstr, collecting
        if msgid is not None and msgstr is not None:
            key = "".join(msgid)
            if key:
                entries[key] = "".join(msgstr)
        msgid = None
        msgstr = None
        collecting = None

    string_part = re.compile(r'"(?:\\.|[^"\\])*"')
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("#"):
            continue
        if not line:
            finish()
            continue
        if line.startswith("msgid "):
            finish()
            collecting = "msgid"
            msgid = [decode_rust_string(string_part.search(line).group(0))]
            msgstr = None
            continue
        if line.startswith("msgstr "):
            collecting = "msgstr"
            msgstr = [decode_rust_string(string_part.search(line).group(0))]
            continue
        if collecting and line.startswith('"'):
            decoded = decode_rust_string(string_part.match(line).group(0))
            if collecting == "msgid" and msgid is not None:
                msgid.append(decoded)
            elif collecting == "msgstr" and msgstr is not None:
                msgstr.append(decoded)
    finish()
    return entries


def iter_rust_files(root: Path) -> list[Path]:
    return sorted(path for path in root.rglob("*.rs") if "target" not in path.parts)


def line_number(source: str, index: int) -> int:
    return source.count("\n", 0, index) + 1


def call_argument_span(source: str, call: str) -> list[tuple[int, str]]:
    spans: list[tuple[int, str]] = []
    needle = call + "("
    index = 0
    while True:
        start = source.find(needle, index)
        if start < 0:
            return spans
        open_paren = start + len(needle) - 1
        depth = 0
        cursor = open_paren
        while cursor < len(source):
            char = source[cursor]
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0:
                    spans.append((start, source[open_paren + 1 : cursor]))
                    index = cursor + 1
                    break
            cursor += 1
        else:
            return spans


def localized_literal(prefix: str) -> bool:
    stripped = prefix.rstrip()
    return stripped.endswith("i18n::text(") or stripped.endswith("i18n::text_with(")


def find_dialog_bypasses(path: Path, source: str) -> list[str]:
    findings: list[str] = []
    for call, kind in (
        ("AlertDialog::new", "dialog"),
        ("add_responses", "dialog response"),
    ):
        for start, args in call_argument_span(source, call):
            for match in STRING_LITERAL.finditer(args):
                literal = decode_rust_string(match.group(0))
                if not literal or re.fullmatch(r"[a-z][a-z0-9_]*", literal):
                    continue
                if localized_literal(args[: match.start()]):
                    continue
                findings.append(
                    f"{path}:{line_number(source, start)}: {kind} literal bypasses localization: {literal!r}"
                )
    return findings


def find_literal_bypasses(path: Path, source: str) -> list[str]:
    findings: list[str] = []
    for pattern, kind in (
        (ACCESSIBLE_LITERAL, "AT-SPI"),
        (WIDGET_LITERAL, "widget"),
        (TITLE_LITERAL, "window title"),
        (LABEL_BUILDER_LITERAL, "label"),
    ):
        for match in pattern.finditer(source):
            literal = decode_rust_string(match.group(1))
            if not literal or literal.startswith("<"):
                continue
            findings.append(
                f"{path}:{line_number(source, match.start())}: {kind} literal bypasses localization: {literal!r}"
            )
    findings.extend(find_dialog_bypasses(path, source))
    return findings


def check_catalog(po_text: str, catalog: dict[str, str]) -> list[str]:
    errors: list[str] = []
    if re.search(r"\b(TODO|FIXME|TBD|WIP)\b", po_text):
        errors.append("English catalog contains placeholder tokens")
    lowered = po_text.lower()
    for needle in PLACEHOLDER_COPY:
        if needle in lowered:
            errors.append(f"English catalog contains placeholder copy: {needle}")
    for msgid, msgstr in catalog.items():
        if msgstr == "":
            errors.append(f"empty English translation for {msgid!r}")
        elif any(token in msgstr for token in PLACEHOLDERS):
            errors.append(f"placeholder English translation for {msgid!r}")
    return errors


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: i18n_coverage.py SRC_DIR PO_FILE", file=sys.stderr)
        return 2
    src_root = Path(argv[1])
    po_path = Path(argv[2])
    po_text = po_path.read_text()
    catalog = parse_po(po_text)
    errors: list[str] = []
    errors.extend(check_catalog(po_text, catalog))

    seen: dict[str, str] = {}
    for path in iter_rust_files(src_root):
        source = strip_test_modules(path.read_text())
        for line, message in extract_i18n_messages(source):
            seen[message] = f"{path}:{line}"
            if message not in catalog:
                errors.append(f"{path}:{line}: localized message missing from catalog: {message!r}")
        errors.extend(find_literal_bypasses(path, source))

    if errors:
        print("\n".join(errors), file=sys.stderr)
        print(f"{len(errors)} localization coverage failure(s)", file=sys.stderr)
        return 1
    print(f"catalog covers {len(seen)} localized source messages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
