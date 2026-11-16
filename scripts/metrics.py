#!/usr/bin/env python3
"""Summarize usage from sanitized OpenCode session-export JSON files."""

import argparse
import json
import math
from pathlib import Path
import sys


COUNTS = ('input', 'output', 'reasoning', 'cache_read', 'cache_write')


def _count(value):
    return value if type(value) is int and value >= 0 else None


def _money(value):
    return value if type(value) in (int, float) and 0 <= value <= 1e100 and math.isfinite(value) else None


def _export_records(export):
    if not isinstance(export, dict) or not isinstance(export.get('info'), dict) or not isinstance(export.get('messages'), list):
        raise ValueError('expected OpenCode session export with info and messages')
    session_id = export['info'].get('id')
    if not isinstance(session_id, str) or not session_id:
        raise ValueError('session export is missing info.id')
    records = []
    for message in export['messages']:
        if not isinstance(message, dict) or message.get('type') != 'assistant':
            continue
        message_id = message.get('id')
        if not isinstance(message_id, str) or not message_id:
            raise ValueError('assistant message is missing id')
        model = message.get('model')
        if isinstance(model, dict) and isinstance(model.get('providerID'), str) and isinstance(model.get('id'), str):
            model_name = model['providerID'] + '/' + model['id']
            variant = model.get('variant')
            if isinstance(variant, str) and variant:
                model_name += '#' + variant
            mode = model['providerID']
        else:
            model_name = 'unknown'
            mode = 'unknown'
        tokens = message.get('tokens') if isinstance(message.get('tokens'), dict) else {}
        cache = tokens.get('cache') if isinstance(tokens.get('cache'), dict) else {}
        usage = {key: _count(tokens.get(key)) for key in ('input', 'output', 'reasoning')}
        usage.update(cache_read=_count(cache.get('read')), cache_write=_count(cache.get('write')))
        cost = _money(message.get('cost'))
        records.append({
            'session_id': session_id,
            'message_id': message_id,
            'agent': message.get('agent') if isinstance(message.get('agent'), str) else 'unknown',
            'model': model_name,
            'mode': mode,
            'tokens': usage,
            'cost': cost,
            'cost_source': 'opencode_reported' if cost is not None else None,
        })
    return records


def _aggregate(records, complete=True):
    if not records:
        return {
            'messages': 0,
            'tokens': dict.fromkeys(COUNTS),
            'cost': None,
            'cost_source': None,
            'coverage': {'tokens_known': dict.fromkeys(COUNTS, 0), 'cost_known': 0},
        }
    tokens = {key: (sum(record['tokens'][key] for record in records)
                    if complete and all(record['tokens'][key] is not None for record in records) else None)
              for key in COUNTS}
    costs = [record['cost'] for record in records]
    cost = sum(costs) if complete and all(value is not None for value in costs) else None
    return {
        'messages': len(records),
        'tokens': tokens,
        'cost': cost,
        'cost_source': 'opencode_reported' if cost is not None else None,
        'coverage': {
            'tokens_known': {key: sum(record['tokens'][key] is not None for record in records)
                             for key in COUNTS},
            'cost_known': sum(value is not None for value in costs),
        },
    }


def summarize_exports(exports):
    """Summarize OpenCode session exports without inferring missing usage or cost."""
    unique = {}
    session_ids = set()
    for export in exports:
        if not isinstance(export, dict) or not isinstance(export.get('info'), dict):
            raise ValueError('expected OpenCode session export with info and messages')
        session_id = export['info'].get('id')
        if not isinstance(session_id, str) or not session_id:
            raise ValueError('session export is missing info.id')
        session_ids.add(session_id)
        for record in _export_records(export):
            key = (record['session_id'], record['message_id'])
            if key in unique and unique[key] != record:
                raise ValueError('conflicting duplicate assistant message')
            unique[key] = record
    records = list(unique.values())
    sessions_with_messages = {record['session_id'] for record in records}
    sessions_without_messages = session_ids - sessions_with_messages
    session_usage_complete = bool(session_ids) and not sessions_without_messages
    by_model = {
        model: _aggregate([record for record in records if record['model'] == model])
        for model in sorted({record['model'] for record in records})
    }
    by_mode = {
        mode: _aggregate([record for record in records if record['mode'] == mode])
        for mode in sorted({record['mode'] for record in records})
    }
    by_agent = {
        agent: _aggregate([record for record in records if record['agent'] == agent])
        for agent in sorted({record['agent'] for record in records})
    }
    return {
        'source': 'opencode_session_export',
        'sessions': len(session_ids),
        'messages': len(records),
        'totals': _aggregate(records, complete=session_usage_complete),
        'coverage': {
            'sessions_with_assistant_messages': len(sessions_with_messages),
            'sessions_without_assistant_messages': len(sessions_without_messages),
            'session_usage_complete': session_usage_complete,
        },
        'by_mode': by_mode,
        'by_model': by_model,
        'by_agent': by_agent,
        'allowance_consumption': None,
        'note': 'Reported zero cost is not evidence of zero economic cost or subscription allowance consumption.',
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('exports', nargs='+', type=Path, help='JSON files from opencode session export --sanitize')
    args = parser.parse_args(argv)
    try:
        exports = [json.loads(path.read_text(encoding='utf-8')) for path in args.exports]
        print(json.dumps(summarize_exports(exports), indent=2, sort_keys=True, allow_nan=False))
        return 0
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as exc:
        print(f'metrics: {type(exc).__name__}: {exc}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
