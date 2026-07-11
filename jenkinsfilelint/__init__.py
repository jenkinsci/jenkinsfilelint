"""Jenkinsfile linter package."""

from importlib.metadata import version, PackageNotFoundError

from . import local as local  # re-export for mock.patch

try:
    __version__ = version("jenkinsfilelint")
except PackageNotFoundError:
    # Package is not installed
    __version__ = "0.0.0.dev0"
