"""Native comparison output and manifest controls."""
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).parent))
import corpus_consumers as consumers
import corpus_graphs as survey


class Consumers(unittest.TestCase):
    @unittest.skipIf(survey.tomllib is None, 'Python 3.11 needed for TOML parsing')
    def test_manifest_keeps_feature_flags_and_literal_path(self):
        case = {'dependencies': [{'name': 'sample', 'version': '1.0.0', 'default_features': False, 'features': ['alloc']}]}
        inventory = [{'name': 'sample', 'version': '1.0.0', 'manifest': '/source/with: space/sample/Cargo.toml'}]
        data = survey.tomllib.loads(consumers.manifest_for(case, inventory))
        self.assertEqual(data['dependencies']['sample']['path'], '/source/with: space/sample')
        self.assertEqual(data['dependencies']['sample']['features'], ['alloc'])
        self.assertFalse(data['dependencies']['sample']['default-features'])

    def test_both_stdout_and_stderr_must_match(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for label in ['rustc-run', 'flux-run-debug']:
                (root / (label + '.stdout')).write_bytes(b'output\n')
                (root / (label + '.stderr')).write_bytes(b'error\n')
            self.assertTrue(consumers.matches_reference(root, 'flux-run-debug'))
            (root / 'flux-run-debug.stderr').write_bytes(b'different\n')
            self.assertFalse(consumers.matches_reference(root, 'flux-run-debug'))
            (root / 'flux-run-debug.stderr').write_bytes(b'error\n')
            (root / 'flux-run-debug.stdout').write_bytes(b'different\n')
            self.assertFalse(consumers.matches_reference(root, 'flux-run-debug'))


if __name__ == '__main__':
    unittest.main()
