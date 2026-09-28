import tempfile
import unittest
from pathlib import Path

from check_site import BASE, check_site


class SiteChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def page(self, body="", head=""):
        text = ('<!doctype html><html><head><title>Vibrix</title>'
                '<meta name="description" content="Development status">'
                f'<link rel="canonical" href="{BASE}">{head}</head>'
                f'<body><h1>Vibrix</h1>{body}</body></html>')
        (self.root / "index.html").write_text(text)

    def test_valid_links_and_fragment(self):
        self.page('<a href="#console">Console</a><section id="console"></section><a href="assets/a.txt">A</a>')
        (self.root / "assets").mkdir()
        (self.root / "assets/a.txt").write_text("a")
        self.assertEqual(check_site(self.root), [])

    def test_missing_target(self):
        self.page('<a href="missing.html">Missing</a>')
        self.assertTrue(any("missing local target" in e for e in check_site(self.root)))

    def test_missing_fragment(self):
        self.page('<a href="#missing">Missing</a>')
        self.assertTrue(any("missing fragment" in e for e in check_site(self.root)))

    def test_duplicate_id(self):
        self.page('<i id="a"></i><i id="a"></i>')
        self.assertTrue(any("duplicate id" in e for e in check_site(self.root)))

    def test_encoded_root_escape(self):
        self.page('<a href="%2e%2e/outside.html">Outside</a>')
        self.assertTrue(any("escapes site root" in e for e in check_site(self.root)))

    def test_executable_link_rejected(self):
        self.page('<a href="javascript:void(0)">Bad</a>')
        self.assertTrue(any("executable link" in e for e in check_site(self.root)))

    def test_external_link_needs_no_network(self):
        self.page('<a href="https://example.org/not-fetched">External</a>')
        self.assertEqual(check_site(self.root), [])

    def test_malformed_structured_data(self):
        self.page(head='<script type="application/ld+json">{broken}</script>')
        self.assertTrue(any("invalid JSON-LD" in e for e in check_site(self.root)))

    def test_project_relative_root_link(self):
        self.page('<a href="/Vibrix/">Home</a>')
        self.assertEqual(check_site(self.root), [])

    def test_missing_metadata(self):
        (self.root / "index.html").write_text("<html><body>empty</body></html>")
        self.assertEqual(len(check_site(self.root)), 3)

    def test_empty_site_fails(self):
        self.assertEqual(check_site(self.root), ["No HTML pages found"])


if __name__ == "__main__":
    unittest.main()
