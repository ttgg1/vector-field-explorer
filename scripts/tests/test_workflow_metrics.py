from pathlib import Path
import copy
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
        self.assertEqual(summary['merged'], 1)
        self.assertNotIn('useful_merged_lines_per_million_tokens', summary)
        conflicting = dict(record, cost=1.0)
        with self.assertRaises(ValueError):
            metrics.summarize([record, conflicting], tasks, True)

    def test_mode_aggregation_preserves_unknown_cost_and_allowance(self):
        openai = metrics.message_record('t', 'implementer', 's', self.message, self.config)
        go_config = copy_config = dict(self.config)
        go_config['roles'] = {'implementer': {'model': 'opencode-go/deepseek-v4.1-flash'}}
        message = {'id': 'go', 'model': {'id': 'deepseek-v4.1-flash', 'providerID': 'opencode-go'},
                   'tokens': {'input': 4, 'output': 2, 'reasoning': 1, 'cache': {'read': 1, 'write': 0}}}
        go = metrics.message_record('t2', 'implementer', 's2', message, go_config)
        report = metrics.summarize([openai, go], [], True)
        self.assertIn('by_mode', report)
        self.assertIsNone(go['cost'])
        self.assertIsNone(go['allowance'])
        self.assertIsNone(report['by_mode']['opencode-go']['cost'])
        self.assertEqual(report['by_mode']['opencode-go']['accepted'], 0)
        self.assertEqual(report['by_mode']['opencode-go']['merged'], 0)
        self.assertNotIn('cost_per_accepted', report)
        self.assertNotIn('tokens_per_accepted', report)
        self.assertNotIn('cost_per_accepted', report['by_mode']['opencode-go'])

    def test_mode_totals_and_outcome_counts_do_not_claim_unit_efficiency(self):
        openai = metrics.message_record('a', 'implementer', 's1', self.message, self.config)
        go_config = dict(self.config)
        go_config['roles'] = {'implementer': {'model': 'opencode-go/deepseek-v4.1-flash'}}
        go_message = {'id': 'go', 'model': {'id': 'deepseek-v4.1-flash', 'providerID': 'opencode-go'},
                      'tokens': {'input': 4, 'output': 2, 'reasoning': 1, 'cache': {'read': 1, 'write': 0}}}
        go = metrics.message_record('b', 'implementer', 's2', go_message, go_config)
        tasks = [{'id': 'a', 'status': 'merged', 'attempts': 1, 'reviews': 1, 'rejections': 0,
                  'escalations': 0, 'merged_lines': 12, 'mode': 'openai'},
                 {'id': 'b', 'status': 'accepted', 'attempts': 1, 'reviews': 0, 'rejections': 0,
                  'escalations': 0, 'merged_lines': None, 'mode': 'opencode-go'}]
        report = metrics.summarize([openai, go], tasks, True)
        self.assertEqual(report['by_mode']['openai']['merged'], 1)
        self.assertEqual(report['by_mode']['opencode-go']['accepted'], 1)
        self.assertEqual(report['by_mode']['opencode-go']['totals']['input'], 4)
        self.assertIsNone(report['by_mode']['opencode-go']['cost'])
        for key in ('tokens_per_accepted', 'cost_per_accepted', 'tokens_per_merged',
                    'cost_per_merged', 'useful_merged_lines_per_million_tokens'):
            self.assertNotIn(key, report)
            self.assertNotIn(key, report['by_mode']['openai'])

    def test_unknown_semantics_and_failed_spend(self):
        message = dict(self.message, id='failed', cost=0, error={'message': 'failure'})
        record = metrics.message_record('t', 'implementer', 's', message, self.config)
        summary = metrics.summarize([record], [], None)
        self.assertIsNone(summary['total_tokens'])
        self.assertIsNone(summary['cost'])
        self.assertNotIn('tokens_per_accepted', summary)

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

    def test_f4_zero_sentinel_with_positive_configured_or_catalog_rates_is_estimated(self):
        self.config['output_includes_reasoning'] = True
        rates = {'input': 2, 'output': 4, 'cache_read': 1, 'cache_write': 3}
        catalog = {'providerID': 'openai', 'modelID': 'gpt-6.1-terra',
                   'cost': [{'input': 2, 'output': 4, 'cache': {'read': 1, 'write': 3}}]}
        for source in ('configured', 'catalog'):
            with self.subTest(source=source):
                self.config['prices'] = {'openai/gpt-6.1-terra': rates} if source == 'configured' else {}
                record = metrics.message_record('t', 'implementer', 's', dict(self.message, cost=0), self.config, catalog)
                self.assertAlmostEqual(record['cost'], 0.000296)
                self.assertEqual(record['cost_source'], 'estimated')
                self.assertTrue(record['coverage']['cost'])

    def test_f4_explicit_known_zero_and_positive_reported_cost_remain_authoritative(self):
        self.config['prices']['openai/gpt-6.1-terra'] = {'input': 2, 'output': 4, 'cache_read': 1, 'cache_write': 3}
        for message, want in ((dict(self.message, cost=0, cost_known=True), 0), (self.message, 0.5)):
            with self.subTest(cost=want):
                record = metrics.message_record('t', 'implementer', 's', message, self.config)
                self.assertEqual(record['cost'], want)
                self.assertEqual(record['cost_source'], 'reported')

    def test_f4_zero_sentinel_does_not_invent_cost_with_unknown_counts_or_semantics(self):
        self.config['prices']['openai/gpt-6.1-terra'] = {'input': 2, 'output': 4, 'cache_read': 1, 'cache_write': 3}
        record = metrics.message_record('t', 'implementer', 's', dict(self.message, cost=0), self.config)
        self.assertIsNone(record['cost'])
        self.config['output_includes_reasoning'] = True
        record = metrics.message_record('t', 'implementer', 's', dict(self.message, cost=0, tokens={}), self.config)
        self.assertIsNone(record['cost'])

    def test_f4_all_zero_rates_only_estimate_free_when_usage_and_semantics_supported(self):
        self.config['output_includes_reasoning'] = True
        self.config['prices']['openai/gpt-6.1-terra'] = dict.fromkeys(('input', 'output', 'cache_read', 'cache_write'), 0)
        record = metrics.message_record('t', 'implementer', 's', dict(self.message, cost=0), self.config)
        self.assertEqual((record['cost'], record['cost_source']), (0, 'estimated'))
        record = metrics.message_record('t', 'implementer', 's', dict(self.message, cost=0, tokens={}), self.config)
        self.assertIsNone(record['cost'])
