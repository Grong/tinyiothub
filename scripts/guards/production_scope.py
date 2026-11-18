#!/usr/bin/env python3
"""Emit `path:line:content` for Rust production scope only.

Test-gated code is removed with a small lexical scanner:

- `//`, `/* ... */` (nested), `"..."`, raw `r#"..."#`, byte strings and char
  literals are masked to spaces, so braces inside them cannot confuse scope
  tracking;
- `#[cfg(test)]` gates the next item/statement: a `mod name;`/`use ...;` item
  is skipped to its `;`, a block item (fn/impl/struct/enum/trait/mod) is
  skipped through its balanced `{}`, and a statement-level item (for example a
  multi-line `let ...;`) is skipped to its terminating `;`.

Any remaining production line is printed verbatim. Used by
legacy-writers.sh; python3 is already a repository tool dependency.
"""

import re
import sys

RAW = re.compile(r'(?:br|rb|r)(?P<h>#{0,255})"')
BYTE_OR_NORMAL_STRING = re.compile(r'b?"')
CHAR = re.compile(r"b?'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.)|[^\\'\n])'")
CFG_TEST = re.compile(r'#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]')
IDENT = re.compile(r'(?:pub(?:\s*\([^)]*\))?\s+)?(?:unsafe\s+)?(?:async\s+)?(?:const\s+)?(?:extern\s+"[^"]*"\s+)?([A-Za-z_][A-Za-z0-9_]*)')
TABLES = r'(agent_tasks|agent_runs|task_wake_outbox|task_approvals|task_finalize_intents|device_jobs)'
SQL_TAIL = re.compile(
    r'(INSERT\s+(OR\s+[A-Z]+\s+)?INTO|UPDATE|DELETE\s+FROM|FROM|JOIN)\s*$'
)
SQL_TABLE_START = re.compile(r'^[ \t]*[\\"]*' + TABLES + r'[\\"]*')
BLOCK_ITEMS = {"fn", "impl", "struct", "enum", "trait", "union", "mod"}
SEMICOLON_ITEMS = {"use", "type", "static", "const"}


def mask(text: str) -> str:
    out = list(text)
    i = 0
    n = len(text)

    def blank(start: int, end: int) -> None:
        for k in range(start, min(end, n)):
            if text[k] != "\n":
                out[k] = " "

    while i < n:
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
            continue
        if text.startswith("/*", i):
            depth = 1
            j = i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            blank(i, j)
            i = j
            continue
        m = RAW.match(text, i)
        if m:
            close = '"' + m.group("h")
            j = text.find(close, m.end())
            j = n if j < 0 else j + len(close)
            blank(i, j)
            i = j
            continue
        m = BYTE_OR_NORMAL_STRING.match(text, i)
        if m:
            j = m.end()
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            blank(i, j)
            i = j
            continue
        m = CHAR.match(text, i)
        if m:
            blank(i, m.end())
            i = m.end()
            continue
        i += 1
    return "".join(out)


def find_block_end(masked: str, pos: int, limit: int) -> int:
    start = masked.find("{", pos, limit)
    if start < 0:
        return limit
    depth = 0
    for j in range(start, limit):
        if masked[j] == "{":
            depth += 1
        elif masked[j] == "}":
            depth -= 1
            if depth == 0:
                return j + 1
    return limit


def find_delimited_end(masked: str, pos: int, limit: int) -> int:
    pairs = {"{": "}", "(": ")", "[": "]"}
    opener = masked[pos]
    closer = pairs[opener]
    depth = 0
    for j in range(pos, limit):
        if masked[j] == opener:
            depth += 1
        elif masked[j] == closer:
            depth -= 1
            if depth == 0:
                return j + 1
    return limit


def skip_space(masked: str, pos: int, limit: int) -> int:
    while pos < limit and masked[pos].isspace():
        pos += 1
    return pos


def find_macro_end(masked: str, bang: int, limit: int) -> int:
    """A `name!` invocation is an item when followed by (), [] or {}."""
    pos = skip_space(masked, bang + 1, limit)
    if pos < limit and masked[pos] in "({[":
        end = find_delimited_end(masked, pos, limit)
        semi = skip_space(masked, end, limit)
        if semi < limit and masked[semi] == ";":
            end = semi + 1
        return end
    return find_semicolon(masked, bang, limit)


def find_if_end(masked: str, pos: int, limit: int) -> int:
    """Include `else` / `else if` blocks in the gated construct."""
    end = find_block_end(masked, pos, limit)
    while True:
        pos = skip_space(masked, end, limit)
        if not masked.startswith("else", pos):
            return end
        pos = skip_space(masked, pos + 4, limit)
        if masked.startswith("if", pos) and (
            pos + 2 >= limit or not (masked[pos + 2].isalnum() or masked[pos + 2] == "_")
        ):
            return find_if_end(masked, pos, limit)
        if pos < limit and masked[pos] == "{":
            end = find_block_end(masked, pos, limit)
            continue
        return end


