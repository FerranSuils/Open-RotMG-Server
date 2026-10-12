#!/usr/bin/python3
import os
from pathlib import Path
import sys

operation = sys.argv[1]
request = dict(line.rstrip('\n').split('=', 1) for line in sys.stdin if '=' in line)
if operation not in {'get', 'store', 'erase'}:
    raise ValueError('Unsupported Git credential operation')
# Git must not persist a second credential copy outside systemd's protected store.
if operation == 'get':
    if (request.get('protocol'), request.get('host'), request.get('path')) != ('https', 'github.com', 'OpenRotMG/Open-RotMG-Backups.git'):
        raise ValueError('Credential request is not for the exact backup repository')
    token = (Path(os.environ['CREDENTIALS_DIRECTORY']) / 'github-token').read_text().strip()
    if not token.startswith('github_pat_') or any(c.isspace() for c in token):
        raise ValueError('Expected a fine-grained repository-only GitHub token')
    print('username=dginovker')
    print('password=' + token)
