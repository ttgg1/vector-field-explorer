from pathlib import Path
import unittest

from scripts.workflow import metrics, policy


class MetricsTests(unittest.TestCase):
    def setUp(self):
        self.config = policy.load_config(Path(__file__).resolve().parents[2] / '.opencode/agent-workflow.json')
        self.message = {'id': 'm1', 'model': {'id': 'gpt-6.1-terra', 'providerID': 'openai'},
                        'tokens': {'input': 100, 'output': 20, 'reasoning': 5, 'cache': {'read': 10, 'write': 2}},
                        'cost': 0.5}

    def test_raw_usage_and_deduplicated_summary(self):
        record = metrics.message_record('t', 'implementer', 's', self.message, self.config)
        tasks = [{'id': 't', 'status': 'merged', 'attempts': 1, 'reviews': 1, 'rejections': 0, 'escalations': 0, 'merged_lines': 12}]
        summary = metrics.summarize([record, record], tasks, True)
        self.assertEqual(summary['total_tokens'], 132)
        self.assertEqual(summary['cost'], 0.5)
        self.assertEqual(summary['useful_merged_lines_per_million_tokens'], 12 / 132 * 1000000)
        conflicting = dict(record, cost=1.0)
        with self.assertRaises(ValueError):
            metrics.summarize([record, conflicting], tasks, True)

    def test_unknown_semantics_and_failed_spend(self):
        message = dict(self.message, id='failed', cost=0, error={'message': 'failure'})
        record = metrics.message_record('t', 'implementer', 's', message, self.config)
        summary = metrics.summarize([record], [], None)
        self.assertIsNone(summary['total_tokens'])
        self.assertIsNone(summary['cost'])
        self.assertIsNone(summary['tokens_per_accepted'])

    def test_configured_rates_count_cache_and_reasoning_once(self):
        self.config['output_includes_reasoning'] = True
        self.config['prices']['openai/gpt-6.1-terra'] = {'input': 2, 'output': 4, 'cache_read': 1, 'cache_write': 3}
        message = dict(self.message)
        message.pop('cost')
        record = metrics.message_record('t', 'implementer', 's', message, self.config)
        self.assertAlmostEqual(record['cost'], (100*2 + 20*4 + 10 + 2*3)/1000000)

    def test_separate_reasoning_and_missing_usage(self):
        self.config['output_includes_reasoning'] = False
        self.config['prices']['openai/gpt-6.1-terra'] = {'input': 2, 'output': 4, 'cache_read': 1, 'cache_write': 3}
        message = dict(self.message)
        message.pop('cost')
        record = metrics.message_record('t', 'implementer', 's', message, self.config)
        self.assertAlmostEqual(record['cost'], (100*2 + 25*4 + 10 + 2*3)/1000000)
        self.assertEqual(metrics.summarize([record], [], False)['total_tokens'], 137)
        message['tokens'] = {'input': 1}
        record = metrics.message_record('t', 'implementer', 's', message, self.config)
        self.assertIsNone(record['cost'])
        self.assertIsNone(metrics.summarize([record], [], False)['total_tokens'])

    def test_missing_reasoning_and_unrelated_catalog_price_are_unknown(self):
        message = dict(self.message)
        message.pop('cost')
        message['tokens'] = {'input': 100, 'output': 20, 'cache': {'read': 10, 'write': 2}}
        self.config['output_includes_reasoning'] = True
        wrong = {'providerID': 'openai', 'modelID': 'other', 'cost': [{'input': 2, 'output': 4, 'cache': {'read': 1, 'write': 3}}]}
        record = metrics.message_record('t', 'implementer', 's', message, self.config, wrong)
        self.assertIsNone(record['cost'])
        self.assertIsNone(metrics.summarize([record], [], True)['total_tokens'])
