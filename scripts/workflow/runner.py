"""Finite, fail-closed workflow. Git trees and durable JSON are the evidence seam."""
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import time
import uuid

from . import api, metrics, policy


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8'), object_pairs_hook=policy._pairs,
                      parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)))


def _keys(value, expected, where):
    policy._keys(value, set(expected.split()), where)


def _texts(value, where):
    if not isinstance(value, list) or not value or any(not policy._text(v) for v in value):
        raise ValueError(f'{where}: expected nonempty text array')


def owned_path(root, value):
    if (not policy._path(value) or any(c in value for c in '*?[]\n\r\t')
            or any(part.lower() == '.git' for part in value.split('/'))):
        raise ValueError(f'unsafe exact relative file path: {value!r}')
    path = root / value
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError(f'path escapes worktree: {value}')
    # No symlinks, including in-worktree aliases: permissions and snapshots must
    # name one unambiguous file. Submodules/special files are unsupported.
    for parent in (path, *path.parents):
        if parent == root:
            break
        if parent.is_symlink():
            raise ValueError(f'symlink path unsupported: {value}')
    if path.exists() and not path.is_file():
        raise ValueError(f'not a regular file: {value}')
    return path


def _files(value, root):
    _texts(value, 'allowed_files')
    if len(value) != len(set(value)):
        raise ValueError('duplicate allowed file')
    for name in value:
        owned_path(root, name)


def _validations(commands):
    if not isinstance(commands, list) or not commands:
        raise ValueError('validation is required')
    for argv in commands:
        _texts(argv, 'validation argv')
        if any('\x00' in arg for arg in argv):
            raise ValueError('NUL in validation argv')


def validate_manifest(request, root, config):
    optional = ''.join(' ' + key for key in ('task_validation', 'task_overrides') if isinstance(request, dict) and key in request)
    _keys(request, 'id requirements acceptance_criteria allowed_files context complexity risk validation expected_head expected_branch' + optional, 'request')
    if not isinstance(request['id'], str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,79}', request['id']):
        raise ValueError('invalid unique run ID')
    for key in ('requirements', 'acceptance_criteria'):
        _texts(request[key], key)
    _files(request['allowed_files'], root)
    if request['complexity'] not in ('simple', 'complex') or request['risk'] not in ('trivial', 'normal', 'high'):
        raise ValueError('invalid complexity/risk')
    if not isinstance(request['expected_head'], str) or not re.fullmatch(r'[a-f0-9]{40}|[a-f0-9]{64}', request['expected_head']):
        raise ValueError('expected_head must be a full Git object ID')
    if not policy._text(request['expected_branch']) or request['expected_branch'].startswith('-'):
        raise ValueError('invalid expected branch')
    _validations(request['validation'])
    local = request.get('task_validation', {})
    if not isinstance(local, dict):
        raise ValueError('task_validation must map task IDs to trusted argv arrays')
    for task_id, commands in local.items():
        if not isinstance(task_id, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,79}', task_id):
            raise ValueError('invalid task_validation task ID')
        _validations(commands)
    overrides = request.get('task_overrides', {})
    if not isinstance(overrides, dict):
        raise ValueError('task_overrides must map task IDs to explicit overrides')
    for task_id, override in overrides.items():
        if not isinstance(task_id, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,79}', task_id):
            raise ValueError('invalid override task ID')
        if not isinstance(override, dict):
            raise ValueError('invalid task override')
    if not isinstance(request['context'], list) or len(request['context']) > 100:
        raise ValueError('invalid bounded context list')
    for entry in request['context']:
        if not isinstance(entry, dict) or set(entry) not in ({'file'}, {'file', 'start', 'end'}):
            raise ValueError('context requires file and optional start/end')
        owned_path(root, entry['file'])
        if 'start' in entry and (type(entry['start']) is not int or type(entry['end']) is not int or not 1 <= entry['start'] <= entry['end']):
            raise ValueError('invalid context line range')
    context_text(root, request['context'], config['limits']['max_context_bytes'])
    return copy.deepcopy(request)


