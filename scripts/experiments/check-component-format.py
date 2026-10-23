#!/usr/bin/env python3
"""Exercise the native CLI formatter; the old Python formatter is retired."""
from pathlib import Path
import subprocess
ROOT = Path(__file__).resolve().parents[2]
subprocess.run(["cargo", "test", "-p", "plico-cli", "formatting"], cwd=ROOT, check=True)
subprocess.run(["cargo", "run", "-p", "plico-cli", "--", "fmt", "--check", "-p", "plico-gallery"], cwd=ROOT, check=True)
