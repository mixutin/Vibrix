#!/usr/bin/env python3
"""Build real static pages from the existing editorial sections, without a router."""

import argparse
from dataclasses import dataclass
from html import escape
from html.parser import HTMLParser
import json
from pathlib import Path
import posixpath
import re
import shutil
import xml.etree.ElementTree as ET
from urllib.parse import urljoin, urlsplit, urlunsplit

BASE = "https://mixutin.github.io/Vibrix/"
REPO = "https://github.com/mixutin/Vibrix"
VOID = set("area base br col embed hr img input link meta param source track wbr".split())
ROUTES = {
    "about": ("About", "A system that can leave the machine.",
              "The idea behind Vibrix: an independent Rust-native operating system designed for persistent removable USB storage."),
    "architecture": ("Architecture", "Understand the whole machine.",
                     "Explore the Vibrix boot path, kernel boundaries, future Rust userspace and removable-storage architecture."),
    "roadmap": ("Roadmap", "From first boot to a portable OS.",
                "Follow the Vibrix engineering milestones, scoped QEMU evidence and the work remaining before persistent USB operation."),
    "software": ("Software", "An ecosystem, still taking shape.",
                 "Explore planned Vibrix packages: vpm, vsh, vcore, rift, netkit and forge. This is a future catalog, not a download directory."),
    "community": ("Community", "Built in public. By people and agents.",
                  "Meet the contributors and recorded AI roles behind Vibrix, and browse public repository activity and contribution guidance."),
    "development": ("Development", "Follow the work, not just the promise.",
                    "Read Vibrix development updates and engineering notes, with evidence links and clear boundaries between progress and plans."),
}
NAV = [("", "Home"), ("about/", "About"), ("architecture/", "Architecture"),
       ("roadmap/", "Roadmap"), ("software/", "Software"), ("status/", "Status"),
       ("docs/", "Docs"), ("development/", "Development"),
       ("community/", "Community"), ("blog/", "Blog")]
ANCHORS = {"vision": "about/", "architecture": "architecture/", "roadmap": "roadmap/",
           "software": "software/", "updates": "development/"}
MARKER = ".vibrix-generated-site"


@dataclass
class Element:
    tag: str
    attrs: dict
    start: int
    opened: int
    parent: object
    closed: int = 0
    end: int = 0


class Document(HTMLParser):
    """Record source spans, preserving editorial HTML and script text verbatim."""

    def __init__(self, text):
        super().__init__(convert_charrefs=False)
        self.text = text
        self.lines = [0]
        self.lines.extend(m.end() for m in re.finditer("\n", text))
        self.stack = []
        self.elements = []
        self.feed(text)
        self.close()
        if self.stack:
            raise ValueError("Unclosed HTML element: " + self.stack[-1].tag)

    def source_position(self):
        line, column = self.getpos()
        return self.lines[line - 1] + column

    def handle_starttag(self, tag, attrs):
        start = self.source_position()
        end = start + len(self.get_starttag_text())
        node = Element(tag, dict(attrs), start, end, self.stack[-1] if self.stack else None)
        self.elements.append(node)
        if tag in VOID:
            node.closed = node.end = end
        else:
            self.stack.append(node)

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        if tag not in VOID:
            node = self.stack.pop()
            node.closed = node.end = node.opened

    def handle_endtag(self, tag):
        if tag in VOID:
            return
        if not self.stack or self.stack[-1].tag != tag:
            raise ValueError("Unbalanced HTML closing tag: " + tag)
        node = self.stack.pop()
        node.closed = self.source_position()
        node.end = self.text.index(">", node.closed) + 1

    def one(self, tag, parent=None):
        nodes = [n for n in self.elements if n.tag == tag and
                 (parent is None or n.parent is parent)]
        if len(nodes) != 1:
            raise ValueError(f"Expected one {tag}, found {len(nodes)}")
        return nodes[0]


def edits(text, replacements):
    for start, end, replacement in sorted(replacements, reverse=True):
        text = text[:start] + replacement + text[end:]
    return text


