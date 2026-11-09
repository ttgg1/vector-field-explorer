#!/usr/bin/env python3
"""Parser-based regression gate for the GitHub Pages artifact (ticket 16a).

This is deliberately *not* an adversarial HTML/CSS sanitizer. It parses the
artifact with the standard library so the recorded classes of bypass (quoted
`>`, `/` separators, entities, CSS backslash escapes, odd quoting) are handled
by a real tokenizer instead of a growing regex. It rejects external fetches,
unresolved local links and path traversal; required strings and the prose claim
guard stay in `scripts/check-site.sh`.

Usage: python3 scripts/check_site.py DIST_DIR
"""

from __future__ import annotations

import html
import posixpath
import re
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

BASE_PATH = "/vector-field-explorer"
FETCH_ATTRS = {"src", "data", "poster", "xlink:href"}
SRCSET_ATTRS = {"srcset", "imagesrcset"}
# Attribute values that legitimately contain prose or metadata URLs (for
# example `meta content="https://…"`). They do not fetch anything.
TEXT_ATTRS = {
    "alt",
    "aria-description",
    "aria-label",
    "content",
    "placeholder",
    "title",
    "value",
}
# An embedded URL in any other attribute is suspicious: a malformed
# `<link/rel=stylesheet/href="https://…">` arrives as one `rel` value.
EMBEDDED_URL_RE = re.compile(r"(?i)https?://")
SCRIPT_EXTERNAL_RE = re.compile(r"(?i)https?://")
SCRIPT_IMPORT_PROTOCOL_RELATIVE_RE = re.compile(
    r"""(?i)\b(?:import|fetch)\s*\(\s*['"]\s*//"""
)
CSS_HEX_ESCAPE_RE = re.compile(r"[0-9a-fA-F]{1,6}")


def fail(message: str) -> "None":
    print(f"check-site: {message}", file=sys.stderr)
    raise SystemExit(1)


def decode_value(value: str) -> str:
    """Decode entities and normalize URL backslashes."""
    return html.unescape(value or "").replace("\\", "/").strip()


def is_external(value: str) -> bool:
    """True for an absolute or protocol-relative URL."""
    value = decode_value(value)
    return bool(re.match(r"(?i)^(?:https?:|//)", value))


def srcset_urls(value: str) -> list[str]:
    """URL candidates of a srcset value, without density/width descriptors."""
    urls: list[str] = []
    for part in decode_value(value).split(","):
        part = part.strip()
        if not part:
            continue
        url = part.split(None, 1)[0]
        if url.lower().startswith("data:"):
            continue
        urls.append(url)
    return urls


def css_decode(text: str) -> str:
    """Decode HTML entities and CSS backslash escapes (`\\/`, `\\2f`, …)."""
    text = html.unescape(text)
    out: list[str] = []
    index = 0
    while index < len(text):
        char = text[index]
        if char != "\\":
            out.append(char)
            index += 1
            continue
        index += 1
        if index >= len(text):
            out.append("\\")
            break
        match = CSS_HEX_ESCAPE_RE.match(text, index)
        if match:
            codepoint = int(match.group(0), 16)
            out.append(chr(codepoint) if 0 < codepoint <= 0x10FFFF else "\ufffd")
            index = match.end()
            if index < len(text) and text[index] in " \t\n\r\f":
                index += 1
        else:
            out.append(text[index])
            index += 1
    return "".join(out)


def css_function_args(text: str, name: str) -> list[str]:
    """Arguments of every `name(…)` call, balanced and quote-aware."""
    pattern = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"\s*\(", re.I)
    found: list[str] = []
    for match in pattern.finditer(text):
        start = match.end()
        depth = 1
        quote: str | None = None
        index = start
        while index < len(text):
            char = text[index]
            if quote:
                if char == quote:
                    quote = None
            elif char in "\"'":
                quote = char
            elif char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0:
                    found.append(text[start:index])
                    break
            index += 1
    return found


def check_css(css: str, where: str) -> None:
    """Reject external CSS fetches after entity/escape decoding."""
    decoded = css_decode(css)
    if re.search(r"(?i)@import", decoded):
        fail(f"@import in {where}")
    for arg in css_function_args(decoded, "url"):
        value = arg.strip()
        if len(value) >= 2 and value[0] in "\"'" and value[-1] == value[0]:
            value = value[1:-1].strip()
        if is_external(value):
            fail(f"external url() in {where}: {value}")
    # image-set() takes quoted/bare image candidates; any external URL inside
    # the argument list is a fetch (including a nested url()).
    for arg in css_function_args(decoded, "image-set"):
        if re.search(r"(?i)(?:https?:)?//", arg):
            fail(f"external image-set() in {where}")


