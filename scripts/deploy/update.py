#!/usr/bin/env python3
import ctypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.request
from contextlib import closing

REPO = 'https://github.com/OpenRotMG/Open-RotMG-Server'
LIVE = Path('/opt/openrotmg/server')
ROOT = Path('/var/lib/openrotmg-deploy')
BACKUPS = Path('/opt/openrotmg-backup')
CONFIG = Path('/etc/openrotmg/config.json')
sys.dont_write_bytecode = True
os.environ['PYTHONDONTWRITEBYTECODE'] = '1'


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def files(root):
    return {str(p.relative_to(root)): digest(p) for p in root.rglob('*') if p.is_file()}


def status(state, desired, **fields):
    value = {'state': state, 'desired': desired, 'running': (LIVE / 'REVISION').read_text().strip(),
             'checked_at': int(time.time()), **fields}
    temporary = ROOT / 'status.json.new'
    temporary.write_text(json.dumps(value) + '\n')
    temporary.replace(ROOT / 'status.json')
    print(json.dumps(value), flush=True)


def download(url, path):
    with urllib.request.urlopen(url, timeout=30) as response:
        require(response.url.startswith('https://'), 'Non-HTTPS artifact redirect')
        body = response.read(64 * 1024 * 1024 + 1)
    require(len(body) <= 64 * 1024 * 1024, 'Release exceeds 64 MiB limit')
    path.write_bytes(body)


def unpack(archive, directory, metadata):
    directory.mkdir(mode=0o755)
    directory.chmod(0o755)
    with tarfile.open(archive) as bundle:
        for member in bundle.getmembers():
            require(not Path(member.name).is_absolute() and '..' not in Path(member.name).parts
                    and member.isfile(), 'Unsafe release archive member')
        bundle.extractall(directory)
    for path in directory.rglob('*'):
        path.chmod(0o755 if path.is_dir() or path.name == 'openrotmg' or path.suffix == '.sh' else 0o644)
    require(files(directory) == metadata['manifest'], 'Release manifest mismatch')
    require(digest(directory / 'openrotmg') == metadata['binary_sha256'], 'Release binary mismatch')
    require((directory / 'REVISION').read_text().strip() == metadata['revision'], 'Release revision mismatch')


def copy_db(source, target):
    with closing(sqlite3.connect(Path(source).resolve().as_uri() + '?mode=ro', uri=True)) as src, closing(sqlite3.connect(target)) as dst:
        src.backup(dst)
    target.chmod(0o600)


