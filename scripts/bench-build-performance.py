#!/usr/bin/env python3
"""Benchmark Gallery edits in a copied checkout. Configure with PLICO_BENCH_* env vars.

PLICO_BENCH_GROUP: stable (default), nightly, or all.
PLICO_BENCH_OUTPUT: fresh ignored output directory (default reports/build-performance).
PLICO_BENCH_SAMPLES: unique edits per workload (default 5).
PLICO_BENCH_VARIANTS: optional comma-separated variant names to run.
PLICO_BENCH_WILD: linker path (default ~/.cargo/bin/wild).
PLICO_BENCH_RUNTIME_ONLY=1 runs the existing CPU UI probe for selected stable profiles.
Uses real Cargo/rustc, bypassing wrappers. No changes to the original app/config.
An existing output directory resumes completed measurements.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import struct
import subprocess
import time
import zlib

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get('PLICO_BENCH_OUTPUT', ROOT / 'reports/build-performance')).resolve()
SAMPLES = int(os.environ.get('PLICO_BENCH_SAMPLES', '5'))
GROUP = os.environ.get('PLICO_BENCH_GROUP', 'stable')
assert GROUP in ('stable', 'nightly', 'all')
assert SAMPLES >= 3
OUT.mkdir(parents=True, exist_ok=True)
SOURCE = OUT / 'source'
RESULTS = OUT / f'{GROUP}-results.json'
ROWS = json.loads(RESULTS.read_text()) if RESULTS.exists() else []


def save():
    RESULTS.write_text(json.dumps(ROWS, indent=2) + '\n')


def command(cmd, env, log):
    begin = time.perf_counter()
    with log.open('w') as stream:
        result = subprocess.run(cmd, cwd=SOURCE, env=env, stdout=stream, stderr=subprocess.STDOUT)
    seconds = time.perf_counter() - begin
    if result.returncode:
        raise RuntimeError(f'command failed ({result.returncode}), see {log}')
    return seconds


def png_fingerprint(path):
    """Compare IHDR and inflated PNG scanlines; equality guarantees identical pixels."""
    raw = path.read_bytes()
    assert raw[:8] == b'\x89PNG\r\n\x1a\n'
    header, compressed = None, bytearray()
    offset = 8
    while offset < len(raw):
        size = struct.unpack('>I', raw[offset:offset + 4])[0]
        kind = raw[offset + 4:offset + 8]
        data = raw[offset + 8:offset + 8 + size]
        if kind == b'IHDR':
            header = data
        elif kind == b'IDAT':
            compressed.extend(data)
        offset += size + 12
    assert header is not None and header[8:10] in (b'\x08\x06', b'\x08\x02')
    return hashlib.sha256(header + zlib.decompress(compressed)).hexdigest()


def snapshot():
    if SOURCE.exists():
        hashes = json.loads((OUT / 'source-hashes.json').read_text())
        for relative, expected in hashes.items():
            if relative.endswith('.rs') or Path(relative).name in ('Cargo.toml', 'Cargo.lock') or relative.startswith('.cargo/'):
                actual = hashlib.sha256((SOURCE / relative).read_bytes()).hexdigest()
                if actual != expected:
                    raise RuntimeError(f'copied build input changed: {relative}; use a fresh output directory')
        return
    SOURCE.mkdir()
    files = subprocess.check_output(
        ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=ROOT
    ).decode().split('\0')
    hashes = {}
    for relative in sorted(set(files)):
        if not relative:
            continue
        src = ROOT / relative
        if not src.is_file():
            continue
        dst = SOURCE / relative
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst, follow_symlinks=False)
        hashes[relative] = hashlib.sha256(src.read_bytes()).hexdigest()
    (OUT / 'source-hashes.json').write_text(json.dumps(hashes, indent=2) + '\n')


def env_for(toolchain):
    env = os.environ.copy()
    # Avoid inherited compiler/cache/profile configuration in controlled comparisons.
    for key in list(env):
        if key.startswith(('CARGO_PROFILE_', 'CARGO_TARGET_', 'MBX_')) or key in (
            'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTDOC',
            'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTC_BOOTSTRAP',
        ):
            env.pop(key)
    rustc = subprocess.check_output(['rustup', 'which', '--toolchain', toolchain, 'rustc'], text=True).strip()
    env.update(RUSTC=rustc, RUSTUP_TOOLCHAIN=toolchain, CARGO_INCREMENTAL='1',
               RUSTC_WRAPPER='', RUSTC_WORKSPACE_WRAPPER='',
               CARGO_TARGET_DIR=str(OUT / ('target-' + toolchain)))
    return env


def variant(name, opt=0, debug=1, deps=2, workspace_debug=2, extra=(), unstable=()):
    configs = [f'profile.dev.package.gallery.opt-level={opt}',
               'profile.dev.package.gallery.debug=' + json.dumps(debug)]
    if deps is not None:
        configs.append('profile.dev.package."*".debug=' + json.dumps(deps))
    if workspace_debug is not None:
        configs.append('profile.dev.debug=' + json.dumps(workspace_debug))
    configs += list(extra)
    return dict(name=name, configs=configs, unstable=list(unstable))


STABLE = [
    variant('current', opt=1, debug=2),
    variant('app-opt0-debug2', debug=2),
    variant('app-opt0-debug1'),
    variant('app-opt0-lines', debug='line-tables-only'),
    variant('app-opt0-no-debug', debug=0),
    variant('app-opt0-debug1-deps0', deps=False),
    variant('guide-debug-policy', debug='line-tables-only', deps=False,
            workspace_debug='line-tables-only'),
    variant('current-repeat', opt=1, debug=2),
    variant('guide-debug-only', opt=1, debug='line-tables-only', deps=False,
            workspace_debug='line-tables-only'),
]
WILD = os.environ.get('PLICO_BENCH_WILD', str(Path.home() / '.cargo/bin/wild'))
NIGHTLY = [
    variant('nightly-llvm-opt0'),
    variant('nightly-threads8', extra=['profile.dev.package.gallery.rustflags=["-Zthreads=8"]'],
            unstable=['profile-rustflags']),
    variant('nightly-cranelift', extra=['profile.dev.package.gallery.codegen-backend="cranelift"'],
            unstable=['codegen-backend']),
    variant('nightly-wild', extra=[
        'profile.dev.package.gallery.rustflags='
        + json.dumps(['-Clinker=clang', '-Clink-arg=--ld-path=' + WILD])],
            unstable=['profile-rustflags']),
    variant('nightly-llvm-opt0-repeat'),
]


def run_group(group, variants):
    env = env_for(group)
    cargo = subprocess.check_output(['rustup', 'which', '--toolchain', group, 'cargo'], text=True).strip()
    metadata = subprocess.check_output([env['RUSTC'], '-Vv'], env=env, text=True)
    (OUT / f'{group}-toolchain.txt').write_text(metadata)
    path = SOURCE / 'apps/gallery/src/workspace.rs'
    original = path.read_bytes()
    label = b'Project workspace'
    layout = b'column(ui).gap(m.space(2)) + |ui| {'
    assert original.count(label) == 1 and original.count(layout) == 1
    completed = next((r for r in ROWS if r['group'] == group
                      and len(r.get('captures', {})) == 2
                      and all(c['matches_baseline'] for c in r['captures'].values())), None)
    reference_pixels = ({scene: c['pixel_sha256'] for scene, c in completed['captures'].items()}
                        if completed else {})
    try:
        for spec in variants:
            name = spec['name']
            base = [cargo]
            for flag in spec['unstable']:
                base += ['-Z', flag]
            base += ['build', '--locked', '--offline', '-j', '6', '-p', 'gallery', '--bin', 'gallery',
                     '--message-format=json']
            for config in spec['configs']:
                base += ['--config', config]
            existing = next((r for r in ROWS if r['group'] == group and r['name'] == name
                             and r.get('command') == base and len(r.get('samples', [])) == 2 * SAMPLES
                             and r.get('median_seconds')), None)
            if existing:
                row = existing
                if len(row.get('captures', {})) == 2 and all(
                    c['matches_baseline'] for c in row['captures'].values()
                ):
                    for scene, capture in row['captures'].items():
                        reference_pixels.setdefault(scene, capture['pixel_sha256'])
                    print(f'{group}/{name}: reusing completed measurements and validation', flush=True)
                    continue
                print(f'{group}/{name}: resuming validation', flush=True)
            else:
                row = dict(group=group, name=name, configs=spec['configs'], samples=[], command=base)
                ROWS[:] = [r for r in ROWS if not (r['group'] == group and r['name'] == name)]
                ROWS.append(row)
                path.write_bytes(original)
                print(f'{group}/{name}: warming dependencies and variant', flush=True)
                row['warmup_seconds'] = command(base, env, OUT / f'{group}-{name}-warmup.log')
                # Seed a changed source before measuring to avoid counting first-edit setup.
                path.write_bytes(original.replace(label, (f'Project workspace warmup {time.time_ns()}').encode(), 1))
                row['edit_warmup_seconds'] = command(base, env, OUT / f'{group}-{name}-edit-warmup.log')
                for i in range(SAMPLES):
                    # Alternate text/layout order to avoid giving one workload a fixed order advantage.
                    for workload in (['text', 'layout'] if i % 2 == 0 else ['layout', 'text']):
                        unique = f'{name}-{workload}-{i}-{time.time_ns()}'
                        if workload == 'text':
                            edited = original.replace(label, ('Project workspace ' + unique).encode(), 1)
                        else:
                            # Unique spacing values avoid compiler/action cache replay.
                            gap = 2.01 + (time.time_ns() % 10000000) / 1000000000
                            edited = original.replace(layout, f'column(ui).gap(plico::Length::Px(m.space(2) as f32 + {gap:.9f})) + |ui| {{'.encode(), 1)
                        path.write_bytes(edited)
                        log = OUT / f'{group}-{name}-{workload}-{i}.log'
                        seconds = command(base, env, log)
                        changed = []
                        for line in log.read_text().splitlines():
                            try:
                                msg = json.loads(line)
                            except json.JSONDecodeError:
                                continue
                            if msg.get('reason') == 'compiler-artifact' and not msg['fresh']:
                                changed.append(dict(package_id=msg['package_id'], target=msg['target']['name'],
                                                    kind=msg['target']['kind']))
                        if len(changed) != 2 or any(a['target'] != 'gallery' for a in changed):
                            raise RuntimeError(f'unexpected recompilation in {log}: {changed}')
                        row['samples'].append(dict(workload=workload, seconds=seconds, artifacts=changed))
                        save()
                        print(f'  {workload} {i + 1}/{SAMPLES}: {seconds:.3f}s', flush=True)
                row['median_seconds'] = {
                    work: statistics.median(x['seconds'] for x in row['samples'] if x['workload'] == work)
                    for work in ('text', 'layout')
                }
            # Restore source and validate the actual application binary with scripted GUI captures.
            path.write_bytes(original)
            command(base, env, OUT / f'{group}-{name}-restore.log')
            binary = Path(env['CARGO_TARGET_DIR']) / 'debug/gallery'
            row['binary_bytes'] = binary.stat().st_size
            comment = subprocess.run(['readelf', '-p', '.comment', str(binary)], capture_output=True, text=True)
            row['binary_comment'] = comment.stdout
            if name == 'nightly-wild':
                assert 'Linker: Wild' in comment.stdout, comment.stdout
            row['captures'] = {}
            for scene in ('controls', 'workspace'):
                png = OUT / f'{group}-{name}-{scene}.png'
                command([str(binary), '--scenario', str(SOURCE / f'apps/gallery/scenarios/{scene}.json'),
                         '--capture', str(png)], env, OUT / f'{group}-{name}-{scene}-capture.log')
                pixels = png_fingerprint(png)
                if scene not in reference_pixels:
                    reference_pixels[scene] = pixels
                row['captures'][scene] = dict(pixel_sha256=pixels, matches_baseline=pixels == reference_pixels[scene])
                if pixels != reference_pixels[scene]:
                    raise RuntimeError(f'capture pixel mismatch: {name}/{scene}')
            save()
            print(f'{group}/{name}: {row["median_seconds"]}, {row["binary_bytes"] / 2**20:.1f} MiB; captures match', flush=True)
    finally:
        path.write_bytes(original)
        save()


def run_runtime():
    """Compare CPU UI preparation using the existing probe; build time excluded."""
    env = env_for('stable')
    cargo = subprocess.check_output(['rustup', 'which', '--toolchain', 'stable', 'cargo'], text=True).strip()
    manifest = SOURCE / 'apps/gallery/Cargo.toml'
    saved = manifest.read_bytes()
    probe = SOURCE / 'apps/gallery/src/build_perf_probe.rs'
    probe.write_bytes((SOURCE / 'docs/experiments/dev-build-probe.rs').read_bytes())
    runtime_rows = []
    cpu = min(os.sched_getaffinity(0))
    try:
        manifest.write_bytes(saved + b'\n[[bin]]\nname = "build-perf-probe"\npath = "src/build_perf_probe.rs"\nrequired-features = ["desktop"]\n')
        specs = [STABLE[0], STABLE[2], STABLE[6]]
        for round_number in range(3):
            order = specs if round_number % 2 == 0 else list(reversed(specs))
            for spec in order:
                cmd = [cargo, 'build', '--locked', '--offline', '-j', '6', '-p', 'gallery', '--bin', 'build-perf-probe']
                for config in spec['configs']:
                    cmd += ['--config', config]
                command(cmd, env, OUT / f'runtime-{spec["name"]}-{round_number}-build.log')
                binary = Path(env['CARGO_TARGET_DIR']) / 'debug/build-perf-probe'
                result = subprocess.run([str(binary)], cwd=SOURCE, env=env, capture_output=True, text=True,
                                        preexec_fn=lambda: os.sched_setaffinity(0, {cpu}), check=True)
                (OUT / f'runtime-{spec["name"]}-{round_number}.csv').write_text(result.stdout)
                for line in result.stdout.splitlines():
                    page, median, p95, declaration, checksum = line.split(',')
                    runtime_rows.append(dict(name=spec['name'], round=round_number, page=page,
                                             median_us=float(median), p95_us=float(p95),
                                             declaration_us=int(declaration), checksum=int(checksum), cpu=cpu))
                (OUT / 'runtime-results.json').write_text(json.dumps(runtime_rows, indent=2) + '\n')
                print(f'runtime/{spec["name"]}/{round_number}: complete', flush=True)
        for page in {r['page'] for r in runtime_rows}:
            assert len({r['checksum'] for r in runtime_rows if r['page'] == page}) == 1, page
    finally:
        manifest.write_bytes(saved)
        probe.unlink()


if __name__ == '__main__':
    snapshot()
    selected = set(filter(None, os.environ.get('PLICO_BENCH_VARIANTS', '').split(',')))
    if os.environ.get('PLICO_BENCH_RUNTIME_ONLY') == '1':
        run_runtime()
        raise SystemExit(0)
    if GROUP in ('stable', 'all'):
        run_group('stable', [v for v in STABLE if not selected or v['name'] in selected])
    if GROUP in ('nightly', 'all'):
        run_group('nightly', [v for v in NIGHTLY if not selected or v['name'] in selected])
