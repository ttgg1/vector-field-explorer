"""Account for unique assistant messages, retaining unknowns rather than inventing usage.

Input, output, cache read and cache write are independent OpenCode counts;
reasoning is recorded separately and included in totals only if configured as
separate from output. Costs are USD; configured/catalog rates are USD/million.
"""

import math


COUNTS = ('input', 'output', 'reasoning', 'cache_read', 'cache_write')


def _count(value):
    return value if type(value) is int and value >= 0 else None


def _money(value):
    return value if type(value) in (int, float) and 0 <= value <= 1e100 and math.isfinite(value) else None


def _model_name(message, role, config):
    model = message.get('model')
    if isinstance(model, dict) and isinstance(model.get('id'), str) and isinstance(model.get('providerID'), str):
        return model['providerID'] + '/' + model['id']
    if isinstance(message.get('modelID'), str) and isinstance(message.get('providerID'), str):
        return message['providerID'] + '/' + message['modelID']
    return config['roles'][role]['model']


def message_record(task_id, role, session_id, message, config, catalog_model=None):
    if not all(isinstance(x, str) and x for x in (task_id, role, session_id)) or not isinstance(message, dict) or not isinstance(message.get('id'), str) or not message['id']:
        raise ValueError('message record requires task, role, session and message ID')
    model = _model_name(message, role, config)
    tokens = message.get('tokens') if isinstance(message.get('tokens'), dict) else {}
    cache = tokens.get('cache') if isinstance(tokens.get('cache'), dict) else {}
    usage = {k: _count(tokens.get(k)) for k in ('input', 'output', 'reasoning')}
    usage.update(cache_read=_count(cache.get('read')), cache_write=_count(cache.get('write')))
    rates = config['prices'].get(model)
    if (rates is None and isinstance(catalog_model, dict)
            and catalog_model.get('providerID') == model.split('/', 1)[0]
            and catalog_model.get('modelID') == model.split('/', 1)[-1]):
        costs = catalog_model.get('cost')
        # Tiered pricing needs context not established here; do not guess a tier.
        if isinstance(costs, list) and len(costs) == 1 and isinstance(costs[0], dict) and 'tier' not in costs[0]:
            c = costs[0]
            cache_rates = c.get('cache') if isinstance(c.get('cache'), dict) else {}
            rates = {'input': c.get('input'), 'output': c.get('output'),
                     'cache_read': cache_rates.get('read'), 'cache_write': cache_rates.get('write')}
    valid_rates = isinstance(rates, dict) and all(_money(rates.get(k)) is not None for k in ('input', 'output', 'cache_read', 'cache_write'))
    reported = _money(message.get('cost'))
    if reported == 0 and not valid_rates and message.get('cost_known') is not True:
        reported = None
    source = 'reported' if reported is not None else None
    cost = reported
    semantics = config.get('output_includes_reasoning')
    if cost is None and valid_rates and semantics is not None and all(usage[k] is not None for k in COUNTS):
        output = usage['output'] + (usage['reasoning'] if semantics is False else 0)
        cost = (usage['input'] * rates['input'] + output * rates['output'] + usage['cache_read'] * rates['cache_read'] + usage['cache_write'] * rates['cache_write']) / 1000000
        source = 'estimated'
    return {'task_id': task_id, 'role': role, 'session_id': session_id, 'message_id': message['id'],
            'model': model, 'usage': usage, 'cost': cost, 'cost_source': source,
            'error': message.get('error'), 'coverage': {'usage': all(usage[k] is not None for k in COUNTS), 'cost': cost is not None}}


def summarize(records, tasks, output_includes_reasoning=None):
    if output_includes_reasoning is not None and type(output_includes_reasoning) is not bool:
        raise ValueError('invalid reasoning semantics')
    unique = {}
    for record in records:
        key = (record['session_id'], record['message_id'])
        if key in unique and unique[key] != record:
            raise ValueError('conflicting duplicate message')
        unique[key] = record
    rows = list(unique.values())
    ids = set()
    for task in tasks:
        if not isinstance(task.get('id'), str) or not task['id'] or task['id'] in ids or task.get('status') not in ('accepted', 'merged', 'failed', 'blocked'):
            raise ValueError('invalid task')
        ids.add(task['id'])
        for key in ('attempts', 'reviews', 'rejections', 'escalations'):
            if _count(task.get(key)) is None:
                raise ValueError(f'invalid task {key}')
        if task.get('merged_lines') is not None and _count(task['merged_lines']) is None:
            raise ValueError('invalid merged lines')
        if task['rejections'] > task['reviews']:
            raise ValueError('rejections exceed reviews')
    totals = {key: sum(row['usage'][key] for row in rows) if all(row['usage'][key] is not None for row in rows) else None for key in COUNTS}
    token_keys = ['input', 'output', 'cache_read', 'cache_write'] + (['reasoning'] if output_includes_reasoning is False else [])
    total_tokens = sum(totals[k] for k in token_keys) if output_includes_reasoning is not None and all(totals[k] is not None for k in COUNTS) else None
    cost = sum(row['cost'] for row in rows) if all(row['cost'] is not None for row in rows) else None
    accepted = [t for t in tasks if t['status'] in ('accepted', 'merged')]
    merged = [t for t in tasks if t['status'] == 'merged']
    def ratio(n, d):
        return n / d if n is not None and d else None
    merged_lines = sum(t['merged_lines'] for t in merged) if merged and all(t.get('merged_lines') is not None for t in merged) else None
    return {'totals': totals, 'total_tokens': total_tokens, 'cost': cost,
            'coverage': {'messages': len(rows), 'cost_known': sum(r['cost'] is not None for r in rows),
                         'usage_known': {k: sum(r['usage'][k] is not None for r in rows) for k in COUNTS}},
            'accepted': len(accepted), 'merged': len(merged),
            'tokens_per_accepted': ratio(total_tokens, len(accepted)), 'cost_per_accepted': ratio(cost, len(accepted)),
            'tokens_per_merged': ratio(total_tokens, len(merged)), 'cost_per_merged': ratio(cost, len(merged)),
            'first_pass_acceptance_rate': ratio(sum(t['attempts'] == 1 for t in accepted), len(tasks)),
            'review_rejection_rate': ratio(sum(t['rejections'] for t in tasks), sum(t['reviews'] for t in tasks)),
            'retry_frequency': ratio(sum(t['attempts'] > 1 for t in tasks), len(tasks)),
            'escalation_frequency': ratio(sum(t['escalations'] > 0 for t in tasks), len(tasks)),
            'useful_merged_lines_per_million_tokens': ratio(merged_lines * 1000000 if merged_lines is not None else None, total_tokens)}
