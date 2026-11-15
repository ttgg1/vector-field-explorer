import copy
import json
from pathlib import Path
import tempfile
import unittest

from scripts.workflow import policy

ROOT = Path(__file__).resolve().parents[2]


class PolicyTests(unittest.TestCase):
    def setUp(self):
        self.config = policy.load_config(ROOT / '.opencode/agent-workflow.json')

    def test_config_rejects_unknown_boolean_integer_and_nan(self):
        for mutate in (lambda c: c.update(extra=1),
                       lambda c: c['limits'].update(max_attempts=True),
                       lambda c: c['prices'].update({'openai/x': {'input': float('nan'), 'output': 1, 'cache_read': 1, 'cache_write': 1}})):
            config = copy.deepcopy(self.config)
            mutate(config)
            with tempfile.TemporaryDirectory() as d:
                path = Path(d) / 'config.json'
                path.write_text(json.dumps(config))
                with self.assertRaises(ValueError):
                    policy.load_config(path)

    def test_config_rejects_unrepresentable_prices(self):
        config = copy.deepcopy(self.config)
        config['prices']['openai/x'] = {'input': 10 ** 400, 'output': 1, 'cache_read': 1, 'cache_write': 1}
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'config.json'
            path.write_text(json.dumps(config))
            with self.assertRaises(ValueError):
                policy.load_config(path)

    def test_catalog_requires_exact_enabled_openai_tool_chat_variant(self):
        models = []
        for role in self.config['roles'].values():
            models.append({'providerID': 'openai', 'modelID': role['model'].split('/', 1)[1],
                           'enabled': True, 'type': 'chat', 'capabilities': {'tools': True},
                           'variants': [{'id': role['effort']}], 'cost': []})
        refs = policy.validate_models(self.config, models)
        self.assertEqual(refs['implementer'], {'id': 'gpt-6.1-terra', 'providerID': 'openai', 'variant': 'medium'})
        with self.assertRaises(ValueError):
            policy.validate_models(self.config, [dict(m, providerID='other') for m in models])

    def test_review_skip_and_forced_escalation(self):
        self.assertFalse(policy.review_policy(self.config, ['docs/a.md'], 3, 1, 'trivial', True)['required'])
        for args in [(['docs/a.md'], 3, 1, 'trivial', False, False),
                     (['docs/a.md'], 3, 1, 'trivial', True, True),
                     (['src/x.rs'], 3, 1, 'trivial', True, False)]:
            self.assertTrue(policy.review_policy(self.config, *args)['required'])
        result = policy.review_policy(self.config, ['src/auth.rs'], 1, 0, 'trivial', True)
        self.assertEqual((result['risk'], result['role']), ('high', 'escalation'))
        with self.assertRaises(ValueError):
            policy.review_policy(self.config, ['a'], True, 0, 'trivial', True)

    def test_security_directory_and_nested_markdown_do_not_skip(self):
        self.assertEqual(policy.review_policy(self.config, ['src/auth/token.rs'], 1, 0, 'trivial', True)['risk'], 'high')
        self.assertTrue(policy.review_policy(self.config, ['src/notes.md'], 1, 0, 'trivial', True)['required'])

    def test_reject_ambiguous_or_empty_paths(self):
        for path in ('a//b', './a', '/a', 'a/../b', 'a\\b'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                policy.review_policy(self.config, [path], 1, 0, 'trivial', True)

    def test_review_json_rejects_blockers_with_approval_and_bad_paths(self):
        finding = {'file': 'src/main.rs', 'line': 1, 'severity': 'important', 'message': 'fix it'}
        with self.assertRaises(ValueError):
            policy.parse_review(json.dumps({'decision': 'approve', 'findings': [finding]}))
        finding['file'] = '../secret'
        with self.assertRaises(ValueError):
            policy.parse_review(json.dumps({'decision': 'block', 'findings': [finding]}))
        self.assertEqual(policy.parse_review('{"decision":"approve","findings":[]}')['decision'], 'approve')
