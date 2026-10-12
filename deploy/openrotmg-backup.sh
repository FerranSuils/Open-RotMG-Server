#!/bin/sh
set -eu
export GIT_TERMINAL_PROMPT=0
export GIT_CONFIG_COUNT=2
export GIT_CONFIG_KEY_0=credential.helper
export GIT_CONFIG_VALUE_0=/opt/openrotmg-backup/credential.py
export GIT_CONFIG_KEY_1=credential.useHttpPath
export GIT_CONFIG_VALUE_1=true
cd /var/lib/openrotmg-backup/repo
/usr/bin/python3 /opt/openrotmg-backup/backup.py export --game /var/lib/openrotmg/game.db --discord /var/lib/openrotmg-discord-bot/levels.sqlite --output state.json.gz --server-revision /opt/openrotmg/server/REVISION --bot-revision /opt/openrotmg-discord-bot/REVISION
git add state.json.gz
git commit -m "Daily persisted-state backup $(date -u +%Y-%m-%dT%H:%M:%SZ)"
git push origin main
commit=$(git rev-parse HEAD)
remote=$(git ls-remote origin refs/heads/main)
test "$commit" = "${remote%%[[:space:]]*}"
/usr/bin/python3 - "$commit" <<'PY'
import datetime,json,pathlib,sys
result={'event':'backup_uploaded','commit':sys.argv[1],'uploaded_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
path=pathlib.Path('/var/lib/openrotmg-backup/last-success.json')
temporary=path.with_suffix('.new')
temporary.write_text(json.dumps(result)+'\n')
temporary.replace(path)
print(json.dumps(result))
PY
