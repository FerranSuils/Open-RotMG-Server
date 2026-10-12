#!/usr/bin/env python3
import argparse
from contextlib import closing
import datetime
import gzip
import json
import os
from pathlib import Path
import re
import sqlite3

GAME = {
    'accounts': ('id', 'guid', 'name'),
    'characters': ('id', 'account', 'stats', 'items', 'revision', 'alive'),
    'deaths': ('character', 'snapshot', 'killed_by', 'died_at'),
}
DISCORD = {'levels': ('guild_id', 'user_id', 'username', 'xp', 'last_xp_at', 'last_message_id')}
CHARACTER = {'id', 'account', 'name', 'level', 'exp', 'fame', 'hp', 'mp', 'items', 'revision',
             'alive', 'created_at', 'shots', 'hits', 'kills', 'quests', 'tiles'}


def character(value):
    if not isinstance(value, dict) or set(value) != CHARACTER:
        raise ValueError('Unsupported character payload; review the credential-free export schema')
    if not isinstance(value['name'], str) or type(value['alive']) is not bool:
        raise ValueError('Invalid character name/alive types')
    for key in CHARACTER - {'name', 'alive', 'items'}:
        if type(value[key]) is not int:
            raise ValueError('Invalid character integer: ' + key)
    if not isinstance(value['items'], list) or len(value['items']) != 12 or any(type(i) is not int for i in value['items']):
        raise ValueError('Invalid character equipment')
    return value


def snapshot(path, schema):
    with closing(sqlite3.connect(Path(path).resolve().as_uri() + '?mode=ro', uri=True)) as db, db:
        db.execute('BEGIN')
        if db.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
            raise ValueError('Source SQLite integrity failed')
        if db.execute('PRAGMA foreign_key_check').fetchall():
            raise ValueError('Source SQLite foreign keys failed')
        tables = {r[0] for r in db.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")}
        if tables != set(schema):
            raise ValueError('Unsupported database tables; update and test the export schema before backing up')
        result = {}
        for table, columns in schema.items():
            if tuple(r[1] for r in db.execute('PRAGMA table_info(' + table + ')')) != columns:
                raise ValueError('Unsupported database columns: ' + table)
            selected = tuple(c for c in columns if c != 'guid')
            rows = db.execute('SELECT ' + ','.join(selected) + ' FROM ' + table + ' ORDER BY ' + ','.join(selected[:2])).fetchall()
            result[table] = [dict(zip(selected, row)) for row in rows]
        return result


def validate(bundle):
    if set(bundle) != {'schema', 'created_at', 'revisions', 'game', 'discord'} or bundle['schema'] != 1:
        raise ValueError('Unsupported backup envelope')
    datetime.datetime.strptime(bundle['created_at'], '%Y-%m-%dT%H:%M:%SZ')
    if set(bundle['revisions']) != {'server', 'discord_bot'} or any(not re.fullmatch('[0-9a-f]{40}', v) for v in bundle['revisions'].values()):
        raise ValueError('Invalid source revisions')
    for name, schema in [('game', GAME), ('discord', DISCORD)]:
        data = bundle[name]
        if set(data) != set(schema):
            raise ValueError('Unsupported backup tables: ' + name)
        for table, columns in schema.items():
            allowed = set(columns) - {'guid'}
            if not isinstance(data[table], list) or any(not isinstance(r, dict) or set(r) != allowed for r in data[table]):
                raise ValueError('Unsupported backup fields: ' + table)
    for row in bundle['game']['characters']:
        character(json.loads(row['stats']))
        items = json.loads(row['items'])
        if not isinstance(items, list) or len(items) != 12 or any(type(i) is not int for i in items):
            raise ValueError('Invalid inventory')
    for row in bundle['game']['deaths']:
        character(json.loads(row['snapshot']))
    return bundle


def export(game, discord, output, server_revision, bot_revision):
    bundle = validate({
        'schema': 1, 'created_at': datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
        'revisions': {'server': Path(server_revision).read_text().strip(), 'discord_bot': Path(bot_revision).read_text().strip()},
        'game': snapshot(game, GAME), 'discord': snapshot(discord, DISCORD),
    })
    data = gzip.compress(json.dumps(bundle, sort_keys=True, separators=(',', ':')).encode(), mtime=0)
    output = Path(output)
    temporary = output.with_name(output.name + '.new')
    with temporary.open('xb') as file:
        file.write(data)
    temporary.replace(output)
    return bundle


def restore(archive, directory):
    bundle = validate(json.loads(gzip.decompress(Path(archive).read_bytes())))
    directory = Path(directory)
    directory.mkdir(mode=0o700)
    # NULL identities cannot authenticate; ownership must be reverified separately after a disaster.
    definitions = {
        'game': """CREATE TABLE accounts(id INTEGER PRIMARY KEY,guid TEXT UNIQUE,name TEXT NOT NULL);
CREATE TABLE characters(id INTEGER PRIMARY KEY,account INTEGER NOT NULL REFERENCES accounts(id),stats TEXT NOT NULL,items TEXT NOT NULL,revision INTEGER NOT NULL,alive INTEGER NOT NULL);
CREATE UNIQUE INDEX one_living_character ON characters(account) WHERE alive=1;
CREATE TABLE deaths(character INTEGER PRIMARY KEY REFERENCES characters(id),snapshot TEXT NOT NULL,killed_by TEXT NOT NULL,died_at INTEGER NOT NULL);""",
        'discord': """CREATE TABLE levels(guild_id TEXT NOT NULL,user_id TEXT NOT NULL,username TEXT NOT NULL,xp INTEGER NOT NULL CHECK(xp>=0 AND xp<=9007199254740991),last_xp_at INTEGER NOT NULL,last_message_id TEXT NOT NULL,PRIMARY KEY(guild_id,user_id)) STRICT;""",
    }
    for name, filename in [('game', 'game.db'), ('discord', 'levels.sqlite')]:
        with closing(sqlite3.connect(':memory:')) as db, db:
            db.execute('PRAGMA foreign_keys=ON')
            db.executescript(definitions[name])
            for table, rows in bundle[name].items():
                columns = tuple(c for c in (GAME if name == 'game' else DISCORD)[table] if c != 'guid')
                db.executemany('INSERT INTO ' + table + '(' + ','.join(columns) + ') VALUES(' + ','.join('?' for _ in columns) + ')', [tuple(row[c] for c in columns) for row in rows])
            db.commit()
            if db.execute('PRAGMA foreign_key_check').fetchall() or db.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
                raise ValueError('Restored database integrity failed')
            path = directory / filename
            with path.open('xb'):
                pass
            with closing(sqlite3.connect(path)) as target:
                db.backup(target)
    return bundle


if __name__ == '__main__':
    os.umask(0o077)
    parser = argparse.ArgumentParser(description='Credential-free persisted-state export and offline restore')
    commands = parser.add_subparsers(dest='command', required=True)
    exp = commands.add_parser('export')
    for option in ['game', 'discord', 'output', 'server-revision', 'bot-revision']:
        exp.add_argument('--' + option, required=True)
    res = commands.add_parser('restore')
    res.add_argument('--archive', required=True)
    res.add_argument('--directory', required=True)
    args = vars(parser.parse_args())
    command = args.pop('command')
    bundle = export(**args) if command == 'export' else restore(**args)
    print(json.dumps({'event': command + '_complete', 'created_at': bundle['created_at'], 'counts': {name: {table: len(rows) for table, rows in bundle[name].items()} for name in ['game', 'discord']}}))
