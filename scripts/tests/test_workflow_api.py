"""Offline contract tests: only the service/CLI boundary is replaced."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from scripts.workflow import api


def assistant(mid='a1', **extra):
    return dict(id=mid, type='assistant', time={'created': 2, 'completed': 3},
                agent='build', model={'id': 'x', 'providerID': 'openai', 'variant': 'medium'},
                content=[{'type': 'text', 'text': 'done'}], finish='stop',
                tokens={'input': 4, 'output': 2, 'reasoning': 1, 'cache': {'read': 3, 'write': 0}},
                cost=0.01, **extra)


class APITests(unittest.TestCase):
    def setUp(self):
        self.client = api.OpenCode(Path.cwd(), timeout=5, max_bytes=10000)
        self.events = []

    def test_cli_uses_existing_auth_and_json_argv_not_shell(self):
        with patch.object(api, 'bounded_command', return_value={'exit': 0, 'output': '{"data":[]}','error': None}) as command:
            self.assertEqual(self.client.models(), [])
        self.assertEqual(command.call_args.args[0], ['opencode', 'api', 'GET', '/api/model'])

    def test_create_primary_build_with_deny_default_and_exact_owned_edits(self):
        ref = {'id': 'x', 'providerID': 'openai', 'variant': 'medium'}
        def service(method, path, data=None, params=None):
            self.assertEqual((method, path), ('POST', '/api/session'))
            self.assertEqual(data['agent'], 'build')
            rules = data['permissions']
            self.assertEqual(rules[0], {'action': '*', 'resource': '*', 'effect': 'deny'})
            edits = [r for r in rules if r['action'] == 'edit' and r['effect'] == 'allow']
            self.assertEqual(edits, [{'action': 'edit', 'resource': 'docs/a.md', 'effect': 'allow'}])
            reads = [r['resource'] for r in rules if r['action'] == 'read' and r['effect'] == 'allow']
            self.assertEqual(reads, ['docs/a.md'])
            self.assertFalse(any(r['action'] in ('grep', 'glob') and r['effect'] == 'allow' for r in rules))
            return {'data': dict(id='ses_test', **data)}
        with patch.object(self.client, 'request', side_effect=service):
            self.assertEqual(self.client.create('task', ref, ['docs/a.md']), 'ses_test')
        with patch.object(self.client, 'request', return_value={'data': {'id': 'ses_bad'}}):
            with self.assertRaises(api.APIError):
                self.client.create('task', ref, [])

    def test_admission_is_not_answer_follow_pages_and_correlate_idle(self):
        user = {'id': 'u1', 'type': 'user', 'sessionID': 'ses_test', 'time': {'created': 1}}
        calls = []
        def service(method, path, data=None, params=None):
            calls.append((method, path, data, params))
            if path.endswith('/prompt'):
                return {'data': user}
            if path.endswith('/wait'):
                return None
            if params == {'order': 'asc'}:
                return {'data': [{'id': 'old', 'type': 'idle', 'outcome': 'succeeded'}, user], 'cursor': {'next': 'p2', 'previous': None}}
            self.assertEqual(params, {'cursor': 'p2'})
            return {'data': [assistant(), {'id': 'i1', 'type': 'idle', 'outcome': 'succeeded', 'time': {'created': 4}}], 'cursor': {'next': None, 'previous': 'p1'}}
        with patch.object(self.client, 'request', side_effect=service):
            self.assertEqual(self.client.turn('ses_test', 'work', self.events.append), 'done')
        self.assertEqual([e['message']['id'] for e in self.events if e['kind'] == 'assistant'], ['a1'])
        self.assertEqual(len([c for c in calls if c[1].endswith('/prompt')]), 1)

    def test_prior_idle_missing_admission_and_incomplete_assistant_fail_closed(self):
        for messages in ([{'id': 'old', 'type': 'idle', 'outcome': 'succeeded'}],
                         [{'id': 'u1', 'type': 'user'}, assistant(time_override=0)],
                         [{'id': 'u1', 'type': 'user'}, dict(assistant(), time={'created': 2}), {'id': 'i', 'type': 'idle', 'outcome': 'succeeded'}]):
            with self.subTest(messages=messages):
                replies = [{'data': {'id': 'u1', 'type': 'user', 'sessionID': 'ses_test'}}, None,
                           {'data': messages, 'cursor': {'next': None}}]
                with patch.object(self.client, 'request', side_effect=replies):
                    with self.assertRaises(api.APIError) as error:
                        self.client.turn('ses_test', 'work', self.events.append)
                    self.assertFalse(error.exception.retryable)

    def test_failed_round_captures_usage_and_security_never_retries(self):
        for kind, status, retry in [('RateLimitError', 429, False), ('AuthenticationError', 401, False),
                                    ('QuotaExceeded', 400, False), ('SecurityError', 403, False),
                                    ('InsufficientCredits', 400, False), ('InvalidAPIKey', 400, False),
                                    ('ProviderError', 500, True)]:
            error = {'type': kind, 'message': kind, 'status': status}
            messages = [{'id': 'u1', 'type': 'user'}, dict(assistant(), finish='error', error=error),
                        {'id': 'i1', 'type': 'idle', 'outcome': 'failed', 'error': error}]
            replies = [{'data': {'id': 'u1', 'type': 'user', 'sessionID': 'ses_test'}}, None,
                       {'data': messages, 'cursor': {'next': None}}]
            events = []
            with patch.object(self.client, 'request', side_effect=replies):
                with self.assertRaises(api.APIError) as caught:
                    self.client.turn('ses_test', 'work', events.append)
                self.assertEqual(caught.exception.retryable, retry)
            self.assertEqual([e['message']['cost'] for e in events if e['kind'] == 'assistant'], [0.01])

    def test_ambiguous_post_never_resubmits_but_collects_available_usage(self):
        with patch.object(self.client, 'request', side_effect=[api.APIError('timeout'),
                          {'data': [assistant()], 'cursor': {'next': None}}]) as request:
            with self.assertRaises(api.APIError):
                self.client.turn('ses_test', 'work', self.events.append)
        self.assertEqual(request.call_count, 2)
        self.assertTrue(any(e['kind'] == 'assistant' for e in self.events))

    def test_pagination_loop_and_response_bounds_fail(self):
        with patch.object(self.client, 'request', return_value={'data': [], 'cursor': {'next': 'repeat'}}):
            with self.assertRaises(api.APIError):
                self.client.messages('ses_test', self.events.append)
        self.client.max_bytes = 4
        replies = [{'data': {'id': 'u1', 'type': 'user', 'sessionID': 'ses_test'}}, None,
                   {'data': [{'id': 'u1', 'type': 'user'}, dict(assistant(), content=[{'type': 'text', 'text': 'too long'}]),
                             {'id': 'i1', 'type': 'idle', 'outcome': 'succeeded'}], 'cursor': {'next': None}}]
        with patch.object(self.client, 'request', side_effect=replies):
            with self.assertRaisesRegex(api.APIError, 'response limit'):
                self.client.turn('ses_test', 'work', self.events.append)

    def test_non_utf8_output_is_an_explicit_failure_not_missing_evidence(self):
        result = api.bounded_command([sys.executable, '-c', 'import os; os.write(1, bytes([255]))'], Path.cwd(), 3, 100)
        self.assertIsNotNone(result['error'])

    def test_switch_must_confirm_returned_model(self):
        ref = {'id': 'x', 'providerID': 'openai', 'variant': 'high'}
        with patch.object(self.client, 'request', side_effect=[None, {'data': {'id': 'ses_test', 'model': dict(ref, variant='low')}}]):
            with self.assertRaises(api.APIError):
                self.client.switch('ses_test', ref)

    def test_session_creation_failure_keeps_known_session_identity(self):
        ref = {'id': 'x', 'providerID': 'openai', 'variant': 'medium'}
        with patch.object(self.client, 'request', return_value={'data': {'id': 'ses_known'}}):
            with self.assertRaises(api.APIError) as caught:
                self.client.create('task', ref, [])
        self.assertEqual(caught.exception.session_id, 'ses_known')

    def test_response_limit_covers_all_assistant_text_not_just_last(self):
        self.client.max_bytes = 5
        replies = [{'data': {'id': 'u1', 'type': 'user', 'sessionID': 'ses_test'}}, None,
                   {'data': [{'id': 'u1', 'type': 'user'}, assistant(), assistant('a2'),
                             {'id': 'i1', 'type': 'idle', 'outcome': 'succeeded'}], 'cursor': {'next': None}}]
        with patch.object(self.client, 'request', side_effect=replies):
            with self.assertRaisesRegex(api.APIError, 'response limit'):
                self.client.turn('ses_test', 'work', self.events.append)

    def test_validation_subprocess_timeout_and_output_are_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            result = api.bounded_command([sys.executable, '-c', 'print("x"*100000)'], Path(directory), 3, 100)
            self.assertIsNotNone(result['error'])
            self.assertLessEqual(len(result['output'].encode()), 100)
            result = api.bounded_command([sys.executable, '-c', 'import time; time.sleep(5)'], Path(directory), 0.05, 100)
            self.assertEqual(result['error'], 'timeout')


if __name__ == '__main__':
    unittest.main()
