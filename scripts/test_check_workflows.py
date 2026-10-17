#!/usr/bin/env python3
"""Tests for check_workflows.py: python3 scripts/test_check_workflows.py"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import check_workflows as cw  # noqa: E402

SHA = "11d5960a326750d5838078e36cf38b85af677262"

GOOD = f"""name: release
on:
  push:
    tags: ["v*"]
permissions: {{}}
jobs:
  build:
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@{SHA} # v4
      - uses: ./local-action
  publish:
    runs-on: ubuntu-latest
    environment: release
    permissions:
      contents: write
      id-token: write
    steps:
      - run: echo ${{{{ secrets.NPM_TOKEN }}}}
"""


def errors(text, name=".github/workflows/release.yml"):
    return cw.check_text(name, text)


class Rules(unittest.TestCase):
    def test_good_workflow_passes(self):
        self.assertEqual(errors(GOOD), [])

    def test_tag_pin_fails(self):
        e = errors(GOOD.replace(f"actions/checkout@{SHA}", "actions/checkout@v4"))
        self.assertTrue(any("not pinned" in x for x in e), e)

    def test_short_sha_fails(self):
        e = errors(GOOD.replace(SHA, SHA[:12]))
        self.assertTrue(any("not pinned" in x for x in e), e)

    def test_id_token_outside_release_environment_fails(self):
        e = errors(GOOD.replace("    environment: release\n", ""))
        self.assertTrue(any("id-token" in x for x in e), e)
        self.assertTrue(any("secret NPM_TOKEN" in x for x in e), e)

    def test_environment_as_mapping(self):
        text = GOOD.replace("    environment: release\n", "    environment:\n      name: release\n      url: https://x\n")
        self.assertEqual(errors(text), [])

    def test_pages_environment_may_use_oidc(self):
        text = GOOD.replace("    environment: release\n", "    environment:\n      name: github-pages\n")
        e = errors(text)
        self.assertFalse(any("id-token" in x for x in e), e)
        self.assertTrue(any("secret NPM_TOKEN" in x for x in e), e)

    def test_workflow_level_id_token_fails(self):
        text = GOOD.replace("permissions: {}\n", "permissions:\n  contents: read\n  id-token: write\n")
        e = errors(text)
        self.assertTrue(any("workflow level" in x for x in e), e)
        self.assertTrue(any("grant write" in x for x in e), e)

    def test_release_job_without_permissions_fails(self):
        text = GOOD.replace("    permissions:\n      contents: read\n", "", 1)
        e = errors(text)
        self.assertTrue(any("job build declares no permissions" in x for x in e), e)
        # Outside the release workflows that is allowed.
        self.assertEqual(errors(text, ".github/workflows/ci.yml"), [])

    def test_pull_request_target_fails(self):
        text = GOOD.replace("  push:\n", "  pull_request_target:\n  push:\n")
        self.assertTrue(any("pull_request_target" in x for x in errors(text)))

    def test_comment_mentions_do_not_count(self):
        text = GOOD.replace("name: release\n", "name: release\n# never use pull_request_target or id-token: write here\n")
        self.assertEqual(errors(text), [])

    def test_the_repo_passes(self):
        self.assertEqual(cw.main([]), 0)


if __name__ == "__main__":
    unittest.main()
