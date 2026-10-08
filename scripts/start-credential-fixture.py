#!/usr/bin/env python3
"""Start an isolated authenticated Quipu server for the clean-machine CI proof.

# arming: ci clean-machine-install; creates a fixture credential, never rotates one.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import urllib.error
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--home', type=Path, required=True)
parser.add_argument('--root', type=Path, required=True)
parser.add_argument('--bind', default='127.0.0.1:3030')
args = parser.parse_args()
args.home = args.home.resolve()
args.root = args.root.resolve()
credential = args.home / '.config/quipu/token'
if credential.exists():
    raise SystemExit('fixture refuses an existing credential; use a fresh isolated HOME')
args.root.mkdir(parents=True, mode=0o700, exist_ok=True)
config_dir = args.root / '.bobbin'
config_dir.mkdir(mode=0o700)
token = secrets.token_hex(32)
credential.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
credential.parent.chmod(0o700)
fd = os.open(credential, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
with os.fdopen(fd, 'w') as out:
    out.write(token + '\n')
config = config_dir / 'config.toml'
fd = os.open(config, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
with os.fdopen(fd, 'w') as out:
    out.write('[quipu.server]\nauth_token = ' + json.dumps(token) + '\n')
endpoint = 'http://' + args.bind
child_env = os.environ.copy()
child_env['HOME'] = str(args.home)
child_env['XDG_CONFIG_HOME'] = str(args.home / '.config')
for name in ['QUIPU_AUTH_TOKEN', 'QUIPU_AUTH_TOKEN_FILE', 'QUIPU_SERVER']:
    child_env.pop(name, None)
with (args.root / 'server.log').open('w') as log:
    process = subprocess.Popen(
        ['quipu-server', '--db', str(args.root / 'fixture.db'), '--bind', args.bind],
        cwd=args.root, env=child_env, start_new_session=True, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
    )
(args.root / 'server.pid').write_text(str(process.pid))
try:
    for _ in range(100):
        if process.poll() is not None:
            raise RuntimeError('fixture server exited; inspect private server.log')
        try:
            with urllib.request.urlopen(endpoint + '/health', timeout=1) as response:
                if response.status == 200:
                    break
        except (OSError, urllib.error.URLError):
            time.sleep(0.1)
    else:
        raise RuntimeError('fixture server did not become healthy')
    for authenticated, expected in [(False, 401), (True, 200)]:
        headers = {'Content-Type': 'application/json', 'X-Quipu-Client': 'caboodle-verify' if authenticated else 'auth-negative-probe'}
        if authenticated:
            headers['Authorization'] = 'Bearer ' + token
        request = urllib.request.Request(endpoint + '/shapes', data=b'{"action":"list"}', headers=headers)
        try:
            with urllib.request.urlopen(request, timeout=5) as response:
                status = response.status
        except urllib.error.HTTPError as error:
            status = error.code
            error.close()
        if status != expected:
            raise RuntimeError(f'fixture auth control {authenticated} returned HTTP {status}, expected {expected}')
except Exception:
    process.terminate()
    process.wait(timeout=5)
    raise
print('isolated credential fixture: unauthenticated401/authenticated200')
