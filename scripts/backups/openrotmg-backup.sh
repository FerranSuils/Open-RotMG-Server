#!/bin/sh
set -eu
export GIT_TERMINAL_PROMPT=0
tools=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
export OPENROTMG_BACKUP_BINARY="$tools/openrotmg"
exec 9>/var/lib/openrotmg-backup/backup.lock
flock 9
export GIT_CONFIG_COUNT=3
export GIT_CONFIG_KEY_0=credential.helper
export GIT_CONFIG_VALUE_0=
export GIT_CONFIG_KEY_1=credential.helper
export GIT_CONFIG_VALUE_1=/opt/openrotmg-backup/openrotmg-backup-credential.py
export GIT_CONFIG_KEY_2=credential.useHttpPath
export GIT_CONFIG_VALUE_2=true
cd /var/lib/openrotmg-backup/repo
git pull --rebase origin main
/usr/bin/python3 "$tools/backup.py" export --game /var/lib/openrotmg/game.db --discord /var/lib/openrotmg-discord-bot/levels.sqlite --output state.json.gz --server-revision /opt/openrotmg/server/REVISION --bot-revision /opt/openrotmg-discord-bot/REVISION --validator-revision "$tools/REVISION"
git add state.json.gz
git commit -m "Daily persisted-state backup $(date -u +%Y-%m-%dT%H:%M:%SZ)"
git push origin main
commit=$(git rev-parse HEAD)
remote=$(git ls-remote origin refs/heads/main)
test "$commit" = "${remote%%[[:space:]]*}"
verified=$(mktemp -d)
trap 'rm -rf "$verified"' EXIT HUP INT TERM
git fetch origin main
test "$commit" = "$(git rev-parse FETCH_HEAD)"
git show FETCH_HEAD:state.json.gz > "$verified/state.json.gz"
cmp state.json.gz "$verified/state.json.gz"
/usr/bin/python3 "$tools/backup.py" restore --archive "$verified/state.json.gz" --directory "$verified/restored"
/usr/bin/python3 - "$commit" <<'PY'
import datetime,gzip,hashlib,json,pathlib,sys
archive=pathlib.Path('state.json.gz').read_bytes()
bundle=json.loads(gzip.decompress(archive))
result={'event':'backup_uploaded','commit':sys.argv[1],'uploaded_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'revisions':bundle['revisions'],'archive_sha256':hashlib.sha256(archive).hexdigest(),'restored':True}
path=pathlib.Path('/var/lib/openrotmg-backup/last-success.json')
temporary=path.with_suffix('.new')
temporary.write_text(json.dumps(result)+'\n')
temporary.replace(path)
print(json.dumps(result))
PY
