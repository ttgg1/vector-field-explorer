"""Configuration and conservative review policy; no side effects beyond loading JSON."""

import fnmatch
import json
import math
import re
from pathlib import PurePosixPath


ROLES = {'orchestrator', 'implementer', 'reviewer', 'escalation'}
MODE_ROLES = {'orchestrator', 'implementer_simple', 'implementer_standard', 'implementer_complex',
              'reviewer_standard', 'reviewer_complex', 'recovery'}
ALIASES = {'orchestrator': 'orchestrator', 'implementer': 'implementer_standard',
           'reviewer': 'reviewer_standard', 'escalation': 'recovery'}
LIMITS = {'max_attempts', 'max_reviews', 'escalate_after', 'max_tasks',
          'max_context_bytes', 'max_diff_bytes', 'max_response_bytes',
          'api_timeout_seconds', 'validation_timeout_seconds',
          'max_validation_output_bytes', 'simple_max_files'}


def _keys(value, expected, where):
    if not isinstance(value, dict) or set(value) != expected:
        raise ValueError(f'{where}: expected keys {sorted(expected)}')


def _positive(value, where):
    if type(value) is not int or value < 1 or value > 100000000:
        raise ValueError(f'{where}: expected positive bounded integer')


def _text(value):
    return isinstance(value, str) and bool(value.strip())


def _path(value):
    if not _text(value) or '\\' in value or '\x00' in value:
        return False
    p = PurePosixPath(value)
    return not p.is_absolute() and all(part not in ('', '..', '.') for part in value.split('/'))


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate key: {key}')
        result[key] = value
    return result


def load_config(path):
    try:
        config = json.loads(path.read_text(encoding='utf-8'), object_pairs_hook=_pairs,
                            parse_constant=lambda x: (_ for _ in ()).throw(ValueError(f'invalid number {x}')))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError(f'cannot load config: {exc}') from exc
    _keys(config, {'workflow', 'modes', 'limits', 'review', 'prices', 'output_includes_reasoning'}, 'config')
    _keys(config['workflow'], {'mode'}, 'workflow')
    _keys(config['modes'], {'openai', 'opencode-go'}, 'modes')
    mode = config['workflow']['mode']
    if not isinstance(mode, str) or mode not in config['modes']:
        raise ValueError('invalid workflow mode')
    for provider, roles in config['modes'].items():
        _keys(roles, MODE_ROLES, provider)
        for name, role in roles.items():
            _keys(role, {'model', 'effort'}, f'{provider}/{name}')
            if (not isinstance(role['model'], str) or
                not re.fullmatch(re.escape(provider) + r'/[A-Za-z0-9][A-Za-z0-9._-]*', role['model']) or
                not isinstance(role['effort'], str) or not re.fullmatch(r'[a-z][a-z0-9_-]*', role['effort'])):
                raise ValueError(f'{provider}/{name}: invalid model or effort')
    _keys(config['limits'], LIMITS, 'limits')
    for key, value in config['limits'].items():
        _positive(value, key)
    _keys(config['review'], {'trivial_max_lines', 'trivial_max_files', 'trivial_paths', 'high_risk_paths', 'skip_trivial'}, 'review')
    review = config['review']
    for key in ('trivial_max_lines', 'trivial_max_files'):
        _positive(review[key], key)
    for key in ('trivial_paths', 'high_risk_paths'):
        if not isinstance(review[key], list) or not review[key] or any(not _path(p) for p in review[key]):
            raise ValueError(f'{key}: expected nonempty relative patterns')
    if type(review['skip_trivial']) is not bool or config['output_includes_reasoning'] not in (None, True, False) or (config['output_includes_reasoning'] is not None and type(config['output_includes_reasoning']) is not bool):
        raise ValueError('invalid boolean')
    if not isinstance(config['prices'], dict):
        raise ValueError('prices must be a mapping')
    for model, rates in config['prices'].items():
        if not _text(model) or not model.startswith('openai/'):
            raise ValueError('invalid price model')
        _keys(rates, {'input', 'output', 'cache_read', 'cache_write'}, 'price')
        if any(type(v) not in (int, float) or v < 0 or v > 1e100 or not math.isfinite(v) for v in rates.values()):
            raise ValueError('invalid price')
    config['selected_mode'] = mode
    config['mode_roles'] = config['modes'][mode]
    config['roles'] = {alias: config['mode_roles'][name] for alias, name in ALIASES.items()}
    return config