def route_of(path):
    return path[:-10] if path.endswith("index.html") else path


def href(target, route):
    if route == "404.html":
        return "/Vibrix/" + target
    result = posixpath.relpath(target or ".", route or ".")
    return result + ("/" if not target or target.endswith("/") else "")


def relocate(text, source, destination):
    """Rebase local assets/links, including old homepage section bookmarks."""
    doc = Document(text)
    changes = []
    pattern = re.compile(r"(\s(?:href|src)\s*=\s*)([\"'])(.*?)\2", re.I | re.S)
    for node in doc.elements:
        if node.tag == "link" and node.attrs.get("rel") == "canonical":
            continue
        raw = text[node.start:node.opened]

        def replace(match):
            from html import unescape
            value = unescape(match.group(3))
            url = urlsplit(urljoin(BASE + source, value))
            if not urlunsplit((url.scheme, url.netloc, url.path, "", "")).startswith(BASE):
                return match.group(0)
            target = url.path[len(urlsplit(BASE).path):]
            if target in ("", "index.html") and url.fragment in ANCHORS:
                target = ANCHORS[url.fragment]
            relative = href(target, destination)
            updated = urlunsplit(("", "", relative, url.query, url.fragment))
            return match.group(1) + match.group(2) + escape(updated, quote=True) + match.group(2)

        rewritten = pattern.sub(replace, raw)
        if raw != rewritten:
            changes.append((node.start, node.opened, rewritten))
    return edits(text, changes)


def navigation(route):
    def links():
        result = []
        for target, label in NAV:
            current = ' aria-current="page"' if target == route else (
                ' aria-current="location"' if target and route.startswith(target) else "")
            result.append(f'<li><a href="{href(target, route)}"{current}>{label}</a></li>')
        return "".join(result)
    items = links()
    return (f'<a class="skip-link" href="#main-content">Skip to content</a>'
            f'<header class="site-header wrap"><div class="site-topbar">'
            f'<a class="brand" href="{href("", route)}" aria-label="Vibrix home"><b aria-hidden="true">◇</b> VIBRIX</a>'
            f'<a class="site-source" href="{REPO}">Source on GitHub ↗</a></div>'
            f'<nav class="desktop-nav" aria-label="Primary"><ul>{items}</ul></nav>'
            f'<details class="mobile-menu"><summary>Explore Vibrix <span aria-hidden="true">+</span></summary>'
            f'<nav aria-label="Mobile"><ul>{items}</ul></nav></details></header>')


def footer(route):
    items = NAV + [("legal/terms/", "Terms"), ("legal/privacy/", "Privacy"), ("legal/license/", "License")]
    links = "".join(f'<li><a href="{href(target, route)}">{label}</a></li>' for target, label in items)
    return (f'<footer class="site-footer wrap"><div><a class="brand" href="{href("", route)}">◇ VIBRIX</a>'
            '<p>Independent kernel. Portable ambition.<br>Evidence before claims.</p></div>'
            f'<nav aria-label="Footer"><ul>{links}</ul></nav>'
            '<p class="site-colophon">Rust-native · USB-first · 0BSD<br>Built by AI agents.</p></footer>')


def breadcrumbs(route, label):
    if not route:
        return "", ""
    crumbs = [("Home", "")]
    if route.startswith("blog/") and route != "blog/":
        crumbs.append(("Blog", "blog/"))
    crumbs.append((label, route))
    rendered = "".join(f'<li><a href="{href(target, route)}">{escape(name)}</a></li>'
                       for name, target in crumbs[:-1])
    rendered += f'<li><span aria-current="page">{escape(label)}</span></li>'
    data = {"@context": "https://schema.org", "@type": "BreadcrumbList", "itemListElement": [
        {"@type": "ListItem", "position": i + 1, "name": name, "item": BASE + target}
        for i, (name, target) in enumerate(crumbs)]}
    return (f'<nav class="breadcrumbs wrap" aria-label="Breadcrumb"><ol>{rendered}</ol></nav>',
            '<script type="application/ld+json">' + json.dumps(data).replace("<", "\\u003c") + '</script>')


