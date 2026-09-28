import hashlib
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET

from build_site import ANCHORS, BASE, Document, NAV, ROUTES, build, href, page, relocate, shell, split_home


def fixture():
    sections = ['<section class="hero wrap"><h1>Vibrix</h1><div id="console">Console</div></section>']
    for ident in ("vision", "architecture", "roadmap", "software", "updates"):
        sections.append(f'<section id="{ident}"><h2>{ident}</h2><p>Original {ident} editorial content.</p>'
                        f'<a href="status/">Status</a><a href="#{ident}">Section</a></section>')
    for css in ("features", "people", "activity", "blogtease", "cta"):
        sections.append(f'<section class="{css}"><h2>{css}</h2><p>Original {css} content.</p></section>')
    sections.insert(2, '<section class="band"><h2>Philosophy</h2><p>Libraries welcome.</p></section>')
    return ('<!doctype html><html lang="en"><head><title>Vibrix</title>'
            '<meta name="description" content="Test fixture">'
            f'<link rel="canonical" href="{BASE}"><link rel="stylesheet" href="style.css">'
            '</head><body><header><nav>Old navigation</nav></header><main>' + "".join(sections) +
            '</main><footer>Old footer</footer><script src="script.js"></script></body></html>')


class PageBuilderTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "source"
        self.output = self.root / "output"
        self.source.mkdir()
        (self.source / "index.html").write_text(fixture(), encoding="utf-8")
        (self.source / "style.css").write_text("body { margin: 0; }")

    def test_splits_every_editorial_section_into_exactly_one_page(self):
        result = split_home(fixture())
        self.assertEqual(len(result), 7)
        for ident, target in ANCHORS.items():
            marker = f"Original {ident} editorial content."
            self.assertIn(marker, result[target + "index.html"])
            self.assertEqual(sum(marker in body for body in result.values()), 1)
        self.assertIn("Original people content.", result["community/index.html"])
        self.assertIn("Original activity content.", result["community/index.html"])
        self.assertIn("Libraries welcome.", result["about/index.html"])

    def test_home_is_a_portal_not_a_duplicate_of_all_full_pages(self):
        home = split_home(fixture())["index.html"]
        self.assertEqual(home.count('class="portal-card"'), 6)
        self.assertNotIn("Original architecture editorial", home)
        for ident in ANCHORS:
            self.assertIn(f'id="{ident}"', home)

    def test_nested_urls_keep_queries_fragments_and_assets(self):
        result = relocate('<div><a href="docs/?q=a&amp;b=2#section">Docs</a><img src="image.png" alt=""></div>', "", "architecture/")
        self.assertIn('href="../docs/?q=a&amp;b=2#section"', result)
        self.assertIn('src="../image.png"', result)

    def test_legacy_home_bookmarks_become_direct_page_links(self):
        result = relocate('<a href="../../#roadmap">Roadmap</a>', "blog/post/", "blog/post/")
        self.assertIn('href="../../roadmap/#roadmap"', result)

    def test_external_links_and_script_strings_are_not_rewritten(self):
        source = '<a href="https://example.org/?q=1">External</a><script>const x = \'<a href="docs/">\';</script>'
        self.assertEqual(relocate(source, "", "about/"), source)

    def test_absolute_canonical_survives_relocation(self):
        source = f'<link rel="canonical" href="{BASE}docs/">'
        self.assertEqual(relocate(source, "docs/", "docs/"), source)

    def test_duplicate_or_unknown_sections_fail_instead_of_disappearing(self):
        for extra in ('<section id="vision"></section>', '<section id="new-feature"></section>', '<div>New unassigned content</div>'):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                split_home(fixture().replace('</main>', extra + '</main>'))

    def test_missing_section_fails(self):
        with self.assertRaisesRegex(ValueError, "Missing homepage"):
            split_home(fixture().replace('<section class="cta"><h2>cta</h2><p>Original cta content.</p></section>', ''))

    def test_unbalanced_markup_fails(self):
        with self.assertRaises(ValueError):
            Document('<main><section></main>')

    def test_void_and_self_closing_elements_do_not_break_spans(self):
        doc = Document('<main><section><p>Text<br>more</p><hr/><svg/></section></main>')
        self.assertEqual(doc.one('section').end, len(doc.text) - len('</main>'))

    def test_shared_navigation_is_static_and_current_page_is_marked(self):
        result = shell(page('about/', *ROUTES['about'], '<section><h2>Body</h2></section>'), 'about/', 'About')
        doc = Document(result)
        primaries = [n for n in doc.elements if n.tag == 'nav' and n.attrs.get('aria-label') in ('Primary', 'Mobile')]
        self.assertEqual(len(primaries), 2)
        for nav in primaries:
            menu = result[nav.start:nav.end]
            self.assertEqual(menu.count('<li>'), len(NAV))
            self.assertIn('aria-current="page">About', menu)
        self.assertIn('<details class="mobile-menu">', result)
        self.assertIn('href="#main-content">Skip to content', result)
        self.assertEqual(result.count('id="main-content"'), 1)
        self.assertNotIn('fetch(', result)

    def test_blog_breadcrumbs_and_historical_byline_survive(self):
        content = '<article><h2>Post</h2><p>By GPT-5.6 Sol</p><footer>Historical author footer</footer></article>'
        result = shell(page('blog/post/', 'Post', 'Title', 'Description', content), 'blog/post/', 'Post')
        self.assertIn('aria-current="location">Blog', result)
        self.assertIn('Historical author footer', result)
        self.assertIn('By GPT-5.6 Sol', result)
        self.assertIn('BreadcrumbList', result)

    def test_build_is_repeatable_and_does_not_modify_sources(self):
        before = hashlib.sha256((self.source / 'index.html').read_bytes()).hexdigest()
        pages = build(self.source, self.output)
        first = {p.relative_to(self.output): p.read_bytes() for p in self.output.rglob('*') if p.is_file()}
        build(self.source, self.output)
        second = {p.relative_to(self.output): p.read_bytes() for p in self.output.rglob('*') if p.is_file()}
        self.assertEqual(first, second)
        self.assertEqual(before, hashlib.sha256((self.source / 'index.html').read_bytes()).hexdigest())
        self.assertEqual(len(pages), 8)

    def test_existing_pages_and_assets_are_preserved_with_shared_layout(self):
        nested = self.source / 'legal/privacy/index.html'
        nested.parent.mkdir(parents=True)
        nested.write_text(page('legal/privacy/', 'Privacy', 'Privacy', 'Privacy details', '<p>Unchanged legal text.</p>'))
        build(self.source, self.output)
        output = (self.output / 'legal/privacy/index.html').read_text()
        self.assertIn('Unchanged legal text.', output)
        self.assertIn('class="site-header wrap"', output)
        self.assertEqual((self.source / 'style.css').read_bytes(), (self.output / 'style.css').read_bytes())

    def test_noindex_404_uses_project_absolute_links_at_arbitrary_depth(self):
        build(self.source, self.output)
        not_found = (self.output / '404.html').read_text()
        self.assertIn('content="noindex"', not_found)
        self.assertIn('href="/Vibrix/style.css"', not_found)
        self.assertIn('href="/Vibrix/docs/"', not_found)
        urls = [n.text for n in ET.parse(self.output / 'sitemap.xml').iter() if n.tag.endswith('loc')]
        self.assertEqual(len(urls), 7)
        self.assertNotIn(BASE + '404.html', urls)
        self.assertEqual(len(urls), len(set(urls)))

    def test_refuses_to_delete_an_unowned_directory(self):
        self.output.mkdir()
        important = self.output / 'important.txt'
        important.write_text('Keep me')
        with self.assertRaisesRegex(ValueError, 'non-generated'):
            build(self.source, self.output)
        self.assertEqual(important.read_text(), 'Keep me')

    def test_nested_source_or_output_is_rejected(self):
        for output in (self.source, self.source / 'build', self.root):
            with self.subTest(output=output), self.assertRaises(ValueError):
                build(self.source, output)

    def test_symlinks_are_rejected_before_copying_or_removal(self):
        (self.source / 'link.css').symlink_to(self.source / 'style.css')
        with self.assertRaisesRegex(ValueError, 'Symbolic'):
            build(self.source, self.output)
        (self.source / 'link.css').unlink()
        self.output.symlink_to(self.source, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symbolic'):
            build(self.source, self.output)

    def test_route_collision_requires_deliberate_integration(self):
        path = self.source / 'about/index.html'
        path.parent.mkdir()
        path.write_text('Existing independently authored page')
        with self.assertRaisesRegex(ValueError, 'collides'):
            build(self.source, self.output)
        self.assertFalse(self.output.exists())

    def test_each_new_page_has_unique_canonical_description_and_one_h1(self):
        build(self.source, self.output)
        canonicals = set()
        for slug in ROUTES:
            text = (self.output / slug / 'index.html').read_text()
            doc = Document(text)
            doc.one('h1')
            canonical = next(n.attrs['href'] for n in doc.elements if n.tag == 'link' and n.attrs.get('rel') == 'canonical')
            description = next(n.attrs['content'] for n in doc.elements if n.tag == 'meta' and n.attrs.get('name') == 'description')
            self.assertEqual(canonical, BASE + slug + '/')
            self.assertTrue(description)
            canonicals.add(canonical)
        self.assertEqual(len(canonicals), 6)


if __name__ == '__main__':
    unittest.main()