def validate_models(config, catalog):
    refs = {}
    mode = config['selected_mode']
    for name, role in config['mode_roles'].items():
        model = role['model']
        if not isinstance(model, str) or not re.fullmatch(re.escape(mode) + r'/[A-Za-z0-9][A-Za-z0-9._-]*', model):
            raise ValueError(f'{mode}/{name}: invalid model')
        model_id = model.split('/', 1)[1]
        matches = [m for m in catalog if isinstance(m, dict) and m.get('providerID') == mode
                   and m.get('modelID') == model_id and m.get('enabled') is True
                   and m.get('status') == 'active'
                   and m.get('type') == 'chat' and isinstance(m.get('capabilities'), dict)
                   and m['capabilities'].get('tools') is True
                   and isinstance(m.get('variants'), list)
                   and any(isinstance(v, dict) and v.get('id') == role['effort'] for v in m['variants'])]
        if not matches:
            raise ValueError(f'{mode}/{name}: unavailable enabled chat/tool model or variant {model}#{role["effort"]}')
        refs[name] = {'id': model_id, 'providerID': mode, 'variant': role['effort']}
    refs.update({alias: refs[name] for alias, name in ALIASES.items()})
    return refs


def catalog_identity(catalog):
    """Stable digest of the exact discovery snapshot used for a resolution."""
    import hashlib
    return hashlib.sha256(json.dumps(catalog, sort_keys=True, separators=(',', ':'),
                                    allow_nan=False).encode()).hexdigest()


def validate_override(config, override, catalog, refs):
    if not isinstance(override, dict) or set(override) != {'role', 'model', 'effort', 'reason'}:
        raise ValueError('override requires role, model, effort and reason')
    if not _text(override['reason']) or override['role'] not in MODE_ROLES:
        raise ValueError('override requires known role and nonempty reason')
    if not isinstance(override['model'], str) or not isinstance(override['effort'], str):
        raise ValueError('invalid override identity')
    mode = config['selected_mode']
    if not override['model'].startswith(mode + '/'):
        raise ValueError('override provider/mode mismatch')
    identity = catalog_identity(catalog)
    selected = next((m for m in catalog if isinstance(m, dict)
                     and m.get('providerID') == mode
                     and m.get('modelID') == override['model'].split('/', 1)[1]
                     and m.get('enabled') is True and m.get('status') == 'active'
                     and m.get('type') == 'chat' and isinstance(m.get('capabilities'), dict)
                     and m['capabilities'].get('tools') is True
                     and any(isinstance(v, dict) and v.get('id') == override['effort']
                             for v in m.get('variants', []))), None)
    configured = config['mode_roles'][override['role']]
    if selected is None or configured != {'model': override['model'], 'effort': override['effort']}:
        raise ValueError('override is not a validated seat in selected mode')
    ref = refs[override['role']]
    return dict(ref, role=override['role'], model=override['model'], effort=override['effort'],
                reason=override['reason'].strip(), catalog_identity=identity,
                catalog_snapshot=selected)


def _match(path, pattern):
    # fnmatch treats slash as ordinary; use segment-wise glob with ** recursion.
    def match(parts, patterns):
        if not patterns:
            return not parts
        if patterns[0] == '**':
            return match(parts, patterns[1:]) or bool(parts) and match(parts[1:], patterns)
        return bool(parts) and fnmatch.fnmatchcase(parts[0], patterns[0]) and match(parts[1:], patterns[1:])
    return match(path.split('/'), pattern.split('/'))