class ArtifactParser(HTMLParser):
    """Collect tags/attributes and raw script/style bodies."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.tags: list[tuple[str, list[tuple[str, str | None]], tuple[int, int]]] = []
        self.script_bodies: list[str] = []
        self.style_bodies: list[str] = []
        self._capture: str | None = None
        self._buffer: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        tag = tag.lower()
        self.tags.append((tag, attrs, self.getpos()))
        if tag in ("script", "style"):
            self._capture = tag
            self._buffer = []

    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.handle_starttag(tag, attrs)
        if tag.lower() in ("script", "style"):
            self._capture = None
            self._buffer = []

    def handle_endtag(self, tag: str) -> None:
        if self._capture and tag.lower() == self._capture:
            body = "".join(self._buffer)
            if self._capture == "script":
                self.script_bodies.append(body)
            else:
                self.style_bodies.append(body)
            self._capture = None
            self._buffer = []

    def handle_data(self, data: str) -> None:
        if self._capture:
            self._buffer.append(data)


def resolve_local(ref: str, dist: Path, page: str) -> Path | None:
    """Resolve one local reference to a file, or None for non-file schemes."""
    value = decode_value(ref)
    low = value.lower()
    if low.startswith(
        (
            "http://",
            "https://",
            "//",
            "mailto:",
            "tel:",
            "javascript:",
            "data:",
            "blob:",
            "about:",
            "#",
        )
    ):
        return None

    path = unquote(urlsplit(value).path)
    if path in (BASE_PATH, BASE_PATH + "/", BASE_PATH + "/./"):
        target = "index.html"
    elif path.startswith(BASE_PATH + "/"):
        target = path[len(BASE_PATH) + 1 :]
        if target.startswith("./"):
            target = target[2:]
        target = target or "index.html"
    elif path.startswith("/"):
        fail(f"absolute local reference '{ref}' in {page}")
    else:
        target = path
        while target.startswith("./"):
            target = target[2:]
        target = target or "index.html"

    target = posixpath.normpath(target)
    if target in (".", ""):
        target = "index.html"
    if ".." in target.split("/"):
        fail(f"path traversal in local reference '{ref}' in {page}")

    candidate = (dist / target).resolve()
    root = dist.resolve()
    if candidate != root and root not in candidate.parents:
        fail(f"path traversal in local reference '{ref}' in {page}")
    if not candidate.is_file():
        fail(f"missing local reference '{ref}' ({target}) in {page}")
    return candidate


def check_page(path: Path, dist: Path) -> tuple[int, int]:
    """Check one HTML page and return (tag count, local reference count)."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        fail(f"cannot read {path}: {error}")

    parser = ArtifactParser()
    try:
        parser.feed(text)
        parser.close()
    except Exception as error:  # HTMLParser is lenient; this is defensive.
        fail(f"cannot parse {path.name}: {error}")

    page = path.name
    local_refs = 0
    for tag, attrs, position in parser.tags:
        line = position[0]
        if tag == "base":
            fail(f"<base> tag in {page}:{line}: it rewrites every relative URL")

        attr_map = {name.lower(): value for name, value in attrs}
        canonical = tag == "link" and "canonical" in (
            (attr_map.get("rel") or "").lower().split()
        )
        meta_refresh = (attr_map.get("http-equiv") or "").lower() == "refresh"

        for name, value in attrs:
            if value is None:
                continue
            name = name.lower()
            if name == "style":
                check_css(value, f"inline style attribute in {page}:{line}")
                continue
            if name in SRCSET_ATTRS:
                for candidate in srcset_urls(value):
                    if is_external(candidate):
                        fail(
                            f"external {name} on <{tag}> in {page}:{line}: {candidate}"
                        )
                continue
            if name in TEXT_ATTRS:
                # Metadata may contain a URL without fetching it. Meta refresh
                # is the one text attribute that navigates, so keep it gated.
                if name == "content" and meta_refresh and re.search(
                    r"(?i)(?:https?:)?//", decode_value(value)
                ):
                    fail(f"external meta refresh in {page}:{line}: {value}")
                continue
            if is_external(value):
                if name == "href" and (tag == "a" or canonical):
                    continue
                fail(f"external {name} on <{tag}> in {page}:{line}: {value}")
            if name == "href" and (tag == "a" or canonical):
                continue
            if EMBEDDED_URL_RE.search(decode_value(value)):
                fail(
                    f"embedded external URL in {name} on <{tag}> in {page}:{line}: {value}"
                )

        for attr in ("href", "src", "data", "poster", "xlink:href"):
            value = attr_map.get(attr)
            if value is None:
                continue
            if resolve_local(value, dist, page) is not None:
                local_refs += 1
        for attr in SRCSET_ATTRS:
            value = attr_map.get(attr)
            if value is None:
                continue
            for candidate in srcset_urls(value):
                if resolve_local(candidate, dist, page) is not None:
                    local_refs += 1

    for body in parser.style_bodies:
        check_css(body, f"inline <style> in {page}")
    for body in parser.script_bodies:
        script = html.unescape(body)
        if SCRIPT_EXTERNAL_RE.search(script) or SCRIPT_IMPORT_PROTOCOL_RELATIVE_RE.search(
            script
        ):
            fail(f"external reference in inline <script> in {page}")

    return len(parser.tags), local_refs


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        fail("usage: check_site.py DIST_DIR")
    dist = Path(argv[1])
    pages = [dist / "index.html", dist / "download.html"]
    for page in pages:
        if not page.is_file():
            fail(f"missing {page}")

    tags = 0
    local_refs = 0
    for page in pages:
        page_tags, page_refs = check_page(page, dist)
        tags += page_tags
        local_refs += page_refs

    stylesheets = sorted(dist.glob("*.css"))
    if not stylesheets:
        fail("no stylesheets in the artifact")
    for stylesheet in stylesheets:
        try:
            css = stylesheet.read_text(encoding="utf-8")
        except OSError as error:
            fail(f"cannot read {stylesheet}: {error}")
        check_css(css, stylesheet.name)

    print(
        f"{len(pages)} pages, {tags} tags, {local_refs} local references, "
        f"{len(stylesheets)} stylesheets clean"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
