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

    def test_mode_configuration_and_catalog_routes(self):
        self.assertEqual(self.config['selected_mode'], 'openai')
        self.assertEqual(self.config['roles']['implementer'], self.config['mode_roles']['implementer_standard'])
        raw = json.loads((ROOT / '.opencode/agent-workflow.json').read_text())
        self.assertEqual(set(raw['modes']), {'openai', 'opencode-go'})
        for mode, expected in {
            'openai': [('gpt-6.1-sol', 'medium'), ('gpt-6-luna', 'low'), ('gpt-6-luna', 'medium'),
                       ('gpt-6.1-sol', 'high'), ('gpt-6.1-sol', 'medium'), ('gpt-6.1-sol', 'high'), ('gpt-6.1-sol', 'high')],
            'opencode-go': [('deepseek-v4.1-flash', 'low'), ('deepseek-v4.1-flash', 'low'),
                            ('deepseek-v4.1-flash', 'high'), ('deepseek-v4-pro', 'high'),
                            ('gpt-6-luna', 'medium'), ('gpt-6-luna', 'high'), ('gpt-6-luna', 'high')],
        }.items():
            names = ('orchestrator', 'implementer_simple', 'implementer_standard', 'implementer_complex',
                     'reviewer_standard', 'reviewer_complex', 'recovery')
            for name, (model, effort) in zip(names, expected):
                self.assertEqual(raw['modes'][mode][name], {'model': f'{mode}/{model}', 'effort': effort})
        raw['workflow']['mode'] = 'opencode-go'
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'config.json'
            path.write_text(json.dumps(raw))
            config = policy.load_config(path)
        self.assertEqual(config['selected_mode'], 'opencode-go')
        self.assertEqual(config['roles']['reviewer'], config['mode_roles']['reviewer_standard'])
        catalog = [{'providerID': r['model'].split('/')[0], 'modelID': r['model'].split('/')[1],
                    'name': 'DeepSeek V4.1 Flash 1017' if 'flash' in r['model'] else 'other',
                    'enabled': True, 'status': 'active', 'type': 'chat', 'capabilities': {'tools': True},
                    'variants': [{'id': r['effort']}]} for r in config['mode_roles'].values()]
        self.assertEqual(catalog[0]['name'], 'DeepSeek V4.1 Flash 1017')
        refs = policy.validate_models(config, catalog)
        self.assertEqual(refs['implementer_standard'], {'providerID': 'opencode-go', 'id': 'deepseek-v4.1-flash', 'variant': 'high'})
        for change in ('missing', 'disabled', 'provider', 'tools', 'variant', 'type'):
            bad = copy.deepcopy(catalog)
            for model in bad:
                if model['modelID'] == 'deepseek-v4.1-flash':
                    if change == 'disabled': model['enabled'] = False
                    if change == 'provider': model['providerID'] = 'openai'
                    if change == 'tools': model['capabilities']['tools'] = False
                    if change == 'variant': model['variants'] = []
                    if change == 'type': model['type'] = 'embedding'
            if change == 'missing': bad = [m for m in bad if m['modelID'] != 'deepseek-v4.1-flash']
            with self.subTest(change=change), self.assertRaises(ValueError):
                policy.validate_models(config, bad)

    def test_task_classification(self):
        def classify(files, **kw):
            args = dict(complexity='simple', risk='trivial', ambiguity='low', test_coverage='adequate')
            args.update(kw)
            return policy.classify_task(self.config, files, **args)['class']
        self.assertEqual(classify(['src/foo/a.rs']), 'S')
        self.assertEqual(classify(['src/foo.rs', 'src/bar.rs']), 'M')
        self.assertEqual(classify(['docs/a.md'], ambiguity='unknown'), 'M')
        self.assertEqual(classify(['docs/a.md'], test_coverage='unknown'), 'M')
        for files, args in [(['src/a.rs', 'src/b.rs', 'src/c.rs'], {}), (['src/auth/x.rs'], {}),
                            (['docs/a.md'], {'complexity': 'complex'}), (['docs/a.md'], {'risk': 'high'}),
                            (['docs/a.md'], {'ambiguity': 'high'}), (['docs/a.md'], {'test_coverage': 'poor'}),
                            (['docs/a.md'], {'failed_attempts': 1}), (['docs/a.md'], {'blocking_findings': 1})]:
            with self.subTest(files=files, args=args): self.assertEqual(classify(files, **args), 'L')
        for files, args in [([], {}), (['../bad'], {}), (['x'], {'risk': 'bad'}),
                            (['x'], {'complexity': 'bad'}), (['x'], {'ambiguity': 'bad'}),
                            (['x'], {'test_coverage': 'bad'}), (['x'], {'failed_attempts': True}),
                            (['x'], {'blocking_findings': -1})]:
            with self.subTest(files=files, args=args), self.assertRaises(ValueError): classify(files, **args)

    def test_mode_schema_rejects_invalid_unselected_routes(self):
        raw = json.loads((ROOT / '.opencode/agent-workflow.json').read_text())
        for mutate in (lambda c: c['modes']['opencode-go'].pop('recovery'),
                       lambda c: c['modes']['opencode-go']['recovery'].update(model='openai/x'),
                       lambda c: c['modes']['opencode-go']['recovery'].update(effort='bad effort'),
                       lambda c: c['workflow'].update(mode='unknown')):
            value = copy.deepcopy(raw)
            mutate(value)
            with tempfile.TemporaryDirectory() as d:
                path = Path(d) / 'config.json'
                path.write_text(json.dumps(value))
                with self.assertRaises(ValueError): policy.load_config(path)

    def test_catalog_rejects_explicit_inactive_status(self):
        models = [{'providerID': 'openai', 'modelID': r['model'].split('/')[1], 'enabled': True,
                   'status': 'deprecated', 'type': 'chat', 'capabilities': {'tools': True},
                   'variants': [{'id': r['effort']}]} for r in self.config['mode_roles'].values()]
        with self.assertRaises(ValueError): policy.validate_models(self.config, models)
        for model in models: model.pop('status')
        with self.assertRaises(ValueError): policy.validate_models(self.config, models)

    def test_config_rejects_unknown_boolean_integer_and_nan(self):
        for mutate in (lambda c: c.update(extra=1),
                       lambda c: c['limits'].update(max_attempts=True),
                       lambda c: c['prices'].update({'openai/x': {'input': float('nan'), 'output': 1, 'cache_read': 1, 'cache_write': 1}})):
            config = json.loads((ROOT / '.opencode/agent-workflow.json').read_text())
            mutate(config)
            with tempfile.TemporaryDirectory() as d:
                path = Path(d) / 'config.json'
                path.write_text(json.dumps(config))
                with self.assertRaises(ValueError):
                    policy.load_config(path)

    def test_config_rejects_unrepresentable_prices(self):
        config = json.loads((ROOT / '.opencode/agent-workflow.json').read_text())
        config['prices']['openai/x'] = {'input': 10 ** 400, 'output': 1, 'cache_read': 1, 'cache_write': 1}
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'config.json'
            path.write_text(json.dumps(config))
            with self.assertRaises(ValueError):
                policy.load_config(path)

    def test_catalog_requires_exact_enabled_openai_tool_chat_variant(self):
        models = []
        for role in self.config['mode_roles'].values():
            models.append({'providerID': 'openai', 'modelID': role['model'].split('/', 1)[1],
                           'enabled': True, 'status': 'active', 'type': 'chat', 'capabilities': {'tools': True},
                           'variants': [{'id': role['effort']}], 'cost': []})
        refs = policy.validate_models(self.config, models)
        self.assertEqual(refs['implementer'], {'id': 'gpt-6-luna', 'providerID': 'openai', 'variant': 'medium'})
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

    def test_f2_architecture_and_nested_agent_instructions_force_high_review(self):
        for path in ('docs/adr/0002-single-simulation-clock.md', 'docs/AGENTS.md', 'CLAUDE.md',
                     'nested/CLAUDE.md', 'AGENTS.override.md', 'docs/GEMINI.md', '.agents/rules.md',
                     '.claude/commands/task.md', 'docs/.opencode/agents/review.md', 'docs/.agents/rules.md',
                     'docs/.claude/rules.md', 'docs/CODEBASE.md', 'GLOSSARY.md'):
            with self.subTest(path=path):
                review = policy.review_policy(self.config, [path], 1, 1, 'trivial', True)
                self.assertEqual((review['required'], review['risk'], review['role']), (True, 'high', 'escalation'))

    def test_f2_docs_executable_and_config_are_not_safe_prose(self):
        for path in ('docs/setup.py', 'docs/install.sh', 'docs/build.json', 'docs/site.yml'):
            with self.subTest(path=path):
                self.assertTrue(policy.review_policy(self.config, [path], 1, 1, 'trivial', True)['required'])
        self.assertFalse(policy.review_policy(self.config, ['docs/guide/usage.md'], 1, 1, 'trivial', True)['required'])
        self.assertFalse(policy.review_policy(self.config, ['README.md'], 1, 1, 'trivial', True)['required'])

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
