#!/usr/bin/env python3
"""Compatibility entry point; scripts/icons.ts is the only Lucide generator."""
import os
from pathlib import Path
import sys

script = Path(__file__).resolve().with_name("icons.ts")
os.execvp("bun", ["bun", str(script), *sys.argv[1:]])
