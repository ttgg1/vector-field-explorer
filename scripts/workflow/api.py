"""Small OpenCode V2 CLI adapter. Admission is never treated as completion."""
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import tempfile
import time
from urllib.parse import quote


class APIError(RuntimeError):
    def __init__(self, message, retryable=False, session_id=None):
        super().__init__(message)
        self.retryable = retryable
        self.session_id = session_id


def bounded_command(argv, cwd, timeout, max_bytes, env=None, input_data=None):
    """Run trusted argv, bounding wall time and combined output; kill its group.

    This is process containment, NOT a sandbox. Inherited user authority remains.
    An over-limit output is explicitly a failure, never truncated success.
    """
    output = bytearray()
    error = None
    source = tempfile.TemporaryFile() if input_data is not None else None
    if source:
        source.write(input_data)
        source.seek(0)
    try:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=source or subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   start_new_session=True)
    except OSError as exc:
        return {'exit': None, 'output': '', 'error': str(exc)}
    finally:
        if source:
            source.close()
    deadline = time.monotonic() + timeout
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        try:
            while selector.get_map():
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    error = 'timeout'
                    break
                for key, _ in selector.select(min(remaining, 0.1)):
                    data = os.read(key.fileobj.fileno(), 65536)
                    if not data:
                        selector.unregister(key.fileobj)
                        continue
                    room = max_bytes - len(output)
                    output.extend(data[:room])
                    if len(data) > room:
                        error = 'output limit exceeded'
                        break
                if error:
                    break
            if not error:
                try:
                    process.wait(timeout=max(0.001, deadline - time.monotonic()))
                except subprocess.TimeoutExpired:
                    error = 'timeout'
        finally:
            # Also terminate descendants holding pipes or deliberately detached from
            # their parent process (but not from this process group).
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            process.stdout.close()
    try:
        text = output.decode('utf-8')
    except UnicodeDecodeError:
        text = output.decode('utf-8', errors='backslashreplace').encode()[:max_bytes].decode('utf-8', errors='ignore')
        error = error or 'non-UTF-8 output (unsupported evidence)'
    return {'exit': process.returncode, 'output': text, 'error': error}


def _security(error):
    return (error.get('status') in (401, 403, 429) or
            bool(re.search(r'auth|security|forbidden|permission|credential|api.?key|too.?many.?requests|limit|quota|budget|credit|spend',
                           str(error.get('type', '')) + ' ' + str(error.get('message', '')), re.I)))


def permissions(files, read_files=None):
    rules = [{'action': '*', 'resource': '*', 'effect': 'deny'}]
    rules += [{'action': 'read', 'resource': f, 'effect': 'allow'} for f in sorted(set(files + (read_files or [])))]
    rules += [{'action': 'edit', 'resource': f, 'effect': 'allow'} for f in files]
    rules += [{'action': action, 'resource': '*', 'effect': 'deny'}
              for action in ('shell', 'subagent', 'execute', 'external_directory')]
    rules += [{'action': 'read', 'resource': resource, 'effect': 'deny'}
              for resource in ('.git', '.git/*', '*.env', '*.env.*')]
    return rules