def classify_task(config, files, complexity, risk, ambiguity='unknown', test_coverage='unknown',
                  failed_attempts=0, blocking_findings=0):
    if (not isinstance(files, list) or not files or any(not _path(p) for p in files) or
        len(files) != len(set(files)) or complexity not in ('simple', 'complex') or
        risk not in ('trivial', 'normal', 'high') or ambiguity not in ('low', 'medium', 'high', 'unknown') or
        test_coverage not in ('adequate', 'limited', 'poor', 'unknown') or
        any(type(n) is not int or n < 0 for n in (failed_attempts, blocking_findings))):
        raise ValueError('invalid classification inputs')
    modules = set()
    for path in files:
        parts = path.split('/')
        if len(parts) > 1 and parts[0] == 'src':
            modules.add(parts[1] if len(parts) > 2 else parts[1].removesuffix('.rs'))
    high_path = any(_match('/'.join(f.split('/')[:n]), pattern)
                    for f in files for n in range(1, len(f.split('/')) + 1)
                    for pattern in config['review']['high_risk_paths'])
    if (complexity == 'complex' or risk == 'high' or high_path or ambiguity == 'high' or
        test_coverage == 'poor' or failed_attempts or blocking_findings or len(modules) >= 3):
        return {'class': 'L', 'reason': 'high complexity, risk, uncertainty, history, or module scope'}
    if (len(files) <= config['limits']['simple_max_files'] and len(modules) <= 1 and risk == 'trivial'
        and ambiguity == 'low' and test_coverage == 'adequate'):
        return {'class': 'S', 'reason': 'small isolated low-risk task with adequate coverage'}
    return {'class': 'M', 'reason': 'ordinary scope or insufficient evidence for simple class'}


def review_policy(config, files, added, deleted, declared_risk, validated, binary=False):
    if declared_risk not in ('trivial', 'normal', 'high') or type(validated) is not bool or type(binary) is not bool:
        raise ValueError('invalid review inputs')
    if not isinstance(files, list) or any(not _path(p) for p in files) or len(set(files)) != len(files):
        raise ValueError('invalid files')
    if any(type(n) is not int or n < 0 for n in (added, deleted)):
        raise ValueError('invalid diff counts')
    review = config['review']
    high = declared_risk == 'high' or any(
        _match('/'.join(f.split('/')[:n]), p)
        for f in files for n in range(1, len(f.split('/')) + 1)
        for p in review['high_risk_paths'])
    if high:
        return {'required': True, 'risk': 'high', 'role': 'escalation', 'reason': 'high risk'}
    trivial = (declared_risk == 'trivial' and bool(files) and 0 < added + deleted <= review['trivial_max_lines']
               and len(files) <= review['trivial_max_files'] and not binary and validated and review['skip_trivial']
               and all(any(_match(f, p) for p in review['trivial_paths']) for f in files))
    return {'required': not trivial, 'risk': 'trivial' if trivial else 'normal', 'role': 'reviewer',
            'reason': 'validated trivial diff' if trivial else 'review required'}


def parse_review(text):
    try:
        value = json.loads(text, object_pairs_hook=_pairs, parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)))
    except (TypeError, json.JSONDecodeError) as exc:
        raise ValueError('invalid review JSON') from exc
    _keys(value, {'decision', 'findings'}, 'review')
    if value['decision'] not in ('approve', 'request_changes', 'block') or not isinstance(value['findings'], list):
        raise ValueError('invalid review decision or findings')
    for finding in value['findings']:
        _keys(finding, {'file', 'line', 'severity', 'message'}, 'finding')
        if not _path(finding['file']) or type(finding['line']) is not int or finding['line'] < 1 or finding['severity'] not in ('critical', 'important', 'minor') or not _text(finding['message']):
            raise ValueError('invalid finding')
        if value['decision'] == 'approve' and finding['severity'] in ('critical', 'important'):
            raise ValueError('approval contains blocking finding')
    return value
