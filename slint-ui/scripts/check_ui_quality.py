#!/usr/bin/env python3
"""Check Slint source structure without a compiler or third-party dependencies.

This is a lexical architecture check, not a Slint type checker. Strings and
comments are tokenized before measuring code or comparing complete UI blocks.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
from dataclasses import asdict, dataclass
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
TOKEN = re.compile(
    r'(?P<space>\s+)|(?P<comment>//[^\n]*|/\*[\s\S]*?\*/)|'
    r'(?P<string>"(?:\\.|[^"\\])*")|'
    r'(?P<name>[A-Za-z_][\w-]*)|(?P<number>\d+(?:\.\d+)?|\.\d+)|(?P<symbol>.)'
)
LIMITS = {"file_lines": 600, "component_lines": 400, "global_lines": 120,
          "global_members": 80, "nesting": 5, "duplicate_tokens": 160}


@dataclass(frozen=True)
class Token:
    value: str
    line: int
    offset: int


@dataclass(frozen=True)
class Issue:
    path: str
    line: int
    rule: str
    message: str


class LexError(ValueError):
    def __init__(self, line: int, message: str):
        self.line = line
        super().__init__(message)


def tokenize(source: str) -> list[Token]:
    result = []
    line = 1
    for match in TOKEN.finditer(source):
        value = match.group()
        if match.lastgroup == "symbol" and (value == '"' or source.startswith("/*", match.start())):
            raise LexError(line, "Unterminated string or block comment")
        if match.lastgroup not in ("space", "comment"):
            result.append(Token(value, line, match.start()))
        line += value.count("\n")
    return result


def braces(tokens: list[Token]) -> tuple[dict[int, int], int, list[int]]:
    stack = []
    pairs = {}
    maximum = 0
    invalid = []
    for i, token in enumerate(tokens):
        if token.value == "{":
            stack.append(i)
            maximum = max(maximum, len(stack))
        elif token.value == "}":
            if stack:
                pairs[stack.pop()] = i
            else:
                invalid.append(i)
    return pairs, maximum, invalid + stack


def code_lines(tokens: list[Token]) -> int:
    return len({token.line for token in tokens})


def global_members(tokens: list[Token]) -> int:
    """Count declarations at the global root, even when packed onto one line."""
    depth = 0
    count = 0
    for token in tokens:
        if token.value == "{":
            depth += 1
        elif token.value == "}":
            depth -= 1
        elif depth == 1 and token.value in ("property", "callback", "function"):
            count += 1
    return count


def nesting_blocks(tokens: list[Token]):
    """Measure each top-level block, including its root and callback bodies.

    Keep the deepest opening brace's location so a finding leads to the code
    that needs extraction. Sibling blocks do not accumulate depth. Tokenizing
    first makes indentation, comments and strings irrelevant to this rule.
    """
    stack = []
    blocks = {}
    for index, token in enumerate(tokens):
        if token.value == "{":
            stack.append(index)
            start = stack[0]
            if start not in blocks or len(stack) > blocks[start][0]:
                blocks[start] = (len(stack), index)
        elif token.value == "}" and stack:
            stack.pop()
    return blocks


def imports(tokens: list[Token]):
    for i, token in enumerate(tokens[:-1]):
        if token.value == "from" and tokens[i + 1].value.startswith('"'):
            yield token.line, tokens[i + 1].value[1:-1]


def fractional_layout(tokens: list[Token]):
    """Find baked fractional lengths in bindings, declarations and assignments.

    Dimensionless ratios (font leading, scaling, easing, opacity) remain valid.
    Comparisons in input handlers are not geometry assignments. Lexing excludes
    comments and strings before inspection; custom length properties are covered
    so moving a measurement into a named constant cannot bypass the rule.
    """
    statement = []
    for i, token in enumerate(tokens):
        if token.value in ("{", "}", ";"):
            statement.clear()
        elif (re.fullmatch(r"(?:\d*\.\d+)", token.value)
              and i + 1 < len(tokens)
              and tokens[i + 1].value in ("px", "%")
              and tokens[i + 1].offset == token.offset + len(token.value)
              and any(t.value in (":", "=") for t in statement)):
            yield token.line, token.value + tokens[i + 1].value
        statement.append(token)


def model_mutations(tokens: list[Token]):
    """UI controls must dispatch list edits to their controller, including row edits."""
    models = {"queue-songs", "local-tracks", "playlist-tracks", "detail-tracks"}
    for i in range(len(tokens) - 3):
        if (not re.fullmatch(r"[A-Za-z_][\w-]*", tokens[i].value) or tokens[i + 1].value != "."
                or tokens[i + 2].value not in models):
            continue
        j = i + 3
        while j < len(tokens):
            if tokens[j].value == "[":
                depth = 1
                j += 1
                while j < len(tokens) and depth:
                    depth += (tokens[j].value == "[") - (tokens[j].value == "]")
                    j += 1
            elif tokens[j].value == ".":
                j += 2
            else:
                break
        suffix = [t.value for t in tokens[j:j + 2]]
        if (suffix and suffix[0] == "=" and suffix[1:] not in (["="], [">"])) or suffix in (["+", "="], ["-", "="], ["*", "="], ["/", "="]):
            yield tokens[i].line, tokens[i].value + "." + tokens[i + 2].value


def check(root: Path, limits: dict[str, int] | None = None) -> dict:
    root = root.resolve()
    limits = {**LIMITS, **(limits or {})}
    files = sorted((root / "ui").rglob("*.slint"))
    issues = []
    metrics = []
    graph = {}
    blocks = defaultdict(list)

    def issue(path, line, rule, message):
        issues.append(Issue(str(path.relative_to(root)), line, rule, message))

    if not files:
        return {"files": [], "issues": [asdict(Issue("ui", 1, "sources", "No Slint files found"))]}
    for path in files:
        source = path.read_text(encoding="utf-8")
        try:
            tokens = tokenize(source)
        except LexError as error:
            issue(path, error.line, "lexical", str(error))
            continue
        pairs, depth, invalid = braces(tokens)
        size = code_lines(tokens)
        components = []
        globals_ = []
        if size > limits["file_lines"]:
            issue(path, 1, "file-lines", f"{size} code lines exceeds {limits['file_lines']}; split by responsibility")
        nesting = nesting_blocks(tokens)
        block_names = {}
        for i in invalid:
            issue(path, tokens[i].line, "braces", "Unmatched brace")
        for line, value in fractional_layout(tokens):
            issue(path, line, "fractional-layout", f"Baked layout literal {value}; use a layout, measured content or an intentional dimensionless ratio")
        for line, model in model_mutations(tokens):
            issue(path, line, "model-mutation", f"Direct write to {model}; call a controller action")
        for i, token in enumerate(tokens[:-1]):
            if token.value in ("component", "global"):
                start = next((j for j in range(i + 1, len(tokens)) if tokens[j].value == "{"), None)
                if start is None or start not in pairs:
                    continue
                count = code_lines(tokens[i:pairs[start] + 1])
                name = tokens[i + 1].value
                block_names[start] = name
                component_depth, deepest = nesting[start]
                metric = {"name": name, "line": token.line, "code_lines": count,
                          "nesting": component_depth, "nesting_line": tokens[deepest].line}
                limit = limits[token.value + "_lines"]
                if count > limit:
                    issue(path, token.line, token.value + "-lines",
                          f"{name}: {count} code lines exceeds {limit}; split by responsibility")
                if token.value == "global":
                    metric["members"] = global_members(tokens[start:pairs[start] + 1])
                    globals_.append(metric)
                    if metric["members"] > limits["global_members"]:
                        issue(path, token.line, "global-members",
                              f"{name}: {metric['members']} members exceeds {limits['global_members']}; split by responsibility")
                else:
                    components.append(metric)
        for start, (block_depth, deepest) in nesting.items():
            if block_depth > limits["nesting"]:
                owner = block_names.get(start, "top-level block")
                issue(path, tokens[deepest].line, "nesting",
                      f"{owner}: brace nesting {block_depth} exceeds {limits['nesting']} "
                      "(including the root and callbacks); extract components by responsibility")
        for start, end in pairs.items():
            # Compare complete element bodies, not arbitrary overlapping windows.
            # Identifiers and literal values are preserved to avoid treating
            # merely similar layouts as interchangeable behavior.
            if start and re.fullmatch(r"[A-Z][\w-]*", tokens[start - 1].value):
                body = tuple(t.value for t in tokens[start - 1:end + 1])
                if len(body) >= limits["duplicate_tokens"]:
                    blocks[body].append((path, start, end, tokens[start - 1].line))
        graph[path] = []
        for line, dependency in imports(tokens):
            if dependency == "std-widgets.slint":
                continue
            target = (path.parent / dependency).resolve()
            if not target.is_file():
                issue(path, line, "import-missing", f"Missing import: {dependency}")
            elif not target.is_relative_to(root):
                issue(path, line, "import-boundary", f"Import leaves project: {dependency}")
            else:
                graph[path].append((target, line))
        metrics.append({"path": str(path.relative_to(root)), "code_lines": size,
                        "nesting": depth, "components": components, "globals": globals_})

    covered = defaultdict(list)
    for body, occurrences in sorted(blocks.items(), key=lambda item: -len(item[0])):
        available = [(p, a, b, line) for p, a, b, line in occurrences
                     if not any(lo <= a and b <= hi for lo, hi in covered[p])]
        if len(available) < 2:
            continue
        first, _, _, line = available[0]
        for path, start, end, at in available:
            covered[path].append((start, end))
        for path, _, _, at in available[1:]:
            issue(path, at, "duplicate-block", f"{len(body)} identical tokens also at {first.relative_to(root)}:{line}; extract a shared component")

    visiting = set()
    visited = set()

    def visit(path, chain):
        if path in visited:
            return
        visiting.add(path)
        for target, line in graph.get(path, []):
            if target in visiting:
                names = [str(p.relative_to(root)) for p in chain + [path, target]]
                issue(path, line, "import-cycle", " -> ".join(names))
            else:
                visit(target, chain + [path])
        visiting.remove(path)
        visited.add(path)

    for path in graph:
        visit(path, [])
    return {"files": metrics, "issues": [asdict(i) for i in sorted(issues, key=lambda i: (i.path, i.line, i.rule))]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="project containing ui/")
    parser.add_argument("--json", action="store_true", help="machine-readable report on stdout")
    args = parser.parse_args()
    report = check(args.root.resolve())
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        for issue in report["issues"]:
            print(f"{issue['path']}:{issue['line']}: {issue['rule']}: {issue['message']}")
        print(f"Slint quality: {len(report['files'])} files, {len(report['issues'])} issues")
    return int(bool(report["issues"]))


if __name__ == "__main__":
    sys.exit(main())