def context_text(root, entries, limit):
    chunks, used = [], 0
    for entry in entries:
        path = owned_path(root, entry['file'])
        header = json.dumps(entry) + '\n'
        used += len(header.encode())
        chunks.append(header)
        last = 0
        with path.open('r', encoding='utf-8') as handle:
            # readline's bound prevents a gigantic single-line allocation.
            while True:
                line = handle.readline(limit + 1)
                if not line:
                    break
                last += 1
                if 'start' in entry and last < entry['start']:
                    if len(line.encode()) > limit:
                        raise ValueError('context line exceeds byte limit')
                    continue
                used += len(line.encode())
                if used > limit:
                    raise ValueError('context byte limit exceeded (not truncated)')
                chunks.append(line)
                if 'end' in entry and last == entry['end']:
                    break
        if 'end' in entry and last < entry['end']:
            raise ValueError('context range beyond end of file')
    if used > limit:
        raise ValueError('context byte limit exceeded')
    return ''.join(chunks)


class Git:
    def __init__(self, root):
        self.root = Path(root).resolve()

    def call(self, *args, env=None, input_data=None, limit=8 * 1024 * 1024):
        environment = dict(os.environ, GIT_NO_REPLACE_OBJECTS='1', GIT_TERMINAL_PROMPT='0', GIT_LITERAL_PATHSPECS='1')
        # Do not inherit an unrelated index or repository from the caller.
        for key in ('GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_COMMON_DIR'):
            environment.pop(key, None)
        environment.update(env or {})
        result = api.bounded_command(['git', '-C', str(self.root), *args], self.root, 60, limit,
                                     env=environment, input_data=input_data)
        if result['error'] or result['exit'] != 0:
            raise ValueError(f'Git {args[0]}: {result["error"] or result["output"]}')
        return result['output']

    def identity(self):
        return {'head': self.call('rev-parse', 'HEAD').strip(),
                'branch': self.call('symbolic-ref', '--short', 'HEAD').strip()}

    def check(self, identity):
        if self.identity() != identity:
            raise ValueError('unexpected HEAD/branch change; edits preserved')

    def directory(self):
        return Path(self.call('rev-parse', '--absolute-git-dir').strip()) / 'agent-workflow'

    def snapshot(self):
        # Include ignored files too: a clean linked worktree cannot hide generated
        # or malicious edits behind .gitignore. Validation should write elsewhere.
        names = sorted(set(filter(None, self.call('ls-files', '--cached', '--others', '-z').split('\x00'))))
        entries, present = [], []
        for name in names:
            path = owned_path(self.root, name)
            if path.exists():
                mode = path.stat().st_mode
                if not stat.S_ISREG(mode):
                    raise ValueError('special file unsupported')
                present.append((name, '100755' if mode & 0o111 else '100644'))
        for offset in range(0, len(present), 100):
            batch = present[offset:offset + 100]
            hashes = self.call('hash-object', '-w', '--no-filters', '--', *[n for n, _ in batch]).splitlines()
            if len(hashes) != len(batch):
                raise ValueError('snapshot hash mismatch')
            entries += [f'{mode} {sha}\t{name}\x00' for (name, mode), sha in zip(batch, hashes)]
        directory = self.directory()
        directory.mkdir(parents=True, exist_ok=True)
        index = directory / ('index-' + uuid.uuid4().hex)
        env = {'GIT_INDEX_FILE': str(index)}
        try:
            self.call('read-tree', '--empty', env=env)
            self.call('update-index', '-z', '--index-info', env=env, input_data=''.join(entries).encode())
            return self.call('write-tree', env=env).strip()
        finally:
            index.unlink(missing_ok=True)

    def diff(self, before, after, limit):
        paths = list(filter(None, self.call('diff-tree', '--no-commit-id', '-r', '--no-renames', '--name-only', '-z', before, after).split('\x00')))
        numstat = self.call('diff', '--no-ext-diff', '--no-textconv', '--no-renames', '--numstat', '-z', before, after, '--')
        added = deleted = 0
        binary = False
        for row in filter(None, numstat.split('\x00')):
            a, d, _ = row.split('\t', 2)
            if a == '-' or d == '-':
                binary = True
            else:
                added += int(a)
                deleted += int(d)
        text = self.call('diff', '--no-ext-diff', '--no-textconv', '--no-renames', '--binary', before, after, '--', limit=limit)
        return {'files': paths, 'added': added, 'deleted': deleted, 'binary': binary,
                'text': text, 'sha256': hashlib.sha256(text.encode()).hexdigest(), 'before': before, 'after': after}


