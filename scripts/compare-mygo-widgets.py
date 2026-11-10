#!/usr/bin/env python3
"""Render the same controls through MyGO and Plico. MyGO is read-only and optional."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mygo', type=Path, default=Path('/home/me/code/go/mygo'))
    parser.add_argument('--output', type=Path, default=Path('reports/mygo-widgets'))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = (root / args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    probe = output / 'probe.go'
    shutil.copyfile(root / 'scripts/mygo-widget-probe.go.txt', probe)
    env = dict(os.environ, PLICO_STYLE_OUTPUT=str(output), CGO_ENABLED='0')
    subprocess.run(['go', 'run', str(probe)], cwd=args.mygo, env=env, check=True)
    subprocess.run(['cargo', 'run', '--offline', '--locked', '--example', 'mygo_style_probe'],
                   cwd=root, env=env, check=True)
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=args.mygo, text=True).strip()
    (output / 'reference.txt').write_text(f'MyGO {revision}\n560x760 DIP, scale 1, light/dark\n')
    print(output)


if __name__ == '__main__':
    main()
