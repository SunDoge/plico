#!/usr/bin/env python3
"""Record actual Parley fallback/weight selection without registering test fonts."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'reports/system-font-selection')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    for weight in (400, 500, 600, 700):
        for text in ('Hello Rust 0123456789', '中文 设置 连接 保存'):
            rows.append(dict(text=text, size=14, weight=weight, x=20,
                             baseline=32 + len(rows) * 40))
    summary = {}
    for name, family in (
        ('default', None),
        ('generic', 'sans-serif'),
        ('explicit', 'Noto Sans, Noto Sans CJK SC, sans-serif'),
    ):
        directory = out / name
        directory.mkdir(exist_ok=True)
        case = directory / 'case.json'
        case.write_text(json.dumps(dict(width=660, height=360, background='#ffffff',
                                       foreground='#000000', fontFamily=family,
                                       scales=[1], rows=rows), ensure_ascii=False, indent=2))
        subprocess.run(['cargo', 'run', '-p', 'plico-shell', '--example',
                        'text_quality_probe', '--', '--cases', str(case),
                        '--output', str(directory)], cwd=ROOT, check=True)
        lines = (directory / 'plico-text-quality-1x.txt').read_text().splitlines()
        records = []
        for line in lines:
            if line.startswith('row='):
                records.append(dict(row=line, fonts=[]))
            elif line.strip().startswith('resolved_family='):
                font = line.strip()
                if font not in records[-1]['fonts']:
                    records[-1]['fonts'].append(font)
        assert len(records) == len(rows)
        assert all(record['fonts'] for record in records)
        summary[name] = dict(requested_family=family, rows=records)
    (out / 'selection.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2))
    for name, result in summary.items():
        print(name, result['requested_family'])
        for record in result['rows']:
            print(record['row'])
            print('\n'.join('  ' + font for font in record['fonts']))


if __name__ == '__main__':
    main()
