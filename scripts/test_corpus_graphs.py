"""Survey scope, failure accounting, and isolation controls."""
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))
import corpus_graphs as survey


class CorpusGraphs(unittest.TestCase):
    def row(self, version='1.0.0', **changes):
        return dict(name='sample', version=version, manifest='/source/sample/Cargo.toml',
                    library=True, proc_macro=False, **changes)

    def test_versions_and_feature_modes_have_distinct_identities(self):
        rows = [self.row(), self.row('2.0.0')]
        default = survey.cases_for(rows, 'default', set(), [])
        minimal = survey.cases_for(rows, 'no-default', set(), [])
        self.assertEqual(len({c['id'] for c in default + minimal}), 4)
        self.assertTrue(all(c['default_features'] for c in default))
        self.assertFalse(any(c['default_features'] for c in minimal))
        matrix = [dict(name='sample', version='2.0.0', label='selected', default_features=False, features=['z', 'a'])]
        selected = survey.cases_for(rows, 'matrix', set(), matrix)
        self.assertEqual([(c['version'], c['selected_features']) for c in selected], [('2.0.0', ['a', 'z'])])
        with self.assertRaises(RuntimeError):
            survey.cases_for(rows[:1], 'matrix', set(), matrix)

    @unittest.skipIf(survey.tomllib is None, 'Python 3.11 needed for TOML parsing')
    def test_wrapper_keeps_dependency_features_and_path(self):
        case = dict(self.row(), default_features=False, selected_features=['alloc'])
        dependency = survey.tomllib.loads(survey.wrapper(case))['dependencies']['corpus_subject']
        self.assertEqual(dependency, dict(package='sample', path='/source/sample', **{'default-features': False, 'features': ['alloc']}))
        profile = survey.tomllib.loads(survey.wrapper(case))['profile']['release']
        self.assertEqual(profile, {'debug-assertions': False, 'overflow-checks': False, 'panic': 'abort'})

    def test_exclusions_and_timeouts_do_not_become_successes(self):
        rows = [dict(id=str(i), status=status) for i, status in enumerate([
            'pass', 'flux_failure', 'flux_timeout', 'offline_dependency', 'rustc_failure',
            'rustc_timeout', 'metadata_failure', 'no_library', 'rustc_hosted_proc_macro'])]
        result = survey.summarize(rows)
        self.assertEqual(result['cases'], 9)
        self.assertEqual(result['rustc_eligible'], 3)
        self.assertEqual(result['pass_percent'], 33.33)
        self.assertEqual(result['counts']['pass'], 1)

    def test_offline_hint_does_not_hide_version_conflicts(self):
        self.assertFalse(survey.offline_failure('failed to select a version; reminder: offline mode'))
        self.assertTrue(survey.offline_failure('no matching package named `missing` found'))
        self.assertEqual(survey.diagnostics('file.rs:2:4: error: missing method\nerror[E0001]: other failure'), ['missing method', 'other failure'])

    def test_cleanup_never_follows_managed_target_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            managed = root / 'managed'
            managed.mkdir()
            (managed / 'preserve').write_text('owned by another tool')
            target = root / 'target'
            target.symlink_to(managed, target_is_directory=True)
            self.assertEqual(survey.cleanup_target(target), 'externally_managed_retained')
            self.assertEqual((managed / 'preserve').read_text(), 'owned by another tool')
            self.assertTrue(target.is_symlink())
            owned = root / 'owned'
            owned.mkdir()
            (owned / 'artifact').write_text('temporary')
            self.assertEqual(survey.cleanup_target(owned), 'removed')
            self.assertFalse(owned.exists())

    def test_source_digest_detects_edits_not_timestamp_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            file = root / 'source.rs'
            file.write_text('old')
            before = survey.package_digest(root)
            file.touch()
            self.assertEqual(survey.package_digest(root), before)
            file.write_text('new')
            self.assertNotEqual(survey.package_digest(root), before)

    def test_non_library_and_proc_macro_roots_are_explicit_exclusions(self):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(output=Path(directory))
            with patch.object(survey, 'run_command') as command:
                for i, (changes, status) in enumerate([
                    ({'library': False}, 'no_library'),
                    ({'proc_macro': True}, 'rustc_hosted_proc_macro'),
                ]):
                    case = self.row()
                    case.update(changes, id=str(i), label='default')
                    self.assertEqual(survey.evaluate(case, args)['status'], status)
                command.assert_not_called()

    def test_metadata_failure_does_not_replace_rustc_control(self):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(output=Path(directory), cargo='cargo', flux=Path('/compiler/flux'),
                                   host_triple='aarch64-apple-darwin', timeout=1, keep_targets=False)
            case = dict(self.row(), id='case', label='default', default_features=True, selected_features=[])
            labels = []
            def run(command, root, label, timeout):
                labels.append(label)
                return {'exit': 1 if label == 'metadata' else 0, 'timeout': False}
            with patch.object(survey, 'run_command', side_effect=run):
                result = survey.evaluate(case,args)
            self.assertEqual(labels, ['rustc','metadata','flux'])
            self.assertEqual(result['status'], 'pass')

    def test_timeout_terminates_command_and_records_it_separately(self):
        with tempfile.TemporaryDirectory() as directory:
            result = survey.run_command([sys.executable, '-c', 'import time;time.sleep(30)'], Path(directory), 'timeout', .02)
            self.assertTrue(result['timeout'])
            self.assertNotEqual(result['exit'], 0)
            self.assertLess(result['seconds'], 5)


if __name__ == '__main__':
    unittest.main()
