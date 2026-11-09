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
# `<link/rel=stylesheet/href="https://…">` arrives as one `rel` value, and a
# `data:` URL on a non-image can carry an external fetch. Protocol-relative
# matches need an argument boundary so local `icons//x.png` is not flagged.
EMBEDDED_URL_RE = re.compile(r"(?i)https?://|(?:^|[\s\"'<>(,])//[a-z0-9]")
SCRIPT_EXTERNAL_RE = re.compile(r"(?i)https?://")
# Any quoted protocol-relative URL in an inline script, not just import/fetch:
# `s.src = '//evil.example/x.js'` is the same external fetch.
SCRIPT_PROTOCOL_RELATIVE_RE = re.compile(r"""(?i)['"`]//[a-z0-9]""")
JS_ESCAPE_RE = re.compile(
    r"\\u\{([0-9a-fA-F]{1,6})\}|\\u([0-9a-fA-F]{4})|\\x([0-9a-fA-F]{2})"
    r"|\\([0-7]{1,3})|\\(.)",
    re.S,
)
CSS_HEX_ESCAPE_RE = re.compile(r"[0-9a-fA-F]{1,6}")
# A `//` only counts at an argument boundary, so local `icons//x.png` passes;
# `https?:` still catches scheme URLs without slashes.
CSS_EXTERNAL_IN_ARGS_RE = re.compile(r"(?i)(?:^|[\s\"',(])(?:https?:)?//|https?:")
WHITESPACE_RE = re.compile(r"[\t\r\n]")


def fail(message: str) -> "None":
    print(f"check-site: {message}", file=sys.stderr)
    raise SystemExit(1)


def decode_value(value: str) -> str:
    """Decode entities, drop WHATWG-stripped whitespace, normalize `\\`."""
    return WHITESPACE_RE.sub(
        "", html.unescape(value or "").replace("\\", "/")
    ).strip()


def is_external(value: str) -> bool:
    """True for an absolute or protocol-relative URL."""
    value = decode_value(value)
    return bool(re.match(r"(?i)^(?:https?:|//)", value))


def js_decode(text: str) -> str:
    """Decode JS string escapes (`\\uXXXX`, `\\xXX`, `\\/`) for scanning."""
    def replace(match: re.Match[str]) -> str:
        if match.group(1):
            return chr(int(match.group(1), 16))
        if match.group(2):
            return chr(int(match.group(2), 16))
        if match.group(3):
            return chr(int(match.group(3), 16))
        if match.group(4):
            return chr(int(match.group(4), 8))
        return match.group(5) or ""

    return JS_ESCAPE_RE.sub(replace, text)


def srcset_urls(value: str) -> list[str]:
    """URL candidates of a srcset value, without descriptors.

    A `data:` candidate is skipped before comma splitting: its payload may
    contain commas that are not candidate separators.
    """
    text = decode_value(value)
    urls: list[str] = []
    index = 0
    while index < len(text):
        while index < len(text) and text[index] in " \t\r\n,":
            index += 1
        if index >= len(text):
            break
        start = index
        if text[index : index + 5].lower() == "data:":
            # A data URI may contain commas; it ends at whitespace. If that
            # token ends with a comma, the comma separator belongs to the
            # list, so the next candidate starts after the whitespace. The
            # no-space `…,payload,https://…` form stays one data URI.
            while index < len(text) and text[index] not in " \t\r\n":
                index += 1
            token = text[start:index]
            if token.endswith(","):
                continue
            while index < len(text) and text[index] != ",":
                index += 1
            if index < len(text):
                index += 1
            continue
        while index < len(text) and text[index] not in " \t\r\n,":
            index += 1
        url = text[start:index]
        # Skip the optional descriptor to the next candidate.
        while index < len(text) and text[index] != ",":
            index += 1
        if index < len(text):
            index += 1
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


CSS_FUNCTION_RE = re.compile(r"(?<![A-Za-z0-9_])(-?[A-Za-z][A-Za-z0-9-]*)\s*\(")


def css_functions(text: str) -> list[tuple[str, str]]:
    """Every `name(…)` call as (name, arguments), balanced and quote-aware."""
    found: list[tuple[str, str]] = []
    for match in CSS_FUNCTION_RE.finditer(text):
        name = match.group(1).lower()
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
                    found.append((name, text[start:index]))
                    break
            index += 1
    return found


