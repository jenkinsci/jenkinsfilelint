"""Locate the jenkinsfilelint-engine binary installed with this package."""

import os
import shutil
import sys
import sysconfig
from pathlib import Path

BINARY = "jenkinsfilelint-engine" + (".exe" if sys.platform == "win32" else "")


def find_bin() -> str:
    """Return the path of the jenkinsfilelint-engine binary.

    The wheel installs it into the scripts directory of the environment the
    package was installed into, which is not always on PATH.

    Raises:
        FileNotFoundError: if the binary cannot be found.
    """
    candidates = [
        sysconfig.get_path("scripts"),
        sysconfig.get_path("scripts", sysconfig.get_preferred_scheme("user")),
        # pip install --target puts scripts in bin/ next to the package.
        str(Path(__file__).resolve().parent.parent / "bin"),
    ]
    for directory in candidates:
        path = os.path.join(directory, BINARY)
        if os.path.isfile(path):
            return path

    path = shutil.which(BINARY)
    if path:
        return path

    raise FileNotFoundError(f"{BINARY} not found in {', '.join(candidates)} or PATH")
