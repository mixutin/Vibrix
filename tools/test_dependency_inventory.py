import unittest

from dependency_inventory import check_workflow_pins, inventory, validate_git_source


class DependencyPolicyTests(unittest.TestCase):
    def test_registry_crates_are_allowed(self):
        validate_git_source("registry+https://github.com/rust-lang/crates.io-index")

    def test_exact_git_revision_is_allowed(self):
        sha = "a" * 40
        validate_git_source(f"git+https://example.org/crate?rev={sha}#{sha}")

    def test_unpinned_branch_tag_and_short_git_revisions_are_rejected(self):
        for query in ["", "branch=main", "tag=v1", "rev=abc123", "rev=", "rev=" + "a" * 40 + "&tag=v1"]:
            with self.subTest(query=query), self.assertRaises(ValueError):
                validate_git_source("git+https://example.org/crate?" + query + "#" + "a" * 40)

    def test_git_lock_must_match_full_revision(self):
        with self.assertRaises(ValueError):
            validate_git_source("git+https://example.org/crate?rev=" + "a" * 40 + "#" + "b" * 40)

    def test_duplicate_revision_parameters_are_rejected(self):
        sha = "a" * 40
        with self.assertRaises(ValueError):
            validate_git_source(f"git+https://example.org/crate?rev={sha}&rev={sha}#{sha}")

    def test_empty_workspace_graph_is_valid(self):
        result = inventory({"packages": [], "workspace_members": [], "resolve": {"nodes": []}})
        self.assertEqual(result["package_count"], 0)

    def test_missing_resolved_graph_is_not_reported_as_clean(self):
        with self.assertRaises(ValueError):
            inventory({"packages": [], "workspace_members": [], "resolve": None})

    def test_report_includes_host_build_and_macro_risks(self):
        package = {"id": "p", "name": "p", "version": "1.0", "source": None,
                   "license": "MIT", "links": "native", "targets": [{"kind": ["custom-build", "proc-macro"]}]}
        result = inventory({"packages": [package], "workspace_members": [],
                            "resolve": {"nodes": [{"id": "p", "features": ["z", "a"], "dependencies": []}]}})
        self.assertEqual(result["external_package_count"], 1)
        item = result["packages"][0]
        self.assertTrue(item["build_script"])
        self.assertTrue(item["proc_macro"])
        self.assertEqual(item["enabled_features"], ["a", "z"])
        self.assertEqual(item["native_links"], "native")

    def test_pinned_actions_and_local_workflows_are_allowed(self):
        for value in ["actions/checkout@" + "a" * 40, "./.github/workflows/dependencies.yml",
                      "docker://example/image@sha256:" + "a" * 64]:
            self.assertEqual(check_workflow_pins("      - uses: " + value), [])

    def test_floating_actions_and_short_pins_are_rejected(self):
        for value in ["actions/checkout@v7", "actions/checkout@main", "actions/checkout@abc123",
                      "docker://example/image:latest", "${{ inputs.action }}", "|", ""]:
            with self.subTest(value=value):
                self.assertTrue(check_workflow_pins("      - uses: " + value))

    def test_quoted_action_and_version_comment_are_allowed(self):
        self.assertEqual(check_workflow_pins('      - uses: "actions/checkout@' + 'a' * 40 + '" # v7'), [])

    def test_comment_is_not_an_action(self):
        self.assertEqual(check_workflow_pins("# - uses: actions/checkout@main"), [])


if __name__ == "__main__":
    unittest.main()