def mask_data_uris(text: str) -> str:
    """Replace data: URI payloads so their `/` characters are not fetches."""
    masked: list[str] = []
    index = 0
    while index < len(text):
        char = text[index]
        if char in "\"'":
            start = index + 1
            if text[start : start + 5].lower() == "data:":
                end = text.find(char, start)
                if end == -1:
                    end = len(text) - 1
                masked.append("DATA")
                index = end + 1
                continue
            masked.append(char)
            index += 1
            continue
        if text[index : index + 5].lower() == "data:":
            end = index + 5
            while end < len(text) and text[end] not in " \t\r\n)":
                end += 1
            masked.append("DATA")
            index = end
            continue
        masked.append(char)
        index += 1
    return "".join(masked)


def check_css(css: str, where: str) -> None:
    """Reject external CSS fetches after entity/escape decoding."""
    decoded = WHITESPACE_RE.sub("", css_decode(css))
    if re.search(r"(?i)@import", decoded):
        fail(f"@import in {where}")
    # Any external URL in any CSS function argument (url, image, image-set,
    # cross-fade, -webkit-cross-fade, …). Local url(../x)/url(/x) and data:
    # payloads have no boundary-anchored scheme, so they stay allowed.
    for name, arguments in css_functions(decoded):
        if CSS_EXTERNAL_IN_ARGS_RE.search(mask_data_uris(arguments)):
            fail(f"external URL in CSS {name}() in {where}")


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


def check_html(text: str, page: str, dist: Path, depth: int = 0) -> tuple[int, int]:
    """Check one HTML document/fragment and return (tags, local references)."""
    if depth > 8:
        fail(f"srcdoc nesting too deep in {page}")

    parser = ArtifactParser()
    try:
        parser.feed(text)
        parser.close()
    except Exception as error:  # HTMLParser is lenient; this is defensive.
        fail(f"cannot parse {page}: {error}")

    local_refs = 0
    nested_tags = 0
    for tag, attrs, position in parser.tags:
        line = position[0]
        if tag == "base":
            fail(f"<base> tag in {page}:{line}: it rewrites every relative URL")

        seen: set[str] = set()
        for name, _value in attrs:
            key = name.lower()
            if key in seen:
                fail(f"duplicate attribute '{key}' on <{tag}> in {page}:{line}")
            seen.add(key)

        attr_map = {name.lower(): value for name, value in attrs}
        # Only an exact `rel="canonical"` exempts the link: a combined
        # `rel="canonical stylesheet"` is still a stylesheet fetch.
        canonical = tag == "link" and (
            (attr_map.get("rel") or "").lower().split() == ["canonical"]
        )
        meta_refresh = (attr_map.get("http-equiv") or "").lower() == "refresh"

        for name, value in attrs:
            name = name.lower()
            if name.startswith("on"):
                fail(f"event-handler attribute '{name}' on <{tag}> in {page}:{line}")
            if value is None:
                continue
            if name == "href" and decode_value(value).lower().startswith(
                "javascript:"
            ):
                fail(f"javascript: href on <{tag}> in {page}:{line}")
            if name == "srcdoc":
                tags, refs = check_html(value, f"{page}:{line} srcdoc", dist, depth + 1)
                nested_tags += tags
                local_refs += refs
                continue
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
            decoded_value = decode_value(value)
            # Only an image's data: URL is exempt from the embedded-URL scan:
            # SVG payloads legitimately contain `http://www.w3.org/…`. A
            # data: iframe/stylesheet/script/object can still fetch externally.
            trusted_data_img = (
                tag == "img"
                and name == "src"
                and decoded_value.lower().startswith("data:")
            )
            if not trusted_data_img and EMBEDDED_URL_RE.search(decoded_value):
                fail(
                    f"embedded external URL in {name} on <{tag}> in {page}:{line}: {value}"
                )

        for attr in FETCH_ATTRS | {"href"}:
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
        script = js_decode(html.unescape(body))
        if SCRIPT_EXTERNAL_RE.search(script) or SCRIPT_PROTOCOL_RELATIVE_RE.search(
            script
        ):
            fail(f"external reference in inline <script> in {page}")

    return len(parser.tags) + nested_tags, local_refs


def check_page(path: Path, dist: Path) -> tuple[int, int]:
    """Read and check one HTML page."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        fail(f"cannot read {path}: {error}")
    return check_html(text, path.name, dist)


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
