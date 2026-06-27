"""Jenkinsfile linter package."""

from importlib.metadata import version, PackageNotFoundError

# Ensure ``jenkinsfilelint.runners`` is accessible as a module attribute,
# which is required by ``unittest.mock.patch`` dotted-path resolution
# (notably on Python 3.10 with editable installs).
from . import runners  # noqa: F401

try:
    __version__ = version("jenkinsfilelint")
except PackageNotFoundError:
    # Package is not installed
    __version__ = "0.0.0.dev0"
