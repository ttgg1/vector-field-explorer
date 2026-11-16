import json
from pathlib import Path
import subprocess
import sys
import unittest

from scripts import metrics


ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / 'scripts/tests/fixtures/opencode-session-export.json'


class MetricsCliTests(unittest.TestCase):
    def test_session_export_groups_by_model_and_preserves_unknowns(self):
        report = metrics.summarize_exports([json.loads(FIXTURE.read_text())])

        deepseek = report['by_model']['opencode-go/deepseek-v4.1-flash#low']
        self.assertEqual(deepseek['tokens'], {
            'input': 100, 'output': 20, 'reasoning': 5, 'cache_read': 30, 'cache_write': 1,
        })
        self.assertEqual(deepseek['cost'], 0)
        self.assertEqual(deepseek['cost_source'], 'opencode_reported')
        glm = report['by_model']['opencode-go/glm-5.3-flash#low']
        self.assertIsNone(glm['tokens']['input'])
        self.assertIsNone(glm['cost'])
        self.assertIsNone(report['totals']['tokens']['input'])
        self.assertIsNone(report['totals']['cost'])
        self.assertIsNone(report['allowance_consumption'])

    def test_identical_repeated_export_is_deduplicated(self):
        export = json.loads(FIXTURE.read_text())
        report = metrics.summarize_exports([export, export])
        self.assertEqual(report['sessions'], 1)
        self.assertEqual(report['messages'], 2)

    def test_empty_export_is_unknown_not_zero_usage(self):
        export = {'info': {'id': 'ses-empty'}, 'messages': []}
        report = metrics.summarize_exports([export])
        self.assertEqual(report['sessions'], 1)
        self.assertEqual(report['messages'], 0)
        self.assertIsNone(report['totals']['tokens']['input'])
        self.assertIsNone(report['totals']['cost'])

    def test_empty_session_keeps_combined_totals_unknown(self):
        empty = {'info': {'id': 'ses-empty'}, 'messages': []}
        populated = json.loads(FIXTURE.read_text())
        report = metrics.summarize_exports([empty, populated])
        self.assertEqual(report['sessions'], 2)
        self.assertIsNone(report['totals']['tokens']['input'])
        self.assertIsNone(report['totals']['cost'])
        self.assertEqual(report['coverage']['sessions_without_assistant_messages'], 1)
        self.assertFalse(report['coverage']['session_usage_complete'])

    def test_conflicting_duplicate_message_is_rejected(self):
        export = json.loads(FIXTURE.read_text())
        conflicting = json.loads(FIXTURE.read_text())
        conflicting['messages'][0]['tokens']['input'] = 101
        with self.assertRaisesRegex(ValueError, 'conflicting duplicate'):
            metrics.summarize_exports([export, conflicting])

    def test_missing_message_model_is_not_guessed_from_session_default(self):
        export = {
            'info': {'id': 'ses-model-change', 'model': {'providerID': 'openai', 'id': 'old-model'}},
            'messages': [{'id': 'msg-unknown', 'type': 'assistant', 'tokens': {'input': 1}}],
        }
        report = metrics.summarize_exports([export])
        self.assertIn('unknown', report['by_model'])
        self.assertNotIn('openai/old-model', report['by_model'])

    def test_cli_reads_export_files_and_writes_json(self):
        result = subprocess.run(
            [sys.executable, str(ROOT / 'scripts/metrics.py'), str(FIXTURE)],
            cwd=ROOT, text=True, capture_output=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['sessions'], 1)


if __name__ == '__main__':
    unittest.main()
