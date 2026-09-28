#!/usr/bin/env python3
"""Offline checks for static HTML links and homepage/status metadata."""

import argparse
import json
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

BASE = "https://mixutin.github.io/Vibrix/"


class Page(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.links = []
        self.ids = set()
        self.duplicates = set()
        self.title = []
        self.description = ""
        self.canonical = ""
        self.h1_count = 0
        self.in_title = False
        self.in_json = False
        self.json_parts = []
        self.json_documents = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            if attrs["id"] in self.ids:
                self.duplicates.add(attrs["id"])
            self.ids.add(attrs["id"])
        for key in ("href", "src"):
            if attrs.get(key):
                self.links.append(attrs[key])
        if tag == "title":
            self.in_title = True
        if tag == "h1":
            self.h1_count += 1
        if tag == "meta" and attrs.get("name") == "description":
            self.description = attrs.get("content", "")
        if tag == "link" and attrs.get("rel") == "canonical":
            self.canonical = attrs.get("href", "")
        if tag == "script" and attrs.get("type") == "application/ld+json":
            self.in_json = True
            self.json_parts = []

    def handle_endtag(self, tag):
        if tag == "title":
            self.in_title = False
        if tag == "script" and self.in_json:
            self.json_documents.append("".join(self.json_parts))
            self.in_json = False

    def handle_data(self, data):
        if self.in_title:
            self.title.append(data)
        if self.in_json:
            self.json_parts.append(data)


def check_site(root: Path) -> list[str]:
    root = root.resolve()
    pages = {}
    errors = []
    for path in sorted(root.rglob("*.html")):
        page = Page()
        page.feed(path.read_text(encoding="utf-8"))
        pages[path] = page
    if not pages:
        return ["No HTML pages found"]
    for path, page in pages.items():
        relative = path.relative_to(root).as_posix()
        for duplicate in sorted(page.duplicates):
            errors.append(f"{relative}: duplicate id {duplicate}")
        for document in page.json_documents:
            try:
                json.loads(document)
            except ValueError as error:
                errors.append(f"{relative}: invalid JSON-LD: {error}")
        if relative in {"index.html", "status/index.html"}:
            expected = BASE + ("" if relative == "index.html" else "status/")
            if not "".join(page.title).strip() or not page.description.strip():
                errors.append(f"{relative}: missing title or description")
            if page.h1_count != 1:
                errors.append(f"{relative}: expected exactly one h1")
            if page.canonical != expected:
                errors.append(f"{relative}: canonical must be {expected}")
        for link in page.links:
            url = urlsplit(link)
            if url.scheme or url.netloc:
                if url.scheme.lower() in {"javascript", "vbscript"}:
                    errors.append(f"{relative}: executable link scheme is not allowed")
                continue
            decoded = unquote(url.path)
            if decoded.startswith("/Vibrix/"):
                target = root / decoded[len("/Vibrix/"):]
            elif decoded.startswith("/"):
                errors.append(f"{relative}: root link must include /Vibrix/: {link}")
                continue
            else:
                target = path.parent / decoded if decoded else path
            target = target.resolve()
            if not target.is_relative_to(root):
                errors.append(f"{relative}: local link escapes site root: {link}")
                continue
            if target.is_dir():
                target = target / "index.html"
            if not target.is_file():
                errors.append(f"{relative}: missing local target: {link}")
            elif url.fragment and target in pages and unquote(url.fragment) not in pages[target].ids:
                errors.append(f"{relative}: missing fragment: {link}")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", type=Path, default=Path("site"))
    args = parser.parse_args()
    errors = check_site(args.root)
    if errors:
        print("\n".join(errors))
        return 1
    print("Static site links, JSON-LD and homepage/status metadata passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
