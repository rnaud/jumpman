#!/usr/bin/env python3
"""Publish the saved experiment backlog using the user's authenticated GitHub CLI."""
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DRAFTS = ROOT / 'docs/experiments/issues'
REPO = 'rnaud/jumpman'


def gh(*args, body=None):
    result = subprocess.run(['gh', *args], input=body, text=True, capture_output=True, cwd=ROOT)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return result.stdout.strip()


def main():
    manifest = json.loads((DRAFTS / 'manifest.json').read_text())
    # Abort on lookup failure: do not blindly create duplicates on a rerun.
    existing = json.loads(gh('issue', 'list', '--repo', REPO, '--state', 'all',
                             '--limit', '1000', '--json', 'title,url'))
    by_title = {issue['title']: issue['url'] for issue in existing}
    for draft in manifest:
        title = draft['title']
        if title in by_title:
            print(f"Already exists: {by_title[title]}", flush=True)
            continue
        body = (DRAFTS / draft['body']).read_text().replace(
            '(../SM64-PORTING-PLAN.md)',
            f'(https://github.com/{REPO}/blob/main/docs/SM64-PORTING-PLAN.md)')
        body += f'\nFull brief: https://github.com/{REPO}/blob/main/docs/experiments/BRIEF.md\n'
        if draft != manifest[0]:
            body += f'\nShared foundation / roadmap: {by_title[manifest[0]["title"]]}\n'
        url = gh('issue', 'create', '--repo', REPO, '--title', title, '--body-file', '-', body=body)
        by_title[title] = url
        print(f'Created: {url}', flush=True)


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, OSError, ValueError) as error:
        print(f'Issue publishing stopped: {error}', file=sys.stderr)
        sys.exit(1)