def shell(text, route, label):
    text = relocate(text, route, route)
    doc = Document(text)
    body, head, main = doc.one("body"), doc.one("head"), doc.one("main")
    header, foot = doc.one("header", body), doc.one("footer", body)
    attrs = dict(main.attrs)
    if attrs.get("id") not in (None, "main-content"):
        raise ValueError("Existing main id needs an explicit bookmark migration")
    attrs.update(id="main-content", tabindex="-1")
    opening = "<main" + "".join(f' {key}="{escape(value or "", quote=True)}"' for key, value in attrs.items()) + ">"
    trail, structured = breadcrumbs(route, label)
    css = f'<link rel="stylesheet" href="{href("multipage.css", route)}">'
    return edits(text, [(header.start, header.end, navigation(route)),
                        (foot.start, foot.end, footer(route)),
                        (main.start, main.opened, opening + trail),
                        (head.closed, head.closed, css + structured)])


def page(route, label, headline, description, content, script=False):
    canonical = BASE + route
    metadata = {"@context": "https://schema.org", "@type": "WebPage", "name": label + " | Vibrix",
                "description": description, "url": canonical, "isPartOf": {"@type": "WebSite", "name": "Vibrix", "url": BASE}}
    return ('<!doctype html><html lang="en"><head><meta charset="utf-8">'
            '<meta name="viewport" content="width=device-width,initial-scale=1">'
            f'<title>{escape(label)} — Vibrix OS</title><meta name="description" content="{escape(description, quote=True)}">'
            f'<link rel="canonical" href="{canonical}"><meta name="robots" content="{"noindex" if route == "404.html" else "index,follow"}">'
            '<meta name="theme-color" content="#08090d"><meta property="og:type" content="website">'
            f'<meta property="og:title" content="{escape(label)} — Vibrix OS"><meta property="og:url" content="{canonical}">'
            f'<meta property="og:description" content="{escape(description, quote=True)}"><meta name="twitter:card" content="summary">'
            f'<link rel="stylesheet" href="{href("style.css", route)}"><link rel="stylesheet" href="{href("engineering.css", route)}">'
            '<script type="application/ld+json">' + json.dumps(metadata) + '</script></head><body><header></header>'
            f'<main class="section-page"><section class="page-intro wrap"><p class="eyebrow">VIBRIX / {escape(label.upper())}</p>'
            f'<h1>{escape(headline)}</h1><p>{escape(description)}</p></section>{content}</main><footer></footer>'
            + (f'<script src="{href("script.js", route)}"></script>' if script else "") + '</body></html>')


def split_home(text):
    doc = Document(text)
    main = doc.one("main")
    groups = {key: [] for key in ("home", *ROUTES)}
    expected = {"hero", "vision", "features", "architecture", "roadmap", "software", "people", "activity", "updates", "philosophy", "blogtease", "cta"}
    found = set()
    for node in doc.elements:
        if node.parent is not main:
            continue
        classes = (node.attrs.get("class") or "").split()
        key = node.attrs.get("id") or next((c for c in classes if c in expected), None)
        if key is None and "band" in classes:
            key = "philosophy"
        if node.tag != "section" or key not in expected or key in found:
            raise ValueError(f"Unknown or duplicate homepage section: {key}; explicitly assign it to a page")
        found.add(key)
        target = {"hero": "home", "cta": "home", "vision": "about", "features": "about", "philosophy": "about",
                  "people": "community", "activity": "community", "updates": "development", "blogtease": "development"}.get(key, key)
        groups[target].append(text[node.start:node.end])
    if found != expected:
        raise ValueError("Missing homepage sections: " + ", ".join(sorted(expected - found)))
    cards = []
    for slug, (label, headline, description) in ROUTES.items():
        anchor = next((a for a, target in ANCHORS.items() if target == slug + "/"), slug)
        cards.append(f'<a class="portal-card" id="{anchor}" href="{slug}/"><span>{label}</span>'
                     f'<h3>{escape(headline)}</h3><p>{escape(description)}</p><b>Explore {label.lower()} <span aria-hidden="true">↗</span></b></a>')
    directory = ('<section class="page-directory wrap"><p class="eyebrow">EXPLORE VIBRIX</p>'
                 '<h2>One project.<br><span>More to discover.</span></h2><div class="portal-grid">' + "".join(cards) + '</div></section>')
    # Keep all editorial sections exactly once. The home retains hero + CTA;
    # its old anchor IDs now lead to links to the corresponding full pages.
    homepage = edits(text, [(main.opened, main.closed, groups["home"][0] + directory + groups["home"][1])])
    result = {"index.html": homepage}
    for slug, (label, headline, description) in ROUTES.items():
        route = slug + "/"
        content = relocate("\n".join(groups[slug]), "", route)
        result[route + "index.html"] = page(route, label, headline, description, content, script=slug == "community")
    return result