def find_semicolon(masked: str, pos: int, limit: int) -> int:
    depths = {"(": 0, "[": 0, "{": 0}
    pairs = {")": "(", "]": "[", "}": "{"}
    for j in range(pos, limit):
        ch = masked[j]
        if ch in depths:
            depths[ch] += 1
        elif ch in pairs:
            key = pairs[ch]
            if depths[key] > 0:
                depths[key] -= 1
        elif ch == ";" and all(value == 0 for value in depths.values()):
            return j + 1
    return limit


def skip_attributes(masked: str, pos: int) -> int:
    while True:
        while pos < len(masked) and masked[pos].isspace():
            pos += 1
        if pos >= len(masked) or masked[pos] != "#":
            return pos
        j = pos + 1
        while j < len(masked) and masked[j].isspace():
            j += 1
        if j >= len(masked) or masked[j] != "[":
            return pos
        depth = 0
        while j < len(masked):
            if masked[j] == "[":
                depth += 1
            elif masked[j] == "]":
                depth -= 1
                if depth == 0:
                    j += 1
                    break
            j += 1
        pos = j


def scope_end(masked: str, attr_end: int) -> int:
    pos = skip_attributes(masked, attr_end)
    limit = len(masked)
    while pos < limit and masked[pos].isspace():
        pos += 1
    match = IDENT.match(masked, pos)
    if not match:
        # `#[cfg(test)] { ... }` or an expression statement.
        if pos < limit and masked[pos] == "{":
            return find_block_end(masked, pos, limit)
        return find_semicolon(masked, pos, limit)
    keyword = match.group(1)
    item_start = match.start(1)
    # `macro_rules! name { ... }` is a block item: the body is the block after
    # the name, not a `name!` invocation followed by `(`/`[`/`{`.
    if keyword == "macro_rules" or masked.startswith("macro_rules", item_start):
        return find_block_end(masked, match.end(), limit)
    # A macro item may be a qualified path (`foo::bar! { ... }`); consume the
    # path segments before looking for the `!`.
    cursor = skip_space(masked, match.end(), limit)
    while masked.startswith("::", cursor):
        segment = IDENT.match(masked, skip_space(masked, cursor + 2, limit))
        if not segment:
            break
        cursor = skip_space(masked, segment.end(), limit)
    if cursor < limit and masked[cursor] == "!":
        return find_macro_end(masked, cursor, limit)
    if keyword == "mod":
        # `mod name;` (file module) or `mod name { ... }`.
        brace = masked.find("{", match.end(), limit)
        semi = masked.find(";", match.end(), limit)
        if semi >= 0 and (brace < 0 or semi < brace):
            return semi + 1
        return find_block_end(masked, match.end(), limit)
    if keyword in BLOCK_ITEMS or masked.startswith("macro_rules", item_start):
        return find_block_end(masked, match.end(), limit)
    if keyword in SEMICOLON_ITEMS:
        # `const fn` / `static fn`-style block items still use a block.
        rest = masked[match.end(): limit].lstrip()
        if keyword == "const" and re.match(r'(?:unsafe\s+)?fn\b', rest):
            return find_block_end(masked, match.end(), limit)
        return find_semicolon(masked, match.end(), limit)
    # Statement-level gate (let/expression/if/match/unsafe block/...).
    rest_start = skip_space(masked, match.end(), limit)
    if keyword == "if":
        return find_if_end(masked, match.end(), limit)
    if keyword in {"match", "for", "while", "loop"} or (
        rest_start < limit and masked[rest_start] == "{"
    ):
        return find_block_end(masked, match.end(), limit)
    return find_semicolon(masked, match.end(), limit)


def production_lines(path: str):
    with open(path, "r", encoding="utf-8") as handle:
        text = handle.read()
    masked = mask(text)
    skipped = set()
    for match in CFG_TEST.finditer(masked):
        end = scope_end(masked, match.end())
        start_line = text.count("\n", 0, match.start()) + 1
        end_line = text.count("\n", 0, end) + 1
        for line in range(start_line, end_line + 1):
            skipped.add(line)
    previous = ""
    previous_number = 0
    for number, line in enumerate(text.splitlines(), start=1):
        if number in skipped:
            continue
        # Surface a SQL keyword at the end of one production line followed by a
        # retired table at the start of the next, so the line-based checks see
        # multi-line SQL.
        if previous and SQL_TAIL.search(previous) and SQL_TABLE_START.match(line):
            print(f"{path}:{previous_number}: (multi-line) {previous} {line}")
        print(f"{path}:{number}:{line}")
        previous = line
        previous_number = number


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: production_scope.py <file>...", file=sys.stderr)
        return 2
    for path in sys.argv[1:]:
        production_lines(path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