def _atomic(path, data):
    temporary = path.with_suffix('.tmp')
    with temporary.open('w', encoding='utf-8') as handle:
        json.dump(data, handle, indent=2, sort_keys=True, allow_nan=False)
        handle.write('\n')
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)
    fd = os.open(path.parent, os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


class Lock:
    def __init__(self, directory):
        self.path = directory / 'lock'
        directory.mkdir(parents=True, exist_ok=True)

    def __enter__(self):
        try:
            self.fd = os.open(self.path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        except FileExistsError as exc:
            raise ValueError(f'worktree locked; reconcile manually: {self.path}') from exc
        os.write(self.fd, json.dumps({'pid': os.getpid(), 'created': time.time()}).encode())
        os.fsync(self.fd)
        return self

    def __exit__(self, *_):
        os.close(self.fd)
        self.path.unlink()


def _tasks(text, request, root, config):
    tasks = json.loads(text, object_pairs_hook=policy._pairs)
    if not isinstance(tasks, list) or not 1 <= len(tasks) <= config['limits']['max_tasks']:
        raise ValueError('invalid planner task count')
    by_id = {}
    for task in tasks:
        _keys(task, 'id requirements acceptance_criteria allowed_files depends_on', 'planned task')
        if not isinstance(task['id'], str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,79}', task['id']) or task['id'] in by_id:
            raise ValueError('invalid/duplicate task ID')
        for key in ('requirements', 'acceptance_criteria'):
            _texts(task[key], key)
        _files(task['allowed_files'], root)
        if not set(task['allowed_files']) <= set(request['allowed_files']):
            raise ValueError('planner ownership outside manifest')
        if not isinstance(task['depends_on'], list) or any(not isinstance(d, str) for d in task['depends_on']) or len(task['depends_on']) != len(set(task['depends_on'])):
            raise ValueError('invalid dependencies')
        by_id[task['id']] = task
    ordered, done = [], set()
    while len(ordered) < len(tasks):
        ready = [t for t in tasks if t['id'] not in done and set(t['depends_on']) <= done]
        if not ready:
            raise ValueError('unknown dependency or dependency cycle')
        for task in ready:
            ordered.append(task)
            done.add(task['id'])
    return ordered


def run(root, manifest, config, service):
    root = Path(root).resolve()
    request = validate_manifest(manifest, root, config)
    git = Git(root)
    expected = {'head': request['expected_head'], 'branch': request['expected_branch']}
    directory = git.directory()
    with Lock(directory):
        runs = directory / 'runs'
        runs.mkdir(exist_ok=True)
        path = runs / (request['id'] + '.json')
        if path.exists():
            raise ValueError('duplicate persistent run ID; use a new clean linked worktree to retry')
        if any(read_json(previous)['status'] in ('failed', 'blocked', 'running') for previous in runs.glob('*.json')):
            raise ValueError('previous unresolved run; use a new clean linked worktree, not a new run ID')
        if (not (root / '.git').is_file() or expected['branch'] in ('main', 'master')
                or Path(git.call('rev-parse', '--path-format=absolute', '--git-common-dir').strip()) == directory.parent):
            raise ValueError('run requires linked worktree on a non-main/master branch')
        git.check(expected)
        if git.call('status', '--porcelain=v1', '-z', '--untracked-files=all'):
            raise ValueError('run requires a clean worktree/index')
        baseline = git.snapshot()
        if baseline != git.call('rev-parse', 'HEAD^{tree}').strip():
            raise ValueError('run requires clean worktree (including ignored files)')
        state = {'version': 2, 'id': request['id'], 'root': str(root), 'identity': expected, 'baseline': baseline,
                 'request': request, 'config': copy.deepcopy(config), 'status': 'running', 'tasks': [],
                 'selected_mode': config.get('selected_mode', 'openai'),
                 'records': [], 'calls': [], 'events': [], 'error': None, 'created': time.time(),
                 'final_validations': [], 'final_validation_status': 'pending'}
        def save():
            _atomic(path, state)
        save()

        def call(task_id, role, session, prompt):
            git.check(expected)
            routed_role = role if role in config['mode_roles'] else policy.ALIASES.get(role, 'implementer_standard')
            if role == 'escalation':
                routed_role = 'recovery'
            call_state = {'id': len(state['calls']) + 1, 'task_id': task_id, 'role': role,
                          'session_id': session, 'status': 'pending', 'user_id': None,
                          'model': state['refs'][routed_role], 'invocation_role': role,
                          'invocation_model': state['refs'][routed_role], 'message_ids': [], 'usage_complete': False,
                          'started': time.time()}
            state['calls'].append(call_state)
            save()
            def event(item):
                state['events'].append(dict(item, call_id=call_state['id']))
                if item['kind'] == 'admitted':
                    call_state['user_id'] = item['user_id']
                if item['kind'] in ('terminal', 'completed'):
                    call_state['usage_complete'] = True
                if item['kind'] == 'assistant' or (item['kind'] == 'assistant_message'):
                    message = item['message']
                    key = (session, message['id'])
                    if not any((r['session_id'], r['message_id']) == key for r in state['records']):
                        call_state['message_ids'].append(message['id'])
                        expected_role = routed_role
                        model = next((m for m in state['catalog'] if m.get('providerID') == config['mode_roles'][expected_role]['model'].split('/')[0]
                                      and m.get('modelID') == config['mode_roles'][expected_role]['model'].split('/')[1]), None)
                        state['records'].append(metrics.message_record(task_id, routed_role, session, message, config, model))
                        save()
                        observed = message.get('model')
                        if isinstance(observed, str):
                            observed = state['refs'][expected_role] if observed == config['mode_roles'][expected_role]['model'] else observed
                        elif isinstance(observed, dict) and 'variant' not in observed and isinstance(observed.get('id'), str):
                            observed = dict(observed, variant=config['mode_roles'][expected_role]['effort'])
                        if observed != state['refs'][expected_role]:
                            raise ValueError('assistant used unexpected provider/model/effort; usage preserved, no fallback')
                call_state['duration_seconds'] = time.time() - call_state['started']
                save()
            try:
                answer = service.turn(session, prompt, event)
                if not isinstance(answer, str) or not answer.strip() or len(answer.encode()) > config['limits']['max_response_bytes']:
                    raise ValueError('invalid/oversized model response')
                call_state.update(status='completed', response=answer)
                return answer
            except Exception as exc:
                call_state.update(status='failed', error=str(exc))
                # A failed call without messages must remain unknown, not zero.
                if not call_state['usage_complete'] or not call_state['message_ids']:
                    state['records'].append(metrics.message_record(task_id, routed_role, session,
                        {'id': 'unknown-call-' + str(call_state['id']), 'error': str(exc)}, config))
                raise
            finally:
                call_state['duration_seconds'] = time.time() - call_state['started']
                save()
                git.check(expected)

        def create(title, role, files):
            git.check(expected)
            # Persist intent before POST; a creation timeout has unknown session ID.
            entry = {'kind': 'session_creation', 'title': title, 'role': role, 'session_id': None}
            state['events'].append(entry)
            save()
            try:
                sid = service.create(title, state['refs'][role], files,
                                     read_files=sorted(set(request['allowed_files'] + [e['file'] for e in request['context']])))
                entry['session_id'] = sid
                save()
                return sid
            except api.APIError as exc:
                entry['session_id'] = exc.session_id
                entry['error'] = str(exc)
                save()
                raise
            finally:
                git.check(expected)

        def validate(commands, evidence, attempt=None):
            results = []
            for argv in commands:
                git.check(expected)
                result = dict(api.bounded_command(argv, root, config['limits']['validation_timeout_seconds'],
                                                  config['limits']['max_validation_output_bytes'],
                                                  env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1')), argv=argv)
                results.append(result)
                evidence.append(dict(result, attempt=attempt))
                save()
                git.check(expected)
            return results

        current = None
        try:
            git.check(expected)
            state['catalog'] = service.models()
            git.check(expected)
            state['refs'] = policy.validate_models(config, state['catalog'])
            for alias, target in policy.ALIASES.items():
                state['refs'][alias] = state['refs'][target]
            state['catalog_identity'] = policy.catalog_identity(state['catalog'])
            save()
            context = context_text(root, request['context'], config['limits']['max_context_bytes'])
            if request['complexity'] == 'simple' and len(request['allowed_files']) <= config['limits']['simple_max_files']:
                tasks = [{k: request[k] for k in ('id', 'requirements', 'acceptance_criteria', 'allowed_files')} | {'depends_on': []}]
                state['planning'] = 'simple bypass'
            else:
                sid = create('plan:' + request['id'], 'orchestrator', [])
                prompt = ('Plan only; no edits, shell, subagents or commands. Return ONLY a JSON array of tasks, each with exactly '
                          'id,requirements (text array),acceptance_criteria (text array),allowed_files (exact files),depends_on (IDs). '
                          'Include each trusted task_validation_id on its matching task; commands are controller-owned. '
                          'Keep all original requirements covered; bounded by max_tasks=' + str(config['limits']['max_tasks']) + '.\n'
                          + json.dumps({k: request[k] for k in ('requirements', 'acceptance_criteria', 'allowed_files')}
                                       | {'task_validation_ids': sorted(request.get('task_validation', {}))}) + '\nCONTEXT\n' + context)
                tasks = _tasks(call(request['id'], 'orchestrator', sid, prompt), request, root, config)
                if git.snapshot() != baseline:
                    raise ValueError('read-only planner modified worktree')
                state['planning'] = 'planned once'
            local_checks = request.get('task_validation', {})
            if not set(local_checks) <= {task['id'] for task in tasks}:
                raise ValueError('task_validation references unknown task ID')
            if not set(request.get('task_overrides', {})) <= {task['id'] for task in tasks}:
                raise ValueError('task_overrides references unknown task ID')
            for task in tasks:
                classification = policy.classify_task(config, task['allowed_files'], request['complexity'], request['risk'])
                current = dict(task, status='failed', attempts=0, reviews=0, rejections=0, escalations=0,
                               classification=classification, started=time.time(),
                               validations=[], merged_lines=None, baseline=git.snapshot())
                state['tasks'].append(current)
                save()
                role = ('recovery' if request['complexity'] == 'complex' else
                        {'S': 'implementer_simple', 'M': 'implementer_standard', 'L': 'implementer_complex'}[classification['class']])
                override = request.get('task_overrides', {}).get(task['id'])
                if override is not None:
                    current['override'] = policy.validate_override(config, override, state['catalog'], state['refs'])
                    role = current['override']['role']
                current['route'] = {'role': role, 'model': config['mode_roles'][role]['model'],
                                    'effort': config['mode_roles'][role]['effort'],
                                    'provider': config['selected_mode'], 'catalog_identity': state['catalog_identity']}
                sid = create('implement:' + task['id'], role, task['allowed_files'])
                current['session_id'] = sid
                active_role = role
                feedback = None
                # Global acceptance is not a prerequisite for partial tasks.
                # A single task can still use the global commands for retries.
                commands = local_checks.get(task['id'], request['validation'] if len(tasks) == 1 else [])
                for attempt in range(1, config['limits']['max_attempts'] + 1):
                    current['attempts'] = attempt
                    if attempt > config['limits']['escalate_after']:
                        role = 'recovery'
                    if role == 'recovery':
                        current['escalations'] += 1
                    save()
                    if role != active_role:
                        git.check(expected)
                        service.switch(sid, state['refs'][role])
                        git.check(expected)
                        active_role = role
                    prompt = ('Implement only the owned files. No Git operations, shell, subagents, or commands. '
                              'The controller runs validation. Preserve other work. Report when edits are ready.\n'
                              + json.dumps(task) + '\nCONTEXT\n' + context) if attempt == 1 else (
                              'Targeted retry in this session; do not repeat unchanged context. Fix this feedback only:\n' + json.dumps(feedback))
                    try:
                        call(task['id'], role, sid, prompt)
                    except api.APIError as exc:
                        if not exc.retryable or attempt == config['limits']['max_attempts']:
                            raise
                        feedback = {'completed_model_error': str(exc)}
                        continue
                    after = git.snapshot()
                    diff = git.diff(current['baseline'], after, config['limits']['max_diff_bytes'])
                    current['diff'] = diff
                    save()
                    if not set(diff['files']) <= set(task['allowed_files']):
                        raise ValueError('ownership violation; edits preserved')
                    results = validate(commands, current['validations'], attempt)
                    validated_tree = git.snapshot()
                    if validated_tree != after:
                        raise ValueError('validation modified worktree; validation must be read-only')
                    if not diff['files'] or any(r['exit'] != 0 or r['error'] for r in results):
                        feedback = {'validation': results, 'error': 'empty diff' if not diff['files'] else 'validation failed',
                                    'diff_sha256': diff['sha256'], 'evidence': str(path)}
                        continue
                    current['deterministic_validation'] = 'passed' if commands else 'unavailable'
                    review = policy.review_policy(config, diff['files'], diff['added'], diff['deleted'], request['risk'], bool(commands), diff['binary'])
                    if review['required']:
                        review['role'] = 'reviewer_complex' if review['risk'] == 'high' else 'reviewer_standard'
                    current['review_policy'] = review
                    save()
                    if review['required']:
                        if current['reviews'] >= config['limits']['max_reviews']:
                            raise ValueError('review cap reached without acceptance')
                        current['reviews'] += 1
                        save()
                        review_sid = create('review:' + task['id'] + ':' + str(current['reviews']), review['role'], [])
                        review_prompt = ('Independent read-only review. Do not edit, execute commands or delegate. '
                            'Review this task\'s requirements/acceptance against its complete actual diff; not a summary. '
                            'Original request criteria are context: final request acceptance remains pending until all tasks and global checks finish. '
                            'Return ONLY JSON {"decision":"approve|request_changes|block","findings":['
                            '{"file":"owned path","line":1,"severity":"critical|important|minor","message":"actionable fix"}]}. '
                            'Approve only if clean/pass; minor nonblocking findings may be recorded.\n'
                            + json.dumps({'task': task, 'original_requirements': request['requirements'],
                                          'original_acceptance_criteria': request['acceptance_criteria'], 'validation': results,
                                          'deterministic_validation': current['deterministic_validation'], 'final_acceptance': 'pending'})
                            + '\nCOMPLETE DIFF\n' + diff['text'])
                        parsed = policy.parse_review(call(task['id'], review['role'], review_sid, review_prompt))
                        if any(f['file'] not in diff['files'] for f in parsed['findings']):
                            raise ValueError('review finding outside actual task diff')
                        current.setdefault('review_results', []).append(parsed)
                        if git.snapshot() != after:
                            raise ValueError('read-only reviewer modified worktree')
                        if parsed['decision'] == 'block':
                            current['status'] = 'blocked'
                            raise ValueError('review blocked')
                        if parsed['decision'] == 'request_changes':
                            current['rejections'] += 1
                            save()
                            if not parsed['findings']:
                                raise ValueError('request_changes requires actionable findings')
                            if current['reviews'] >= config['limits']['max_reviews']:
                                raise ValueError('review cap reached without acceptance')
                            feedback = {'review': parsed, 'diff_sha256': diff['sha256'], 'evidence': str(path)}
                            continue
                    current.update(status='provisional', provisional_tree=after, provisional_identity=git.identity())
                    current['duration_seconds'] = time.time() - current['started']
                    save()
                    break
                if current['status'] != 'provisional':
                    raise ValueError('attempt cap reached without acceptance')
            final_tree = git.snapshot()
            state['final_diff'] = git.diff(baseline, final_tree, config['limits']['max_diff_bytes'])
            state['final_validation_status'] = 'failed'
            save()
            results = validate(request['validation'], state['final_validations'])
            if git.snapshot() != final_tree:
                raise ValueError('final validation modified worktree; validation must be read-only')
            if any(r['exit'] != 0 or r['error'] for r in results):
                raise ValueError('final request validation failed; explicit recovery required')
            state['final_validation_status'] = 'passed'
            if not state['final_diff']['files']:
                raise ValueError('empty final run diff; accepted subtasks undo all work')
            git.check(expected)
            for task in state['tasks']:
                task.update(status='accepted', accepted_tree=task['provisional_tree'], accepted_identity=task['provisional_identity'])
            state.update(status='accepted', accepted_tree=final_tree, accepted_identity=git.identity(), accepted_diff=state['final_diff'])
        except Exception as exc:
            state.update(status='blocked' if current and current['status'] == 'blocked' else 'failed', error=f'{type(exc).__name__}: {exc}')
            # Best-effort final snapshot cannot overwrite the primary failure.
            try:
                state['failed_tree'] = git.snapshot()
            except Exception as snapshot_error:
                state['snapshot_error'] = str(snapshot_error)
        finally:
            state['finished'] = time.time()
            save()
        return state


def record_merge(root, run_id, commit, target):
    git = Git(root)
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,79}', run_id) or not re.fullmatch(r'[a-f0-9]{40}|[a-f0-9]{64}', commit) or not policy._text(target) or target.startswith('-'):
        raise ValueError('invalid merge identity arguments')
    directory = git.directory()
    with Lock(directory):
        path = directory / 'runs' / (run_id + '.json')
        state = read_json(path)
        if state['root'] != str(Path(root).resolve()) or state['status'] != 'accepted':
            raise ValueError('only this worktree accepted runs can be recorded')
        base = state['identity']['head']
        target_ref = target if target.startswith('refs/') else 'refs/heads/' + target
        git.call('check-ref-format', target_ref)
        git.call('show-ref', '--verify', target_ref)
        target_sha = git.call('rev-parse', '--verify', target_ref + '^{commit}').strip()
        git.call('merge-base', '--is-ancestor', commit, target_sha)
        git.call('merge-base', '--is-ancestor', base, commit)
        # Strict preserving proof: a direct, single-parent commit of exactly the
        # accepted tree on the original BASE, included verbatim in target.
        parents = git.call('rev-list', '--parents', '-n', '1', commit).split()
        if parents != [commit, base] or git.call('rev-parse', commit + '^{tree}').strip() != state['accepted_tree']:
            raise ValueError('commit does not preserve exact accepted baseline/tree identity')
        actual = git.diff(state['baseline'], state['accepted_tree'], state['config']['limits']['max_diff_bytes'])
        if actual != state['accepted_diff'] or not actual['files']:
            raise ValueError('accepted patch identity mismatch')
        last_owner = {name: task['id'] for task in state['tasks'] for name in task['diff']['files']}
        for task in state['tasks']:
            task['status'] = 'merged'
            final_diff = dict(actual, files=[name for name in actual['files'] if last_owner[name] == task['id']])
            task['merged_lines'] = useful_lines(git, final_diff)
        state.update(status='merged', merge={'commit': commit, 'target_ref': target_ref, 'target_commit': target_sha,
                                            'base': base, 'tree': state['accepted_tree'], 'patch_sha256': actual['sha256']})
        _atomic(path, state)
        return state


def useful_lines(git, diff):
    """Proxy: nonblank added source lines, excluding docs/tests/generated/config."""
    extensions = {'.rs', '.py', '.js', '.ts', '.tsx', '.jsx', '.wgsl', '.c', '.h', '.cpp', '.sh'}
    count = 0
    for name in diff['files']:
        path = Path(name)
        if (path.suffix not in extensions or any(p.lower() in ('docs', 'tests', 'test', 'generated', 'vendor', 'target', 'dist', '.github', '.opencode') for p in path.parts)
                or path.name.startswith(('test_', 'test-', 'generated')) or path.stem.endswith(('_test', '.test', '.spec'))):
            continue
        text = git.call('diff', '--no-ext-diff', '--no-textconv', '--no-renames', '--unified=0', diff['before'], diff['after'], '--', name)
        count += sum(line.startswith('+') and not line.startswith('+++') and bool(line[1:].strip()) for line in text.splitlines())
    return count


def report(paths):
    states = [read_json(path) for path in paths]
    semantics = {state['config']['output_includes_reasoning'] for state in states}
    convention = semantics.pop() if len(semantics) == 1 else None
    records, tasks, runs = [], [], []
    for state in states:
        records.extend(state['records'])
        for call in state['calls']:
            if not call.get('usage_complete', False) or not call.get('message_ids'):
                unknown_id = 'unknown-call-' + str(call['id'])
                if not any(r['session_id'] == call['session_id'] and r['message_id'] == unknown_id for r in records):
                    legacy_config = state['config']
                    if 'roles' not in legacy_config:
                        legacy_config['roles'] = {}
                    if call['role'] not in legacy_config['roles']:
                        model_ref = call.get('model', {})
                        legacy_config['roles'][call['role']] = {'model': model_ref.get('providerID', 'openai') + '/' + model_ref.get('id', 'unknown')}
                    records.append(metrics.message_record(call['task_id'], call['role'], call['session_id'],
                                   {'id': unknown_id, 'error': call.get('error', 'usage unavailable/incomplete')}, legacy_config))
        eligible = state['status'] in ('accepted', 'merged') and bool(state.get('accepted_diff', {}).get('files'))
        reported_tasks = []
        for task in state['tasks']:
            reported = dict(task)
            if not eligible or task['status'] == 'provisional':
                reported['status'] = 'blocked' if task['status'] == 'blocked' else 'failed'
                if reported['status'] != task['status']:
                    reported['historical_status'] = task['status']
            reported_tasks.append(reported)
            tasks.append(dict(reported, id=state['id'] + ':' + task['id']))
        runs.append({'id': state['id'], 'root': state['root'], 'status': state['status'], 'tasks': reported_tasks,
                     'selected_mode': state.get('selected_mode', state.get('config', {}).get('selected_mode', 'openai')),
                     'calls': state['calls'], 'records': state['records'], 'error': state['error'], 'merge': state.get('merge'),
                     'final_validations': state.get('final_validations', []),
                     'final_validation_status': state.get('final_validation_status', 'unknown')})
        # Planner-only failures still count as failed work, not an empty success.
        if not state['tasks']:
            tasks.append({'id': state['id'], 'status': 'failed', 'attempts': 0, 'reviews': 0, 'rejections': 0, 'escalations': 0})
    result = metrics.summarize(records, tasks, convention)
    result['runs'] = runs
    return result
