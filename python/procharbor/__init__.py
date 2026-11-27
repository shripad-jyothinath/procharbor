"""ProcHarbor - Native Linux task manager and system monitor."""

import os
import sys
import subprocess
import shutil

__version__ = "1.3.0"

def find_binary() -> str:
    """Locate the native compiled procharbor binary."""
    # Check directory containing this file or parent directories (site-packages/procharbor or bin)
    base_dir = os.path.dirname(os.path.abspath(__file__))
    candidates = [
        os.path.join(base_dir, "procharbor"),
        os.path.join(base_dir, "..", "..", "bin", "procharbor"),
        os.path.join(sys.prefix, "bin", "procharbor"),
        os.path.expanduser("~/.local/bin/procharbor"),
        "/usr/local/bin/procharbor",
        "/usr/bin/procharbor",
    ]
    for path in candidates:
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return os.path.abspath(path)

    # Fall back to PATH lookup
    which_path = shutil.which("procharbor")
    if which_path:
        return which_path

    raise FileNotFoundError(
        "Could not find native procharbor executable. "
        "Please ensure procharbor is installed or in your PATH."
    )

def main() -> int:
    try:
        bin_path = find_binary()
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1
    return subprocess.call([bin_path] + sys.argv[1:])

if __name__ == "__main__":
    sys.exit(main())
