#!/usr/bin/env python3
"""Publication must reject prerelease tags and unrelated workspace versions."""

import unittest

from check_release_tag import validate_release_tag


class ReleaseTagTests(unittest.TestCase):
    def test_matching_stable_tag_is_accepted(self):
        validate_release_tag("v3.0.1", "3.0.1")

    def test_prerelease_and_mismatched_tags_are_rejected(self):
        for tag in ("v3.0.1-rc.1", "v3.0.1+build.1", "v3.0.0", "3.0.1", "v03.0.1"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                validate_release_tag(tag, "3.0.1")

    def test_prerelease_or_invalid_workspace_version_cannot_publish(self):
        for version in ("3.0.1-rc.1", "3.0.1+build.1", "03.0.1", "3.0", "3.0.1\n"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                validate_release_tag(f"v{version}", version)


if __name__ == "__main__":
    unittest.main()
