#!/usr/bin/env python3
"""Tests for the __init__.py module."""

import sys
import importlib
from unittest.mock import patch


class TestVersion:
    """Test version retrieval."""

    def test_version_when_package_not_found(self):
        """Test version falls back to dev version when package is not installed."""
        # We need to mock at the importlib.metadata level before importing
        with patch("importlib.metadata.version") as mock_version:
            mock_version.side_effect = importlib.metadata.PackageNotFoundError()

            # Remove the module from cache if it exists
            original = sys.modules.pop("jenkinsfilelint", None)
            try:
                # Now import it fresh
                import jenkinsfilelint

                # Check that it fell back to the dev version
                assert jenkinsfilelint.__version__ == "0.0.0.dev0"
            finally:
                # Clean up: put the original package back. Its submodules
                # (cli, offline, ...) are attributes of that object, and
                # mock.patch("jenkinsfilelint.offline...") looks them up there.
                if original is not None:
                    sys.modules["jenkinsfilelint"] = original

    def test_version_when_package_installed(self):
        """Test version is retrieved when package is installed."""
        import jenkinsfilelint

        # When package is installed, version should be a string
        assert isinstance(jenkinsfilelint.__version__, str)
        assert len(jenkinsfilelint.__version__) > 0