def rows(path):
    with closing(sqlite3.connect(Path(path).resolve().as_uri() + '?mode=ro', uri=True)) as db:
        db.execute('BEGIN')
        require(db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok', 'SQLite integrity failed')
        require(not db.execute('PRAGMA foreign_key_check').fetchall(), 'SQLite foreign keys failed')
        tables = [r[0] for r in db.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")]
        return {table: db.execute('SELECT * FROM "' + table.replace('"', '""') + '" ORDER BY 1').fetchall() for table in tables}


def ready(port, process=None):
    deadline = time.monotonic() + 45
    while True:
        if process is not None:
            require(process.poll() is None, 'Rehearsal exited before readiness; inspect shadow.log')
        try:
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/health', timeout=3) as response:
                health = json.load(response)
            require(type(health['uptime_seconds']) is int, 'Invalid server health')
            return health
        except OSError:
            require(time.monotonic() < deadline, 'Server readiness deadline exceeded')
            time.sleep(.2)


def backup_module(stage):
    spec = importlib.util.spec_from_file_location('release_backup', stage / 'scripts/backups/backup.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def install_backups(stage):
    run('runuser', '-u', 'openrotmg', '--', sys.executable, '-B', '-W', 'error::ResourceWarning',
        '-m', 'unittest', 'discover', '-s', str(stage / 'scripts/backups'), '-p', 'test_*.py')
    for name in ['backup.py', 'test_backup.py', 'test_backup_credential.py']:
        target = BACKUPS / name
        temporary = target.with_name(name + '.new')
        temporary.write_bytes((stage / 'scripts/backups' / name).read_bytes())
        temporary.chmod(target.stat().st_mode & 0o777)
        temporary.replace(target)
    run('systemctl', 'start', 'openrotmg-backup.service')
    require(subprocess.check_output(['systemctl', 'show', 'openrotmg-backup', '-p', 'Result', '--value'], text=True).strip() == 'success', 'Independent GitHub backup failed')


def rehearse(stage, work, config):
    shadow = work / 'shadow'
    shadow.mkdir(mode=0o700)
    copy_db(Path(config['state']) / 'game.db', shadow / 'game.db')
    shutil.copyfile(Path(config['state']) / 'server.pem', shadow / 'server.pem')
    (shadow / 'server.pem').chmod(0o600)
    before = rows(shadow / 'game.db')
    cfg = {**config, 'bind': '127.0.0.1:18939', 'data': str(stage / 'data'),
           'state': str(shadow), 'shore_bots': False}
    path = work / 'shadow.json'
    path.write_text(json.dumps(cfg))
    with (work / 'shadow.log').open('w') as log:
        process = subprocess.Popen([str(stage / 'openrotmg'), '--config', str(path)],
                                   stdout=log, stderr=subprocess.STDOUT)
        try:
            health = ready(18939, process)
            require(health['players'] == health['bots'] == health['qa_actors'] == 0, 'Unexpected rehearsal actors')
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
            process.wait(timeout=20)
        require(process.returncode == 0, 'Rehearsal shutdown failed')
    after = rows(shadow / 'game.db')
    require(all(after.get(table) == data for table, data in before.items()), 'Migration changed existing records')
    module = backup_module(stage)
    archive = work / 'shadow-export.json.gz'
    bundle = module.export(shadow / 'game.db', '/var/lib/openrotmg-discord-bot/levels.sqlite', archive,
                           stage / 'REVISION', '/opt/openrotmg-discord-bot/REVISION')
    restored = work / 'restored'
    module.restore(archive, restored)
    require(module.snapshot(restored / 'game.db', module.GAME_SCHEMAS[bundle['schema']]) == bundle['game']
            and module.snapshot(restored / 'levels.sqlite', module.DISCORD) == bundle['discord'], 'Credential-free restore mismatch')


def deploy(stage, work, metadata, config):
    run('runuser', '-u', 'openrotmg', '--', 'test', '-x', str(stage / 'openrotmg'))
    install_backups(stage)
    rehearse(stage, work, config)
    require(files(stage) == metadata['manifest'], 'Rehearsal changed release files')
    run('systemctl', 'stop', 'openrotmg.service')
    require(subprocess.check_output(['systemctl', 'show', 'openrotmg', '-p', 'Result', '--value'], text=True).strip() == 'success', 'Shutdown save failed; no source exchange performed')
    frozen = work / 'cutover'
    frozen.mkdir(mode=0o700)
    copy_db(Path(config['state']) / 'game.db', frozen / 'game.db')
    shutil.copyfile(Path(config['state']) / 'server.pem', frozen / 'server.pem')
    (frozen / 'server.pem').chmod(0o600)
    libc = ctypes.CDLL(None, use_errno=True)
    require(libc.renameat2(-100, os.fsencode(LIVE), -100, os.fsencode(stage), 2) == 0,
            'Atomic source exchange failed; frozen recovery state retained')
    (work / 'previous-server-path').write_text(str(stage) + '\n')
    run('runuser', '-u', 'openrotmg', '--', 'test', '-x', str(LIVE / 'openrotmg'))
    run('systemctl', 'start', 'openrotmg.service')
    ready(int(config['bind'].rsplit(':', 1)[1]))
    require((LIVE / 'REVISION').read_text().strip() == metadata['revision'], 'Activated revision mismatch')
    require(digest(LIVE / 'openrotmg') == metadata['binary_sha256'], 'Activated binary mismatch')
    run('systemctl', 'start', 'openrotmg-backup.service')
    require(subprocess.check_output(['systemctl', 'show', 'openrotmg-backup', '-p', 'Result', '--value'], text=True).strip() == 'success', 'Post-release GitHub backup failed')


def main():
    os.umask(0o077)
    ROOT.mkdir(mode=0o700, exist_ok=True)
    desired = subprocess.check_output(['git', 'ls-remote', '--exit-code', REPO + '.git', 'refs/heads/main'], text=True, timeout=30).split()[0]
    require(re.fullmatch('[0-9a-f]{40}', desired), 'Invalid main revision')
    if (LIVE / 'REVISION').read_text().strip() == desired:
        ready(int(json.loads(CONFIG.read_text())['bind'].rsplit(':', 1)[1]))
        previous = ROOT / 'status.json'
        if previous.exists() and json.loads(previous.read_text())['state'] == 'failed':
            run('systemctl', 'start', 'openrotmg-backup.service')
            require(subprocess.check_output(['systemctl', 'show', 'openrotmg-backup', '-p', 'Result', '--value'], text=True).strip() == 'success', 'Independent backup remains failed')
        status('up_to_date', desired)
        return
    status('checking_release', desired)
    work = ROOT / (desired + '-' + str(time.time_ns()))
    work.mkdir(mode=0o700)
    try:
        url = REPO + '/releases/download/server-' + desired
        try:
            download(url + '/release.json', work / 'release.json')
        except urllib.error.HTTPError as error:
            error.close()
            if error.code != 404:
                raise
            status('waiting_for_build', desired)
            return
        metadata = json.loads((work / 'release.json').read_text())
        require(metadata['format'] == 1 and metadata['revision'] == desired, 'Artifact source mismatch')
        download(url + '/server.tar.gz', work / 'server.tar.gz')
        require(digest(work / 'server.tar.gz') == metadata['archive_sha256'], 'Artifact archive mismatch')
        stage = LIVE.with_name('server-stage-' + str(time.time_ns()))
        unpack(work / 'server.tar.gz', stage, metadata)
        status('rehearsing', desired)
        config = json.loads(CONFIG.read_text())
        deploy(stage, work, metadata, config)
        status('deployed', desired, recovery_directory=str(work))
    except Exception:
        status('failed', desired, recovery_directory=str(work))
        raise


if __name__ == '__main__':
    main()
