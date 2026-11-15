"""Real linked Git worktrees; fake only the spending OpenCode boundary."""
import copy
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from scripts.workflow import api, policy, runner

ROOT = Path(__file__).resolve().parents[2]


def git(root, *argv):
    return subprocess.check_output(['git', '-C', str(root), *argv], stderr=subprocess.STDOUT).decode().strip()


class Service:
    def __init__(self, root, config, edits=None, reviews=None, plan=None):
        self.root, self.config = root, config
        self.sessions, self.turns, self.switches = {}, [], []
        self.edits = edits or [lambda: (root / 'docs/a.md').write_text('new\n')]
        self.reviews = reviews or ['{"decision":"approve","findings":[]}']
        self.plan = plan

    def models(self):
        return [{'providerID': r['model'].split('/')[0], 'modelID': r['model'].split('/')[1], 'enabled': True,
                 'status': 'active', 'type': 'chat', 'capabilities': {'tools': True}, 'variants': [{'id': r['effort']}], 'cost': []}
                for r in self.config['mode_roles'].values()]

    def create(self, title, ref, files, read_files=None):
        sid = 'ses_' + str(len(self.sessions))
        self.sessions[sid] = (title, ref, files)
        return sid

    def switch(self, sid, ref):
        self.switches.append((sid, ref))
        title, _, files = self.sessions[sid]
        self.sessions[sid] = (title, ref, files)

    def turn(self, sid, text, event):
        title, ref, files = self.sessions[sid]
        self.turns.append((sid, text))
        mid = 'm' + str(len(self.turns))
        event({'kind': 'admitted', 'user_id': 'u' + mid})
        event({'kind': 'assistant', 'message': {'id': mid, 'type': 'assistant', 'model': ref,
              'tokens': {'input': 10, 'output': 2, 'reasoning': 1, 'cache': {'read': 5, 'write': 0}}, 'cost': 0.01}})
        if title.startswith('plan:'):
            return json.dumps(self.plan)
        if title.startswith('review:'):
            answer = self.reviews.pop(0)
            if callable(answer):
                return answer()
            return answer
        edit = self.edits.pop(0)
        edit()
        return 'implemented'


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.main = Path(self.temp.name) / 'main'
        self.main.mkdir()
        git(self.main, 'init', '-b', 'main')
        git(self.main, 'config', 'user.email', 'test@example.test')
        git(self.main, 'config', 'user.name', 'Offline Test')
        (self.main / 'docs').mkdir()
        (self.main / 'docs/a.md').write_text('old\n')
        (self.main / 'code.py').write_text('old = 1\n')
        git(self.main, 'add', '.')
        git(self.main, 'commit', '-m', 'base')
        self.root = Path(self.temp.name) / 'work'
        git(self.main, 'worktree', 'add', '-b', 'feature/test', str(self.root))
        self.head = git(self.root, 'rev-parse', 'HEAD')
        self.config = policy.load_config(ROOT / '.opencode/agent-workflow.json')
        self.request = {'id': 'run-one', 'requirements': ['Change the text'], 'acceptance_criteria': ['Text is new'],
                        'allowed_files': ['docs/a.md'], 'context': [{'file': 'docs/a.md', 'start': 1, 'end': 1}],
                        'complexity': 'simple', 'risk': 'trivial', 'validation': [[sys.executable, '-c', 'pass']],
                        'expected_head': self.head, 'expected_branch': 'feature/test'}

    def fresh_worktree(self, label):
        self.root = Path(self.temp.name) / label
        branch = 'feature/' + label
        git(self.main, 'worktree', 'add', '-b', branch, str(self.root), self.head)
        self.request.update(expected_branch=branch, expected_head=self.head)

    def run_work(self, service=None):
        return runner.run(self.root, self.request, self.config, service or Service(self.root, self.config))

    def state(self):
        return json.loads((Path(git(self.root, 'rev-parse', '--absolute-git-dir')) / 'agent-workflow/runs/run-one.json').read_text())

    def test_simple_trivial_skips_planner_and_reviewer_acceptance_is_not_merge(self):
        service = Service(self.root, self.config)
        result = self.run_work(service)
        self.assertEqual(result['status'], 'accepted')
        self.assertEqual(len(service.sessions), 1)
        self.assertEqual(result['tasks'][0]['reviews'], 0)
        self.assertEqual(result['tasks'][0]['review_policy']['reason'], 'validated trivial diff')
        self.assertEqual(result['tasks'][0]['status'], 'accepted')
        self.assertEqual(result['records'][0]['usage']['cache_read'], 5)
        self.assertEqual(git(self.root, 'rev-parse', 'HEAD'), self.head)
        self.assertEqual(self.state()['accepted_tree'], result['accepted_tree'])
        with self.assertRaises(ValueError):
            self.run_work(service)

    def test_task_class_routes_sessions_and_records_resolution(self):
        self.request['risk'] = 'normal'
        service = Service(self.root, self.config)
        result = self.run_work(service)
        task = result['tasks'][0]
        self.assertEqual(task['classification']['class'], 'M')
        self.assertEqual(task['route']['role'], 'implementer_standard')
        self.assertEqual(task['route']['model'], self.config['mode_roles']['implementer_simple']['model'])
        self.assertEqual(result['selected_mode'], 'openai')
        self.assertEqual(task['duration_seconds'] >= 0, True)
        self.assertTrue(all(call['model'] for call in result['calls']))

    def test_normal_and_high_risk_routing_use_independent_standard_and_complex_reviewers(self):
        for risk, role in (('normal', 'reviewer_standard'), ('high', 'reviewer_complex')):
            self.fresh_worktree('route-' + risk)
            self.request.update(id='review-' + risk, risk=risk, allowed_files=['new.py'])
            result = self.run_work(Service(self.root, self.config,
                [lambda: (self.root / 'new.py').write_text('value = 2\n')]))
            review = next(s for s in result['calls'] if s['role'] == role)
            self.assertEqual(review['model'], policy.validate_models(self.config, Service(self.root, self.config).models())[role])
            (self.root / 'new.py').unlink()

    def test_override_rejected_before_session_and_recovery_and_review_roles_are_explicit(self):
        self.request['task_overrides'] = {'run-one': {'role': 'implementer_simple', 'model': 'openai/nope',
            'effort': 'low', 'reason': 'requested', 'catalog_identity': 'stale'}}
        service = Service(self.root, self.config)
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(service.sessions, {})

    def test_recovery_role_is_selected_for_declared_complex_task(self):
        self.request['complexity'] = 'complex'
        task = {'id': 'one', 'requirements': ['change'], 'acceptance_criteria': ['new'],
                'allowed_files': ['docs/a.md'], 'depends_on': []}
        service = Service(self.root, self.config, plan=[task])
        result = self.run_work(service)
        self.assertEqual(result['tasks'][0]['route']['role'], 'recovery')
        implementation = next(value for value in service.sessions.values() if value[0].startswith('implement:'))
        self.assertEqual(implementation[1], policy.validate_models(self.config, service.models())['recovery'])

    def test_run_records_usage_coverage_cost_basis_and_mode(self):
        result = self.run_work()
        record = result['records'][0]
        self.assertEqual(record['cost_source'], 'reported')
        self.assertIn('allowance', record)
        self.assertIsNone(record['allowance'])

    def test_arbitrary_catalog_override_reaches_session_and_is_audited(self):
        catalog = Service(self.root, self.config).models()
        catalog.append({'providerID': 'openai', 'modelID': 'gpt-6.1-terra', 'enabled': True,
                        'status': 'active', 'type': 'chat', 'capabilities': {'tools': True},
                        'variants': [{'id': 'xhigh'}], 'name': 'Terra', 'cost': []})
        self.config['mode_roles']['implementer_simple'] = {'model': 'openai/gpt-6-luna', 'effort': 'low'}
        override = {'role': 'implementer_simple', 'model': 'openai/gpt-6.1-terra', 'effort': 'xhigh',
                    'reason': 'explicit test override', 'catalog_identity': policy.catalog_identity(catalog)}
        self.request['task_overrides'] = {'run-one': override}
        service = Service(self.root, self.config)
        service.models = lambda: catalog
        result = self.run_work(service)
        self.assertEqual(next(iter(service.sessions.values()))[1],
                         {'providerID': 'openai', 'id': 'gpt-6.1-terra', 'variant': 'xhigh'})
        route = result['tasks'][0]['route']
        self.assertEqual((route['model'], route['effort'], route['reason']),
                         ('openai/gpt-6.1-terra', 'xhigh', 'explicit test override'))

    def test_override_missing_or_stale_catalog_identity_fails_before_session(self):
        catalog = Service(self.root, self.config).models()
        override = {'role': 'implementer_simple', 'model': 'openai/gpt-6-luna', 'effort': 'low',
                    'reason': 'explicit', 'catalog_identity': 'stale'}
        self.request['task_overrides'] = {'run-one': override}
        service = Service(self.root, self.config)
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(service.sessions, {})
        self.request['id'] = 'missing-identity'
        self.request.pop('task_overrides', None)
        self.request['task_overrides'] = {'missing-identity': {'role': 'implementer_simple', 'model': 'openai/gpt-6-luna', 'effort': 'low', 'reason': 'explicit'}}
        service = Service(self.root, self.config)
        with self.assertRaisesRegex(ValueError, 'catalog_identity'):
            self.run_work(service)
        self.assertEqual(service.sessions, {})

    def test_override_call_and_recovery_records_use_actual_invocation_refs(self):
        catalog = Service(self.root, self.config).models()
        catalog.append({'providerID': 'openai', 'modelID': 'gpt-6.1-terra', 'enabled': True,
                        'status': 'active', 'type': 'chat', 'capabilities': {'tools': True},
                        'variants': [{'id': 'xhigh'}], 'cost': []})
        self.config['limits']['escalate_after'] = 1
        self.config['limits']['max_attempts'] = 2
        override = {'role': 'implementer_simple', 'model': 'openai/gpt-6.1-terra', 'effort': 'xhigh',
                    'reason': 'test', 'catalog_identity': policy.catalog_identity(catalog)}
        self.request['task_overrides'] = {'run-one': override}
        self.request['validation'] = [[sys.executable, '-c', 'raise SystemExit(1)']]
        self.config['output_includes_reasoning'] = True
        self.config['prices']['openai/gpt-6-luna'] = {'input': 2, 'output': 3, 'cache_read': 1, 'cache_write': 1}
        class Recovering(Service):
            def models(self): return catalog
            def turn(inner, sid, text, event):
                inner.turns.append((sid, text))
                model = inner.sessions[sid][1]
                event({'kind': 'assistant', 'message': {'id': 'm' + str(len(inner.turns)), 'model': model,
                      'tokens': {'input': 1, 'output': 1, 'reasoning': 0, 'cache': {'read': 0, 'write': 0}}, 'cost': 0.01}})
                inner.root.joinpath('docs/a.md').write_text('new\n')
                return 'implemented'
            def switch(inner, sid, ref):
                super().switch(sid, ref)
        result = self.run_work(Recovering(self.root, self.config))
        self.assertEqual(result['calls'][-1]['invocation_model'], policy.validate_models(self.config, catalog)['recovery'])

    def test_failed_run_keeps_spend_but_not_accepted_outcome_count(self):
        self.request['id'] = 'failed-spend'
        self.config['limits']['max_attempts'] = 1
        self.config['output_includes_reasoning'] = True
        self.config['prices']['openai/gpt-6-luna'] = {'input': 2, 'output': 3, 'cache_read': 1, 'cache_write': 1}
        self.request['validation'] = [[sys.executable, '-c', 'raise SystemExit(2)']]
        result = self.run_work(Service(self.root, self.config))
        self.assertEqual(result['status'], 'failed')
        report = runner.report([runner.Git(self.root).directory() / 'runs/failed-spend.json'])
        self.assertGreater(report['coverage']['cost_known'], 0)
        self.assertEqual(report['accepted'], 0)
        self.assertNotIn('cost_per_accepted', report)

    def test_report_uses_verified_run_and_merge_status_without_unit_efficiency(self):
        result = self.run_work()
        run_file = runner.Git(self.root).directory() / 'runs/run-one.json'
        report = runner.report([run_file])
        self.assertEqual((report['accepted'], report['merged']), (1, 0))
        self.assertEqual(report['runs'][0]['tasks'][0]['status'], 'accepted')
        self.assertNotIn('cost_per_accepted', report)
        git(self.root, 'add', '.')
        git(self.root, 'commit', '-m', 'accepted task')
        commit = git(self.root, 'rev-parse', 'HEAD')
        git(self.main, 'merge', '--ff-only', commit)
        runner.record_merge(self.root, 'run-one', commit, 'main')
        report = runner.report([run_file])
        self.assertEqual((report['accepted'], report['merged']), (1, 1))

    def test_normal_and_high_review_actual_new_file_diff_independent_session(self):
        for risk, effort in [('normal', 'medium'), ('high', 'high')]:
            with self.subTest(risk=risk):
                self.request['id'] = 'run-' + risk
                self.request['risk'] = risk
                self.request['allowed_files'] = ['new.py']
                service = Service(self.root, self.config, [lambda: (self.root / 'new.py').write_text('value = 3\n')])
                result = self.run_work(service)
                review_sessions = [(sid, s) for sid, s in service.sessions.items() if s[0].startswith('review:')]
                self.assertEqual(len(review_sessions), 1)
                sid, session = review_sessions[0]
                self.assertEqual(session[1]['variant'], effort)
                self.assertEqual(session[2], [])
                prompt = dict(service.turns)[sid]
                self.assertIn('+value = 3', prompt)
                self.assertIn('Text is new', prompt)
                self.assertNotIn('implemented', prompt)
                self.assertEqual(result['tasks'][0]['reviews'], 1)
                (self.root / 'new.py').unlink()

    def test_validation_retry_reuses_session_and_escalates_without_context_repetition(self):
        self.config['limits']['escalate_after'] = 1
        self.request['validation'] = [[sys.executable, '-c', 'from pathlib import Path; assert Path("docs/a.md").read_text() == "good\\n"']]
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('bad\n'),
                                                  lambda: (self.root / 'docs/a.md').write_text('good\n')])
        result = self.run_work(service)
        self.assertEqual(result['status'], 'accepted', result['error'])
        self.assertEqual(result['tasks'][0]['attempts'], 2)
        self.assertEqual(len(service.sessions), 1)
        self.assertEqual(service.turns[0][0], service.turns[1][0])
        self.assertIn('CONTEXT', service.turns[0][1])
        self.assertNotIn('CONTEXT', service.turns[1][1])
        self.assertIn('validation', service.turns[1][1])
        self.assertEqual(service.switches[0][1]['variant'], 'high')
        self.assertEqual(len(result['records']), 2)

    def test_reviews_are_bounded_malformed_block_and_outside_findings_fail_closed(self):
        cases = ['not json', '{"decision":"block","findings":[]}', json.dumps({'decision': 'request_changes', 'findings': [
            {'file': 'code.py', 'line': 1, 'severity': 'important', 'message': 'outside'}]})]
        self.request['risk'] = 'normal'
        for i, review in enumerate(cases):
            self.fresh_worktree('review-' + str(i))
            self.request['id'] = 'bad-' + str(i)
            result = self.run_work(Service(self.root, self.config, reviews=[review]))
            self.assertIn(result['status'], ('failed', 'blocked'))
            self.assertEqual(result['tasks'][0]['reviews'], 1)
            self.assertEqual(result['tasks'][0]['attempts'], 1)
            (self.root / 'docs/a.md').write_text('old\n')
        self.request['id'] = 'cap'
        self.fresh_worktree('cap-work')
        self.config['limits']['max_reviews'] = 1
        review = json.dumps({'decision': 'request_changes', 'findings': [{'file': 'docs/a.md', 'line': 1, 'severity': 'important', 'message': 'fix text'}]})
        result = self.run_work(Service(self.root, self.config, reviews=[review]))
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['tasks'][0]['rejections'], 1)

    def test_attempt_cap_empty_success_ownership_and_unexpected_commit_preserve_evidence(self):
        for kind in ('empty', 'outside', 'commit'):
            self.fresh_worktree('attempt-' + kind)
            self.request['id'] = kind
            if kind == 'empty':
                self.config['limits']['max_attempts'] = 1
                edit = lambda: None
            elif kind == 'outside':
                edit = lambda: (self.root / 'extra.py').write_text('unauthorized\n')
            else:
                def edit():
                    (self.root / 'docs/a.md').write_text('changed\n')
                    git(self.root, 'add', '.')
                    git(self.root, 'commit', '-m', 'unexpected')
            result = self.run_work(Service(self.root, self.config, [edit]))
            self.assertEqual(result['status'], 'failed')
            self.assertTrue(result['error'])
            self.assertEqual(result['tasks'][0]['attempts'], 1)
            if kind == 'outside':
                self.assertTrue((self.root / 'extra.py').exists())
                (self.root / 'extra.py').unlink()

    def test_schema_traversal_symlink_context_and_dirty_main_identity_rejected(self):
        for update in ({'allowed_files': ['../x']}, {'allowed_files': ['.git/config']}, {'allowed_files': ['a*']},
                       {'acceptance_criteria': []}, {'validation': []}, {'surprise': True}, {'complexity': True}):
            request = dict(self.request, **update)
            with self.subTest(update=update), self.assertRaises(ValueError):
                runner.run(self.root, request, self.config, Service(self.root, self.config))
        (self.root / 'escape').symlink_to(self.main, target_is_directory=True)
        with self.assertRaises(ValueError):
            runner.validate_manifest(dict(self.request, allowed_files=['escape/code.py']), self.root, self.config)
        (self.root / 'escape').unlink()
        self.config['limits']['max_context_bytes'] = 2
        with self.assertRaises(ValueError):
            self.run_work()
        self.config['limits']['max_context_bytes'] = 10000
        (self.root / 'dirty').write_text('dirty')
        with self.assertRaises(ValueError):
            self.run_work()
        (self.root / 'dirty').unlink()
        with self.assertRaises(ValueError):
            runner.run(self.main, dict(self.request, expected_branch='main'), self.config, Service(self.main, self.config))
        self.request['expected_head'] = '0' * 40
        with self.assertRaises(ValueError):
            self.run_work()

    def test_exclusive_lock_is_not_removed_or_stolen(self):
        directory = Path(git(self.root, 'rev-parse', '--absolute-git-dir')) / 'agent-workflow'
        directory.mkdir()
        lock = directory / 'lock'
        lock.write_text('owner evidence')
        with self.assertRaises(ValueError):
            self.run_work()
        self.assertEqual(lock.read_text(), 'owner evidence')

    def test_complex_planner_rejects_cycles_duplicate_ids_and_injected_validation(self):
        self.request['complexity'] = 'complex'
        task = {'id': 'one', 'requirements': ['do text'], 'acceptance_criteria': ['new text'], 'allowed_files': ['docs/a.md'], 'depends_on': []}
        for index, plan in enumerate(([task, task], [dict(task, depends_on=['one'])], [dict(task, validation=[['bad']])], [dict(task, allowed_files=['code.py'])])):
            self.fresh_worktree('planner-' + str(index))
            self.request['id'] = 'plan-' + str(len(list((Path(git(self.root, 'rev-parse', '--absolute-git-dir'))).glob('agent-workflow/runs/*.json'))))
            service = Service(self.root, self.config, plan=plan)
            result = self.run_work(service)
            self.assertEqual(result['status'], 'failed')
            self.assertEqual(len(service.sessions), 1)

    def test_complex_tasks_incremental_diffs_and_merge_proof(self):
        self.request.update(complexity='complex', risk='normal', allowed_files=['docs/a.md', 'code.py'])
        task = {'id': 'one', 'requirements': ['do text'], 'acceptance_criteria': ['new text'], 'allowed_files': ['docs/a.md'], 'depends_on': []}
        plan = [task, dict(task, id='two', allowed_files=['code.py'], depends_on=['one'])]
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('new\n'),
                                                  lambda: (self.root / 'code.py').write_text('value = 2\n')],
                          ['{"decision":"approve","findings":[]}'] * 2, plan)
        result = self.run_work(service)
        self.assertEqual(result['status'], 'accepted')
        reviews = [text for sid, text in service.turns if service.sessions[sid][0].startswith('review:')]
        self.assertNotIn('diff --git a/docs/a.md', reviews[1])
        with self.assertRaises(ValueError):
            runner.record_merge(self.root, 'run-one', self.head, 'main')
        git(self.root, 'add', '.')
        git(self.root, 'commit', '-m', 'accepted')
        commit = git(self.root, 'rev-parse', 'HEAD')
        with self.assertRaises(ValueError):
            runner.record_merge(self.root, 'run-one', commit, 'main')
        git(self.main, 'merge', '--ff-only', commit)
        merged = runner.record_merge(self.root, 'run-one', commit, 'main')
        self.assertEqual([t['status'] for t in merged['tasks']], ['merged', 'merged'])
        self.assertEqual(merged['tasks'][0]['merged_lines'], 0)
        self.assertEqual(merged['tasks'][1]['merged_lines'], 1)
        self.assertEqual(merged['merge']['commit'], commit)

    def test_f1_decomposition_defers_whole_request_validation_until_both_edits(self):
        self.config['limits']['max_attempts'] = 1
        self.request.update(complexity='complex', allowed_files=['docs/a.md', 'code.py'],
                            acceptance_criteria=['Both requested files have their final contents'])
        self.request['validation'] = [[sys.executable, '-c', 'from pathlib import Path; '
            'assert Path("docs/a.md").read_text() == "new\\n"; assert Path("code.py").read_text() == "value = 2\\n"']]
        task = {'id': 'change-doc', 'requirements': ['Change the text'], 'acceptance_criteria': ['Text is new'],
                'allowed_files': ['docs/a.md'], 'depends_on': []}
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('new\n'),
                                                  lambda: (self.root / 'code.py').write_text('value = 2\n')],
                          ['{"decision":"approve","findings":[]}'] * 2,
                          [task, dict(task, id='change-code', allowed_files=['code.py'], depends_on=['change-doc'])])
        result = self.run_work(service)
        self.assertEqual(result['status'], 'accepted', result['error'])
        self.assertEqual([t['attempts'] for t in result['tasks']], [1, 1])
        self.assertEqual([t['validations'] for t in result['tasks']], [[], []])
        self.assertEqual([t['reviews'] for t in result['tasks']], [1, 1])
        self.assertTrue(all(t['review_policy']['required'] for t in result['tasks']))
        self.assertEqual(result['final_validation_status'], 'passed')
        self.assertEqual([v['exit'] for v in result['final_validations']], [0])
        self.assertEqual(result['final_validations'][0]['argv'], self.request['validation'][0])
        prompts = [text for sid, text in service.turns if service.sessions[sid][0].startswith('review:')]
        self.assertTrue(all('"final_acceptance": "pending"' in prompt for prompt in prompts))
        self.assertTrue(all('"deterministic_validation": "unavailable"' in prompt for prompt in prompts))

    def test_f1_final_global_failure_preserves_provisional_reviews_and_edits(self):
        self.config['limits']['max_attempts'] = 1
        self.request.update(complexity='complex', risk='normal', allowed_files=['docs/a.md', 'code.py'])
        self.request['validation'] = [[sys.executable, '-c', 'raise SystemExit(7)']]
        task = {'id': 'one', 'requirements': ['text'], 'acceptance_criteria': ['new'],
                'allowed_files': ['docs/a.md'], 'depends_on': []}
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('new\n'),
                                                  lambda: (self.root / 'code.py').write_text('value = 2\n')],
                          ['{"decision":"approve","findings":[]}'] * 2,
                          [task, dict(task, id='two', allowed_files=['code.py'], depends_on=['one'])])
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(len(result['tasks']), 2)
        self.assertEqual([t['status'] for t in result['tasks']], ['provisional', 'provisional'])
        self.assertEqual(result['final_validation_status'], 'failed')
        self.assertEqual([v['exit'] for v in result['final_validations']], [7])
        self.assertEqual([t['review_results'][0]['decision'] for t in result['tasks']], ['approve', 'approve'])
        self.assertEqual((self.root / 'code.py').read_text(), 'value = 2\n')
        self.assertEqual(len(result['records']), 5)
        self.assertNotIn('accepted_tree', result)

    def test_f1_trusted_local_checks_retry_in_same_session_then_final_gate(self):
        self.request.update(complexity='complex', risk='normal', allowed_files=['docs/a.md', 'code.py'])
        local = [sys.executable, '-c', 'from pathlib import Path; assert Path("docs/a.md").read_text() == "good\\n"']
        self.request['task_validation'] = {'one': [local]}
        self.request['validation'] = [[sys.executable, '-c', 'from pathlib import Path; '
            'assert Path("docs/a.md").read_text() == "good\\n"; assert Path("code.py").read_text() == "value = 2\\n"']]
        task = {'id': 'one', 'requirements': ['text'], 'acceptance_criteria': ['good'],
                'allowed_files': ['docs/a.md'], 'depends_on': []}
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('bad\n'),
                                                  lambda: (self.root / 'docs/a.md').write_text('good\n'),
                                                  lambda: (self.root / 'code.py').write_text('value = 2\n')],
                          ['{"decision":"approve","findings":[]}'] * 2,
                          [task, dict(task, id='two', allowed_files=['code.py'], depends_on=['one'])])
        result = self.run_work(service)
        self.assertEqual(result['status'], 'accepted', result['error'])
        self.assertEqual([v['exit'] for v in result['tasks'][0]['validations']], [1, 0])
        planner_prompt = next(text for sid, text in service.turns if service.sessions[sid][0].startswith('plan:'))
        self.assertIn('"task_validation_ids": ["one"]', planner_prompt)
        self.assertEqual([t['attempts'] for t in result['tasks']], [2, 1])
        rounds = [(sid, text) for sid, text in service.turns if service.sessions[sid][0] == 'implement:one']
        self.assertEqual(rounds[0][0], rounds[1][0])
        self.assertIn('validation', rounds[1][1])
        self.assertNotIn('CONTEXT', rounds[1][1])
        self.assertEqual(result['final_validation_status'], 'passed')

    def test_f1_local_validation_schema_and_unknown_task_ids_fail_closed(self):
        for mapping in ([], {'one': []}, {'one': ['shell string']}, {'one': [[True]]}):
            with self.subTest(mapping=mapping), self.assertRaises(ValueError):
                runner.validate_manifest(dict(self.request, task_validation=mapping), self.root, self.config)
        self.request['task_validation'] = {'unknown': [[sys.executable, '-c', 'pass']]}
        service = Service(self.root, self.config)
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertIn('unknown task', result['error'])
        self.assertEqual(len(service.sessions), 0)

    def test_api_security_failure_keeps_usage_and_unknown_coverage_no_hopping(self):
        service = Service(self.root, self.config)
        def failed(sid, text, event):
            event({'kind': 'assistant', 'message': {'id': 'failed', 'model': {'id': 'gpt-6-luna', 'providerID': 'openai', 'variant': 'medium'}, 'cost': 0.02, 'error': {'type': 'QuotaError'}}})
            raise api.APIError('quota', retryable=False)
        service.turn = failed
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['tasks'][0]['attempts'], 1)
        self.assertEqual(result['records'][0]['cost'], 0.02)
        self.assertIsNone(result['records'][0]['usage']['input'])
        self.assertEqual(len(service.sessions), 1)

    def test_failed_run_cannot_be_restarted_by_changing_id(self):
        self.config['limits']['max_attempts'] = 1
        result = self.run_work(Service(self.root, self.config, [lambda: None]))
        self.assertEqual(result['status'], 'failed')
        self.request['id'] = 'new-id'
        with self.assertRaisesRegex(ValueError, 'new.*worktree'):
            self.run_work()

    def test_planner_cannot_edit_before_task_snapshots(self):
        self.request['complexity'] = 'complex'
        service = Service(self.root, self.config)
        def planner(sid, text, event):
            (self.root / 'code.py').write_text('unauthorized = True\n')
            return json.dumps([{'id': 'one', 'requirements': ['do'], 'acceptance_criteria': ['new'],
                                'allowed_files': ['docs/a.md'], 'depends_on': []}])
        service.turn = planner
        self.assertEqual(self.run_work(service)['status'], 'failed')
        self.assertEqual(len(service.sessions), 1)

    def test_validation_cannot_mutate_and_binary_deleted_renamed_diffs_are_real(self):
        self.request['allowed_files'] = ['code.py', 'new.py', 'docs/a.md']
        self.request['risk'] = 'trivial'
        def edit():
            (self.root / 'code.py').rename(self.root / 'new.py')
            (self.root / 'docs/a.md').write_bytes(b'\x00\x01binary')
        result = self.run_work(Service(self.root, self.config, [edit]))
        self.assertEqual(result['status'], 'accepted')
        self.assertEqual(set(result['tasks'][0]['diff']['files']), {'code.py', 'new.py', 'docs/a.md'})
        self.assertTrue(result['tasks'][0]['diff']['binary'])
        self.assertEqual(result['tasks'][0]['reviews'], 1)
        self.fresh_worktree('validation-mutation')
        self.request['validation'] = [[sys.executable, '-c', 'from pathlib import Path; Path("code.py").write_text("bad")']]
        result = self.run_work()
        self.assertEqual(result['status'], 'failed')
        self.assertIn('validation modified', result['error'])

    def test_changed_commit_patch_or_non_ref_target_cannot_fake_merge(self):
        self.run_work()
        (self.root / 'docs/a.md').write_text('different\n')
        git(self.root, 'add', '.')
        git(self.root, 'commit', '-m', 'not accepted')
        commit = git(self.root, 'rev-parse', 'HEAD')
        git(self.main, 'merge', '--ff-only', commit)
        with self.assertRaisesRegex(ValueError, 'accepted'):
            runner.record_merge(self.root, 'run-one', commit, 'main')
        with self.assertRaises(ValueError):
            runner.record_merge(self.root, 'run-one', commit, commit)

    def test_cli_offline_run_merge_and_metrics_through_fake_executable(self):
        # The executable replaces ONLY the external OpenCode service. All CLI,
        # adapter, permissions, orchestration, Git and accounting code is real.
        bindir = Path(self.temp.name) / 'bin'
        bindir.mkdir()
        # Invoke the checked-in runner from INSIDE its target worktree; imports
        # must not generate unowned __pycache__ files before preflight.
        (self.root / 'scripts/workflow').mkdir(parents=True)
        for source in (ROOT / 'scripts/workflow').glob('*.py'):
            shutil.copyfile(source, self.root / 'scripts/workflow' / source.name)
        shutil.copyfile(ROOT / 'scripts/agent_workflow.py', self.root / 'scripts/agent_workflow.py')
        git(self.root, 'add', '.')
        git(self.root, 'commit', '-m', 'workflow fixture')
        self.request['expected_head'] = git(self.root, 'rev-parse', 'HEAD')
        catalog = Service(self.root, self.config).models()
        executable = bindir / 'opencode'
        executable.write_text('#!' + sys.executable + '\n' + '''
import json, os, pathlib, sys
args=sys.argv[1:]
method,path=args[1:3]
body=json.loads(args[args.index('--data')+1]) if '--data' in args else None
store=pathlib.Path(os.environ['FAKE_STORE'])
if path == '/api/model': result={'location': {}, 'data': json.loads(os.environ['FAKE_CATALOG'])}
elif path == '/api/session':
 result={'data': dict(body,id='ses_offline')}; store.write_text(json.dumps(result['data']))
elif path.endswith('/prompt'):
 session=json.loads(store.read_text()); pathlib.Path(session['location']['directory'],'docs/a.md').write_text('new\\n')
 result={'data': {'id':'u1','type':'user','sessionID':'ses_offline','time':{'created':1}}}
elif path.endswith('/wait'): sys.exit(0)
elif path.endswith('/message'):
 session=json.loads(store.read_text())
 result={'data':[{'id':'u1','type':'user'},
 {'id':'a1','type':'assistant','agent':'build','model':session['model'],'time':{'created':2,'completed':3},
 'finish':'stop','content':[{'type':'text','text':'done'}], 'tokens':{'input':10,'output':2,'reasoning':1,'cache':{'read':5,'write':0}},'cost':0.01},
 {'id':'i1','type':'idle','outcome':'succeeded','time':{'created':4}}], 'cursor':{'next':None,'previous':None}}
else: raise RuntimeError(path)
print(json.dumps(result))
''')
        executable.chmod(0o755)
        manifest = Path(self.temp.name) / 'request.json'
        manifest.write_text(json.dumps(self.request))
        environment = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ['PATH'],
                           FAKE_STORE=str(Path(self.temp.name) / 'service.json'), FAKE_CATALOG=json.dumps(catalog))
        def cli(*args):
            process = subprocess.run([sys.executable, str(self.root / 'scripts/agent_workflow.py'), '--root', str(self.root),
                                      '--config', str(ROOT / '.opencode/agent-workflow.json'), *args],
                                     env=environment, text=True, capture_output=True)
            self.assertEqual(process.returncode, 0, process.stderr + process.stdout)
            return json.loads(process.stdout)
        self.assertEqual(cli('doctor')['status'], 'ok')
        self.assertEqual(cli('run', str(manifest))['status'], 'accepted')
        self.assertEqual(cli('metrics')['merged'], 0)
        git(self.root, 'add', '.')
        git(self.root, 'commit', '-m', 'accepted offline')
        commit = git(self.root, 'rev-parse', 'HEAD')
        git(self.main, 'merge', '--ff-only', commit)
        self.assertEqual(cli('record-merge', 'run-one', '--commit', commit, '--target', 'main')['status'], 'merged')
        self.assertEqual(cli('metrics')['merged'], 1)

    def test_python_validation_imports_do_not_create_unowned_bytecode(self):
        self.request['validation'] = [[sys.executable, '-c', 'import code']]
        result = self.run_work()
        self.assertEqual(result['status'], 'accepted', result['error'])
        self.assertFalse((self.root / '__pycache__').exists())

    def test_metrics_pending_call_cannot_be_counted_as_zero_usage(self):
        result = self.run_work()
        state = self.state()
        state['calls'][0]['status'] = 'pending'
        state['calls'][0]['usage_complete'] = False
        state['records'] = []
        path = Path(self.temp.name) / 'interrupted.json'
        path.write_text(json.dumps(state))
        report = runner.report([path])
        self.assertIsNone(report['totals']['input'])
        self.assertIsNone(report['cost'])

    def test_retry_review_feedback_is_actionable_and_complete_final_usage(self):
        self.request['risk'] = 'normal'
        rejection = json.dumps({'decision': 'request_changes', 'findings': [
            {'file': 'docs/a.md', 'line': 1, 'severity': 'important', 'message': 'Use good text'}]})
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('bad\n'),
                                                  lambda: (self.root / 'docs/a.md').write_text('good\n')],
                          [rejection, '{"decision":"approve","findings":[]}'])
        result = self.run_work(service)
        self.assertEqual(result['status'], 'accepted')
        self.assertEqual((result['tasks'][0]['attempts'], result['tasks'][0]['reviews'], result['tasks'][0]['rejections']), (2, 2, 1))
        implement_prompts = [text for sid, text in service.turns if service.sessions[sid][0].startswith('implement:')]
        self.assertIn('Use good text', implement_prompts[1])
        self.assertNotIn('CONTEXT', implement_prompts[1])
        self.assertEqual(len(result['records']), 4)

    def test_diff_limit_stops_before_review_without_truncation(self):
        self.config['limits']['max_diff_bytes'] = 10
        service = Service(self.root, self.config)
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertIn('output limit', result['error'])
        self.assertEqual(len(service.sessions), 1)

    def test_shared_tasks_count_only_final_merged_source_lines_once(self):
        self.request.update(complexity='complex', risk='normal', allowed_files=['code.py'])
        task = {'id': 'one', 'requirements': ['source'], 'acceptance_criteria': ['value'], 'allowed_files': ['code.py'], 'depends_on': []}
        service = Service(self.root, self.config, [lambda: (self.root / 'code.py').write_text('a = 1\nb = 2\n'),
                                                  lambda: (self.root / 'code.py').write_text('a = 3\n')],
                          ['{"decision":"approve","findings":[]}'] * 2,
                          [task, dict(task, id='two', depends_on=['one'])])
        self.assertEqual(self.run_work(service)['status'], 'accepted')
        git(self.root, 'add', '.')
        git(self.root, 'commit', '-m', 'accepted')
        commit = git(self.root, 'rev-parse', 'HEAD')
        git(self.main, 'merge', '--ff-only', commit)
        merged = runner.record_merge(self.root, 'run-one', commit, 'main')
        self.assertEqual([t['merged_lines'] for t in merged['tasks']], [0, 1])

    def test_subtasks_that_undo_all_work_cannot_claim_final_success(self):
        self.request.update(complexity='complex', risk='normal')
        task = {'id': 'one', 'requirements': ['text'], 'acceptance_criteria': ['new'], 'allowed_files': ['docs/a.md'], 'depends_on': []}
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('new\n'),
                                                  lambda: (self.root / 'docs/a.md').write_text('old\n')],
                          ['{"decision":"approve","findings":[]}'] * 2,
                          [task, dict(task, id='two', depends_on=['one'])])
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertIn('empty final', result['error'])

    def test_f3_failed_empty_diff_counts_no_final_acceptance_in_production_report(self):
        self.request.update(complexity='complex', risk='normal')
        task = {'id': 'one', 'requirements': ['text'], 'acceptance_criteria': ['new'],
                'allowed_files': ['docs/a.md'], 'depends_on': []}
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('new\n'),
                                                  lambda: (self.root / 'docs/a.md').write_text('old\n')],
                          ['{"decision":"approve","findings":[]}'] * 2,
                          [task, dict(task, id='two', depends_on=['one'])])
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        report = runner.report([runner.Git(self.root).directory() / 'runs/run-one.json'])
        self.assertEqual((report['accepted'], report['merged']), (0, 0))
        self.assertEqual(report['first_pass_acceptance_rate'], 0)
        self.assertNotIn('cost_per_accepted', report)
        tasks = report['runs'][0]['tasks']
        self.assertTrue(all(t['status'] not in ('accepted', 'merged') for t in tasks))
        self.assertEqual([t['attempts'] for t in tasks], [1, 1])
        self.assertEqual([t['reviews'] for t in tasks], [1, 1])
        self.assertEqual([t['review_results'][0]['decision'] for t in tasks], ['approve', 'approve'])
        self.assertEqual(len(report['runs'][0]['records']), len(result['records']))
        self.assertTrue(all(record.get('mode') == 'openai' for record in report['runs'][0]['records']))
        self.assertAlmostEqual(sum(r['cost'] for r in result['records']), 0.05)

    def test_f3_legacy_failed_run_with_surviving_edits_cannot_count_historical_acceptance(self):
        self.assertEqual(self.run_work()['status'], 'accepted')
        state = self.state()
        # Historical v1 evidence can have accepted task flags after a final
        # failure. Reporting must reconcile run-level eligibility, not trust them.
        state.update(status='failed', error='final global validation failed')
        path = Path(self.temp.name) / 'legacy-failed.json'
        path.write_text(json.dumps(state))
        report = runner.report([path])
        self.assertEqual((report['accepted'], report['merged']), (0, 0))
        task = report['runs'][0]['tasks'][0]
        self.assertEqual(task['historical_status'], 'accepted')
        self.assertEqual(task['status'], 'failed')
        self.assertEqual(len(report['runs'][0]['records']), len(state['records']))
        self.assertTrue(all(record.get('mode') == 'openai' for record in report['runs'][0]['records']))

    def test_f3_final_global_failure_counts_zero_despite_two_historical_approvals(self):
        self.request.update(complexity='complex', risk='normal', allowed_files=['docs/a.md', 'code.py'])
        self.request['validation'] = [[sys.executable, '-c', 'raise SystemExit(7)']]
        task = {'id': 'one', 'requirements': ['text'], 'acceptance_criteria': ['new'],
                'allowed_files': ['docs/a.md'], 'depends_on': []}
        service = Service(self.root, self.config, [lambda: (self.root / 'docs/a.md').write_text('new\n'),
                                                  lambda: (self.root / 'code.py').write_text('value = 2\n')],
                          ['{"decision":"approve","findings":[]}'] * 2,
                          [task, dict(task, id='two', allowed_files=['code.py'], depends_on=['one'])])
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(len(result['tasks']), 2)
        report = runner.report([runner.Git(self.root).directory() / 'runs/run-one.json'])
        self.assertEqual((report['accepted'], report['merged']), (0, 0))
        tasks = report['runs'][0]['tasks']
        self.assertEqual([t['historical_status'] for t in tasks], ['provisional', 'provisional'])
        self.assertEqual([t['status'] for t in tasks], ['failed', 'failed'])
        self.assertEqual([t['review_results'][0]['decision'] for t in tasks], ['approve', 'approve'])
        self.assertEqual(len(report['runs'][0]['records']), len(result['records']))
        self.assertTrue(all(record.get('mode') == 'openai' for record in report['runs'][0]['records']))
        self.assertAlmostEqual(sum(r['cost'] for r in result['records']), 0.05)

    def test_unexpected_provider_model_or_effort_stops_but_keeps_usage(self):
        service = Service(self.root, self.config)
        def wrong_model(sid, text, event):
            event({'kind': 'assistant', 'message': {'id': 'wrong', 'type': 'assistant',
                  'model': {'id': 'x', 'providerID': 'other', 'variant': 'low'}, 'cost': 0.02}})
            (self.root / 'docs/a.md').write_text('new\n')
            return 'done'
        service.turn = wrong_model
        result = self.run_work(service)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(len(service.sessions), 1)
        self.assertEqual(result['records'][0]['model'], 'other/x')
        self.assertEqual(result['records'][0]['cost'], 0.02)


if __name__ == '__main__':
    unittest.main()
