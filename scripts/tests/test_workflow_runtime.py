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
        return [{'providerID': 'openai', 'modelID': r['model'].split('/')[1], 'enabled': True,
                 'type': 'chat', 'capabilities': {'tools': True}, 'variants': [{'id': r['effort']}], 'cost': []}
                for r in self.config['roles'].values()]

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

    def test_api_security_failure_keeps_usage_and_unknown_coverage_no_hopping(self):
        service = Service(self.root, self.config)
        def failed(sid, text, event):
            event({'kind': 'assistant', 'message': {'id': 'failed', 'cost': 0.02, 'error': {'type': 'QuotaError'}}})
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
