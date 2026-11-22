"""Compatibility launcher for the reader's Plico CLI development session."""

import argparse
import os
from pathlib import Path

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    image = parser.parse_args().image.resolve(strict=True)
    project = Path(__file__).resolve().parent
    root = project.parents[1]
    os.chdir(root)
    os.execvp("cargo", ["cargo", "plico", "dev", str(project), "--wasm", "--", str(image)])
