#!/usr/bin/env python3
"""Compare selected feature consumers against Rustc in three Flux build modes."""
import argparse
import json
from pathlib import Path
import shlex
import subprocess

import corpus_graphs as survey

ROOT = Path(__file__).resolve().parent.parent


def manifest_for(case, inventory):
    manifest = '[workspace]\n[package]\nname="flux-feature-consumer"\nversion="0.0.0"\nedition="2024"\n[dependencies]\n'
    for dependency in case['dependencies']:
        row = next(r for r in inventory if r['name'] == dependency['name'] and r['version'] == dependency['version'])
        value = {'path': str(Path(row['manifest']).parent), 'default-features': dependency.get('default_features', True), 'features': dependency.get('features', [])}
        fields = ', '.join(json.dumps(key) + ' = ' + json.dumps(item) for key, item in value.items())
        manifest += json.dumps(dependency['name']) + '={' + fields + '}\n'
    return manifest + '[profile.dev]\npanic=\"abort\"\n[profile.release]\npanic=\"abort\"\n'


def matches_reference(root, label):
    return all((root / (label + '.' + stream)).read_bytes() == (root / ('rustc-run.' + stream)).read_bytes() for stream in ['stdout', 'stderr'])


def compare(case, args, inventory):
    root = args.output / case['name']
    root.mkdir(parents=True, exist_ok=True)
    result = {'name': case['name'], 'configuration': case, 'source_sha256': survey.digest(ROOT / case['source']), 'stages': {}}
    manifest = manifest_for(case, inventory)
    (root / 'src').mkdir(exist_ok=True)
    (root / 'src/main.rs').write_bytes((ROOT / case['source']).read_bytes())
    (root / 'Cargo.toml').write_text(manifest)
    manifest_path = str(root / 'Cargo.toml')
    steps = [('rustc-build', shlex.split(args.cargo) + ['build', '--offline', '-j1', '--manifest-path', manifest_path]),
             ('rustc-run', [str(root / 'target/debug/flux-feature-consumer')])]
    for label, flags in [('debug', []), ('fast', ['--fast-build']), ('release', ['--release'])]:
        steps += [('flux-build-' + label, [str(args.flux), 'build', '--offline', '--macro-host-compiler', 'rustc', '-j1', *flags, '--manifest-path', manifest_path]),
                  ('flux-run-' + label, [str(root / 'target/flux' / label / 'flux-feature-consumer')])]
    result['status'] = 'pass'
    for label, command in steps:
        stage = survey.run_command(command, root, label, args.timeout)
        result['stages'][label] = stage
        if stage['exit'] or stage['timeout']:
            result['status'] = label + ('-timeout' if stage['timeout'] else '-failure')
            result['diagnostics'] = survey.diagnostics((root / (label + '.stderr')).read_text(errors='replace'))
            break
        if label.startswith('flux-run-'):
            if not matches_reference(root, label):
                result['status'] = label + '-mismatch'
                break
    result['artifacts'] = survey.cleanup_target(root / 'target')
    survey.write_json(root / 'result.json', result)
    print(case['name'] + ': ' + result['status'], flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--inventory', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--flux', type=Path, default=ROOT / 'target/debug/flux')
    parser.add_argument('--timeout', type=int, default=300)
    args = parser.parse_args()
    args.output = args.output.resolve()
    if args.output.exists():
        raise RuntimeError('choose a new output directory to preserve consumer evidence')
    args.output.mkdir(parents=True)
    inventory = json.loads(args.inventory.read_text())['inventory']
    cases = json.loads(args.config.read_text())
    survey.write_json(args.output / 'run.json', {'configuration': cases, 'runner_sha256': survey.digest(Path(__file__)), 'helper_sha256': survey.digest(Path(survey.__file__)), 'flux_sha256': survey.digest(args.flux),
                      'fluxc_sha256': survey.digest(args.flux.with_name('fluxc')),
                      'rustc': subprocess.check_output(['rustc','-Vv'], text=True)})
    results = [compare(case, args, inventory) for case in cases]
    survey.write_json(args.output / 'results.json', results)


if __name__ == '__main__':
    main()
