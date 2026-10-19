"""``python -m jenkinsfilelint_engine`` runs the binary."""

import subprocess
import sys

from jenkinsfilelint_engine import find_bin

if __name__ == "__main__":
    sys.exit(subprocess.call([find_bin(), *sys.argv[1:]]))
