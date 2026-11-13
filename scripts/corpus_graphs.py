#!/usr/bin/env python3
"""Screen registry library graphs against Rustc. Requires Python 3.11 or newer.

Use --cargo mbx for local scheduling; published commands use ordinary Cargo.
Each isolated case retains its manifest, lockfile, metadata and logs. Its target
folder is removed after measurement unless --keep-targets is set. Externally
managed target symlinks and their contents are always left untouched.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import signal
import subprocess
import threading
import time
try:
    import tomllib
except ImportError:
    tomllib = None

ROOT = Path(__file__).resolve().parent.parent
ACTIVE: set[subprocess.Popen] = set()
ACTIVE_LOCK = threading.Lock()
STOP = threading.Event()


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()


def package_digest(root):
    value = hashlib.sha256()
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            content = os.readlink(path)
        elif path.is_file():
            content = digest(path)
        else:
            continue
        value.update(str(path.relative_to(root)).encode() + b'\0' + content.encode() + b'\0')
    return value.hexdigest()


def inventory(registry):
    if tomllib is None:
        raise RuntimeError('this survey requires Python 3.11 or newer')
    rows = []
    for manifest in sorted(registry.glob('*/*/Cargo.toml')):
        data = tomllib.loads(manifest.read_text())
        package = data.get('package')
        if not package:
            continue
        lib = data.get('lib', {})
        library = bool(lib) or (package.get('autolib', True) and (manifest.parent / 'src/lib.rs').is_file())
        rows.append({'name': package['name'], 'version': package['version'],
                     'manifest': str(manifest.resolve()),
                     'manifest_sha256': digest(manifest),
                     'source_sha256': package_digest(manifest.parent),
                     'library': library, 'proc_macro': lib.get('proc-macro', lib.get('proc_macro', False)),
                     'features': sorted(data.get('features', {}))})
    return rows


def cases_for(rows, mode, names, matrix):
    cases = []
    for row in rows:
        if names and row['name'] not in names:
            continue
        variants = [{'label': mode, 'default_features': mode != 'no-default', 'features': []}]
        if mode == 'matrix':
            variants = [v for v in matrix if v['name'] == row['name'] and v['version'] == row['version']]
        for variant in variants:
            case = {**row, 'label': variant['label'],
                    'default_features': variant['default_features'],
                    'selected_features': sorted(variant.get('features', []))}
            identity = json.dumps(case, sort_keys=True).encode()
            case['id'] = hashlib.sha256(identity).hexdigest()[:16]
            cases.append(case)
    if mode == 'matrix':
        for variant in matrix:
            if not any(c['name'] == variant['name'] and c['version'] == variant['version'] and c['label'] == variant['label'] for c in cases):
                raise RuntimeError(f'matrix entry absent from inventory: {variant}')
    return cases


def wrapper(case):
    dependency = ('{ package = ' + json.dumps(case['name']) + ', path = ' +
                  json.dumps(str(Path(case['manifest']).parent)) + ', default-features = ' +
                  str(case['default_features']).lower() + ', features = ' +
                  json.dumps(case['selected_features']) + ' }')
    return ('[workspace]\n[package]\nname="flux-corpus-consumer"\nversion="0.0.0"\nedition="2024"\n'
            '[dependencies]\ncorpus_subject = ' + dependency + '\n'
            '[profile.release]\ndebug-assertions=false\noverflow-checks=false\npanic=\"abort\"\n')


def offline_failure(text):
    return any(marker in text for marker in ['no matching package named', 'attempting to make an HTTP request',
                                             'failed to download', 'unable to update'])


def diagnostics(text):
    return re.findall(r'^.*?\berror(?:\[[^]]+\])?: (.+)$', text, re.M)[:8]


def run_command(command, case_root, label, timeout):
    start = time.monotonic()
    with (case_root / (label + '.stdout')).open('wb') as stdout, (case_root / (label + '.stderr')).open('wb') as stderr:
        with ACTIVE_LOCK:
            if STOP.is_set():
                raise InterruptedError('survey stopped')
            process = subprocess.Popen(command, cwd=ROOT, stdout=stdout, stderr=stderr, start_new_session=True)
            ACTIVE.add(process)
        expired = False
        try:
            process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            expired = True
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            raise
        finally:
            with ACTIVE_LOCK:
                ACTIVE.discard(process)
    if STOP.is_set():
        raise InterruptedError('survey stopped')
    return {'command': command, 'exit': process.returncode, 'timeout': expired,
            'seconds': round(time.monotonic() - start, 2)}


def cleanup_target(target):
    if target.is_symlink():
        return 'externally_managed_retained'
    if target.exists():
        shutil.rmtree(target)
        return 'removed'
    return 'absent'


def evaluate(case, args):
    case_root = args.output / 'cases' / case['id']
    case_root.mkdir(parents=True, exist_ok=True)
    result_path = case_root / 'result.json'
    if result_path.exists():
        return json.loads(result_path.read_text())
    result = {**case, 'stages': {}}
    if not case['library']:
        result['status'] = 'no_library'
    elif case['proc_macro']:
        result['status'] = 'rustc_hosted_proc_macro'
    else:
        (case_root / 'src').mkdir(exist_ok=True)
        (case_root / 'src/lib.rs').write_text('pub use corpus_subject::*;\n')
        (case_root / 'Cargo.toml').write_text(wrapper(case))
        manifest = str(case_root / 'Cargo.toml')
        cargo = shlex.split(args.cargo)
        commands = [
            ('rustc', cargo + ['build', '--offline', '--release', '--lib', '-j1', '--manifest-path', manifest]),
            ('metadata', cargo + ['metadata', '--offline', '--format-version', '1', '--filter-platform', args.host_triple, '--manifest-path', manifest]),
            ('flux', [str(args.flux), 'build', '--offline', '--lib', '--macro-host-compiler', 'rustc', '--fast-build', '-j1', '--manifest-path', manifest]),
        ]
        result['status'] = 'pass'
        for label, command in commands:
            stage = run_command(command, case_root, label, args.timeout)
            result['stages'][label] = stage
            if label == 'metadata':
                # Metadata is evidence, not the eligibility control. Cargo can
                # fail to describe inactive target dependencies even when a
                # host library builds successfully. Still attempt Flux.
                continue
            if stage['timeout'] or stage['exit']:
                text = (case_root / (label + '.stderr')).read_text(errors='replace')
                result['diagnostics'] = diagnostics(text)
                result['status'] = label + '_timeout' if stage['timeout'] else (
                    'offline_dependency' if label != 'flux' and offline_failure(text) else label + '_failure')
                if label == 'flux':
                    failed = re.findall(r'compilation of (\S+) failed', text)
                    result['failed_crate'] = failed[-1] if failed else None
                    paths = re.findall(r'/registry/src/[^/]+/([^/]+)/', text)
                    result['failed_package_versions'] = sorted(set(paths))
                break
        if 'metadata' in result['stages'] and result['stages']['metadata']['exit'] == 0:
            metadata = json.loads((case_root / 'metadata.stdout').read_text())
            result['resolved_packages'] = [{'name': p['name'], 'version': p['version']} for p in metadata['packages'] if p['name'] != 'flux-corpus-consumer']
        # Only the isolated survey directory created above owns these outputs.
        if not args.keep_targets:
            result['artifacts'] = cleanup_target(case_root / 'target')
    write_json(result_path, result)
    print(f"{case['name']} {case['version']} {case['label']}: {result['status']}", flush=True)
    return result


def summarize(results):
    counts = {}
    groups = {}
    for result in results:
        status = result['status']
        counts[status] = counts.get(status, 0) + 1
        if status == 'flux_failure':
            key = json.dumps([result.get('failed_package_versions', []), result.get('diagnostics', [])[:1]])
            groups.setdefault(key, []).append(result['id'])
    eligible = sum(counts.get(s, 0) for s in ['pass', 'flux_failure', 'flux_timeout'])
    return {'cases': len(results), 'counts': counts, 'rustc_eligible': eligible,
            'pass_percent': round(100 * counts.get('pass', 0) / eligible, 2) if eligible else None,
            'diagnostic_groups': [{'key': json.loads(key), 'cases': ids} for key, ids in sorted(groups.items(), key=lambda x: -len(x[1]))]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--registry', type=Path, default=Path.home() / '.cargo/registry/src')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--flux', type=Path, default=ROOT / 'target/debug/flux')
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--timeout', type=int, default=180)
    parser.add_argument('--mode', choices=['default', 'no-default', 'matrix'], default='default')
    parser.add_argument('--names', default='')
    parser.add_argument('--matrix', type=Path)
    parser.add_argument('--keep-targets', action='store_true')
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.flux = args.flux.resolve()
    if args.jobs < 1 or args.timeout < 1:
        parser.error('jobs and timeout must be positive')
    if args.mode == 'matrix' and args.matrix is None:
        parser.error('matrix mode requires --matrix')
    args.host_triple = subprocess.check_output(['rustc', '-vV'], text=True).split('host: ')[1].splitlines()[0]
    args.output.mkdir(parents=True, exist_ok=True)
    rows = inventory(args.registry)
    matrix = json.loads(args.matrix.read_text()) if args.matrix else []
    cases = cases_for(rows, args.mode, set(filter(None, args.names.split(','))), matrix)
    provenance = {'inventory': rows, 'cases': cases, 'runner_sha256': digest(Path(__file__)),
                  'flux_sha256': digest(args.flux),
                  'fluxc_sha256': digest(args.flux.with_name('fluxc')),
                  'rustc': subprocess.check_output(['rustc', '-Vv'], text=True),
                  'host': platform.platform(), 'mode': args.mode,
                  'environment_sha256': hashlib.sha256(json.dumps({k: os.environ.get(k) for k in [
                      'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET',
                      'RUSTUP_TOOLCHAIN', 'RUSTC_WRAPPER', 'FLUX_BUILTINS_SOURCE',
                      'CC', 'CXX', 'CFLAGS', 'CXXFLAGS']}, sort_keys=True).encode()).hexdigest(),
                  'timeout': args.timeout, 'cargo': args.cargo}
    run_path = args.output / 'run.json'
    if run_path.exists() and json.loads(run_path.read_text()) != provenance:
        raise RuntimeError('survey inputs changed; choose a new output directory')
    write_json(run_path, provenance)
    pool = concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs)
    try:
        results = list(pool.map(lambda case: evaluate(case, args), cases))
    except BaseException:
        STOP.set()
        with ACTIVE_LOCK:
            for process in list(ACTIVE):
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        pool.shutdown(wait=True, cancel_futures=True)
        raise
    else:
        pool.shutdown()
    if digest(args.flux) != provenance['flux_sha256'] or digest(args.flux.with_name('fluxc')) != provenance['fluxc_sha256']:
        raise RuntimeError('compiler binaries changed during survey; results are unqualified')
    changed = [row for row in rows if package_digest(Path(row['manifest']).parent) != row['source_sha256']]
    write_json(args.output / 'source-integrity.json', {'checked_packages': len(rows), 'changed': changed})
    changed_keys = {(r['name'], r['version']) for r in changed}
    for result in results:
        if any((p['name'], p['version']) in changed_keys for p in result.get('resolved_packages', [])):
            result['unqualified_status'] = result['status']
            result['status'] = 'source_changed'
    write_json(args.output / 'results.json', results)
    write_json(args.output / 'summary.json', summarize(results))
    print(json.dumps(summarize(results)['counts'], sort_keys=True), flush=True)


if __name__ == '__main__':
    main()