def build(source, output):
    if source.is_symlink() or output.is_symlink():
        raise ValueError("Source and output must not be symbolic links")
    source, output = source.resolve(), output.resolve()
    if source == output or source in output.parents or output in source.parents:
        raise ValueError("Source and output must be separate, non-nested directories")
    if not source.is_dir():
        raise ValueError("Missing source directory")
    if any(p.is_symlink() for p in source.rglob("*")):
        raise ValueError("Symbolic links are not permitted in the site source")
    if output.exists() and not (output / MARKER).is_file():
        raise ValueError("Refusing to replace a non-generated output directory")
    generated = split_home((source / "index.html").read_text(encoding="utf-8"))
    for path in generated:
        if path != "index.html" and (source / path).exists():
            raise ValueError("Generated route collides with a source page: " + path)
    if (source / "404.html").exists():
        raise ValueError("Existing 404 page needs explicit integration")
    rendered = {p.relative_to(source).as_posix(): p.read_text(encoding="utf-8") for p in source.rglob("*.html")}
    rendered.update(generated)
    recovery = (f'<section class="recovery wrap"><a class="button primary" href="{BASE}">Back to home</a> '
                f'<a class="button" href="{BASE}docs/">Browse documentation</a></section>')
    rendered["404.html"] = page("404.html", "Page not found", "This path ends here.",
                                "The page may have moved. Explore Vibrix using the navigation, or return to the homepage.", recovery)
    labels = dict(NAV + [("legal/terms/", "Terms"), ("legal/privacy/", "Privacy"), ("legal/license/", "License"),
                        ("blog/why-vibrix-exists/", "Why Vibrix exists"), ("404.html", "Page not found")])
    for path, text in rendered.items():
        route = route_of(path)
        rendered[path] = shell(text, route, labels.get(route, route.strip("/").split("/")[-1].replace("-", " ").title()))
    # All transformations succeed before touching the owned output directory.
    if output.exists():
        shutil.rmtree(output)
    shutil.copytree(source, output)
    (output / MARKER).write_text("Generated by tools/build_site.py\n", encoding="utf-8")
    (output / ".nojekyll").touch()
    for path, text in rendered.items():
        target = output / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text + "\n", encoding="utf-8")
    ET.register_namespace("", "http://www.sitemaps.org/schemas/sitemap/0.9")
    root = ET.Element("{http://www.sitemaps.org/schemas/sitemap/0.9}urlset")
    for path in sorted(rendered):
        if path != "404.html":
            url = ET.SubElement(root, "{http://www.sitemaps.org/schemas/sitemap/0.9}url")
            ET.SubElement(url, "{http://www.sitemaps.org/schemas/sitemap/0.9}loc").text = BASE + route_of(path)
    ET.ElementTree(root).write(output / "sitemap.xml", encoding="utf-8", xml_declaration=True)
    return sorted(rendered)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("site"))
    parser.add_argument("--output", type=Path, default=Path("build/site"))
    args = parser.parse_args()
    try:
        pages = build(args.source, args.output)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Site build failed: {error}\n")
    print(f"Built {len(pages)} static pages in {args.output}")
    for path in pages:
        print("  " + path)


if __name__ == "__main__":
    main()