class OpenCode:
    def __init__(self, root, timeout=600, max_bytes=32768):
        self.root = Path(root).resolve()
        self.timeout = timeout
        self.max_bytes = max_bytes
        self.transport_bytes = 8 * 1024 * 1024

    def request(self, method, path, data=None, params=None):
        argv = ['opencode', 'api', method, path]
        if data is not None:
            argv += ['--data', json.dumps(data, separators=(',', ':'))]
        for key, value in (params or {}).items():
            argv += ['--param', key + '=' + str(value)]
        result = bounded_command(argv, self.root, self.timeout, self.transport_bytes)
        if result['error'] or result['exit'] != 0:
            raise APIError(f'API {method} {path}: {result["error"] or result["output"] or result["exit"]}')
        if not result['output'].strip():
            return None
        try:
            value = json.loads(result['output'])
        except ValueError as exc:
            raise APIError('non-JSON API response') from exc
        if isinstance(value, dict) and 'type' in value and 'message' in value:
            raise APIError(json.dumps(value))
        return value

    def models(self):
        envelope = self.request('GET', '/api/model')
        if not isinstance(envelope, dict) or not isinstance(envelope.get('data'), list):
            raise APIError('invalid model catalog envelope')
        return envelope['data']

    def create(self, title, ref, files, read_files=None):
        rules = permissions(files, read_files)
        data = {'title': title, 'agent': 'build', 'model': ref,
                'location': {'directory': str(self.root)}, 'permissions': rules}
        envelope = self.request('POST', '/api/session', data)
        session = envelope.get('data', {}) if isinstance(envelope, dict) else {}
        if (not isinstance(session.get('id'), str) or not re.fullmatch(r'ses_[A-Za-z0-9]+', session['id'])
                or session.get('permissions') != rules or session.get('agent') != 'build'
                or session.get('model') != ref or session.get('location', {}).get('directory') != str(self.root)):
            raise APIError('session identity/model/permissions not confirmed; reconcile created session manually',
                           session_id=session.get('id'))
        return session['id']

    def switch(self, sid, ref):
        self.request('POST', f'/api/session/{quote(sid, safe="")}/model', {'model': ref})
        envelope = self.request('GET', f'/api/session/{quote(sid, safe="")}')
        if (not isinstance(envelope, dict) or envelope.get('data', {}).get('model') != ref
                or envelope.get('data', {}).get('id') != sid):
            raise APIError('switched model not confirmed')

    def messages(self, sid, event):
        messages, cursors, ids = [], set(), set()
        params = {'order': 'asc'}
        total = 0
        for _ in range(100):
            envelope = self.request('GET', f'/api/session/{quote(sid, safe="")}/message', params=params)
            if not isinstance(envelope, dict) or not isinstance(envelope.get('data'), list) or not isinstance(envelope.get('cursor'), dict):
                raise APIError('invalid timeline envelope')
            for message in envelope['data']:
                if not isinstance(message, dict) or not isinstance(message.get('id'), str) or message['id'] in ids:
                    raise APIError('invalid/duplicate timeline message')
                ids.add(message['id'])
                messages.append(message)
                total += len(json.dumps(message).encode())
                if message.get('type') == 'assistant':
                    event({'kind': 'assistant', 'message': message})
                if total > self.transport_bytes:
                    raise APIError('timeline evidence limit exceeded')
            cursor = envelope['cursor'].get('next')
            if cursor is None:
                return messages
            if not isinstance(cursor, str) or not cursor or cursor in cursors:
                raise APIError('timeline pagination loop/invalid cursor')
            cursors.add(cursor)
            params = {'cursor': cursor}
        raise APIError('timeline page limit exceeded')

    def turn(self, sid, text, event):
        if len(text.encode()) > self.transport_bytes:
            raise APIError('prompt limit exceeded')
        # Response bound is separate from complete diff/context prompt bounds.
        # No arbitrary POST is retried, including admission or wait timeouts.
        admitted = None
        failure = None
        try:
            envelope = self.request('POST', f'/api/session/{quote(sid, safe="")}/prompt', {'text': text})
            user = envelope.get('data', {}) if isinstance(envelope, dict) else {}
            if user.get('type') != 'user' or user.get('sessionID') != sid or not isinstance(user.get('id'), str):
                raise APIError('ambiguous prompt admission')
            admitted = user['id']
            event({'kind': 'admitted', 'user_id': admitted})
            self.request('POST', f'/api/experimental/session/{quote(sid, safe="")}/wait')
        except APIError as exc:
            failure = exc
            event({'kind': 'ambiguous', 'user_id': admitted, 'error': str(exc)})
        try:
            timeline = self.messages(sid, event)
        except APIError:
            if failure:
                raise failure
            raise
        if failure:
            raise failure
        indices = [i for i, message in enumerate(timeline) if message['id'] == admitted and message.get('type') == 'user']
        if len(indices) != 1:
            raise APIError('admitted user absent from timeline; manual reconciliation required')
        round_messages = timeline[indices[0] + 1:]
        if any(m.get('type') == 'user' for m in round_messages):
            raise APIError('unexpected concurrent prompt; manual reconciliation required')
        if not round_messages or round_messages[-1].get('type') != 'idle':
            raise APIError('no terminal idle after admitted prompt')
        idle = round_messages[-1]
        assistants = [m for m in round_messages if m.get('type') == 'assistant']
        event({'kind': 'terminal', 'user_id': admitted, 'idle_id': idle['id'], 'outcome': idle.get('outcome')})
        errors = [m['error'] for m in assistants + [idle] if isinstance(m.get('error'), dict)]
        if errors or idle.get('outcome') != 'succeeded':
            retry = (idle.get('outcome') == 'failed' and bool(errors) and not any(_security(e) for e in errors))
            raise APIError(json.dumps(errors) if errors else 'interrupted/unknown outcome', retryable=retry)
        if not assistants or any(not m.get('time', {}).get('completed') or m.get('finish') not in ('stop', 'tool-calls') for m in assistants):
            raise APIError('incomplete/length-limited assistant result')
        text = '\n'.join(part['text'] for part in assistants[-1].get('content', [])
                         if part.get('type') == 'text' and isinstance(part.get('text'), str))
        response_bytes = sum(len(p['text'].encode()) for m in assistants for p in m.get('content', [])
                             if p.get('type') == 'text' and isinstance(p.get('text'), str))
        if response_bytes > self.max_bytes:
            raise APIError('model response limit exceeded')
        if not text.strip():
            raise APIError('empty model response')
        event({'kind': 'completed', 'user_id': admitted, 'idle_id': idle['id']})
        return text
