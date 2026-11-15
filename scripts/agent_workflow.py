#!/usr/bin/env python3
"""Run the repository's trusted-request OpenCode workflow (stdlib only)."""
import argparse
import json
from pathlib import Path
import sys

# The controller itself must not dirty a pristine target before preflight.
sys.dont_write_bytecode = True

# Support both `python scripts/agent_workflow.py` and module execution.
if __package__ in (None, ''):
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from scripts.workflow import api, policy, runner


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path.cwd(), help='linked Git worktree (default: current directory)')
    parser.add_argument('--config', type=Path, help='complete trusted config override (not merged)')
    commands = parser.add_subparsers(dest='command', required=True)
    doctor = commands.add_parser('doctor', help='validate config and discover models; never generates')
    doctor.add_argument('--offline', action='store_true', help='schema validation only, no service discovery')
    run = commands.add_parser('run', help='execute one trusted request; never commits or merges')
    run.add_argument('manifest', type=Path)
    report = commands.add_parser('metrics', help='aggregate persisted run evidence, including failures')
    report.add_argument('files', type=Path, nargs='*', help='run JSON files; defaults to this worktree runs')
    merge = commands.add_parser('record-merge', help='prove exact accepted commit is included in target ref')
    merge.add_argument('run_id')
    merge.add_argument('--commit', required=True, help='full accepted single-parent commit hash')
    merge.add_argument('--target', required=True, help='target branch or full refs/... ref')
    args = parser.parse_args(argv)
    try:
        root = args.root.resolve()
        config = policy.load_config(args.config or root / '.opencode/agent-workflow.json')
        service = api.OpenCode(root, config['limits']['api_timeout_seconds'], config['limits']['max_response_bytes'])
        if args.command == 'doctor':
            result = {'status': 'ok', 'config': config, 'discovery': not args.offline}
            if not args.offline:
                result['roles'] = policy.validate_models(config, service.models())
        elif args.command == 'run':
            if args.manifest.stat().st_size > 1024 * 1024:
                raise ValueError('manifest exceeds 1 MiB limit')
            result = runner.run(root, runner.read_json(args.manifest), config, service)
        elif args.command == 'record-merge':
            result = runner.record_merge(root, args.run_id, args.commit, args.target)
        else:
            files = args.files or sorted((runner.Git(root).directory() / 'runs').glob('*.json'))
            if not files:
                raise ValueError('no persisted runs to report')
            result = runner.report(files)
        print(json.dumps(result, indent=2, sort_keys=True, allow_nan=False))
        return 1 if result.get('status') in ('failed', 'blocked', 'running') else 0
    except (OSError, ValueError, api.APIError) as exc:
        print(json.dumps({'status': 'failed', 'error': f'{type(exc).__name__}: {exc}'}), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
