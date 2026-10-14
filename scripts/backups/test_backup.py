from contextlib import closing
from copy import deepcopy
import gzip
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
import backup


class Backups(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.game = self.root / 'game.db'
        self.discord = self.root / 'levels.sqlite'
        self.output = self.root / 'state.json.gz'
        self.revision = self.root / 'revision'
        self.revision.write_text('a' * 40)
        self.stats = {key: 0 for key in backup.CHARACTER}
        self.stats.update(id=1, account=1, name='Guest1', alive=False, items=[2711, 2606] + [-1] * 10)
        with closing(sqlite3.connect(self.game)) as db, db:
            db.executescript('''PRAGMA foreign_keys=ON;
CREATE TABLE accounts(id INTEGER PRIMARY KEY,guid TEXT NOT NULL UNIQUE,name TEXT NOT NULL);
CREATE TABLE characters(id INTEGER PRIMARY KEY,account INTEGER NOT NULL REFERENCES accounts(id),stats TEXT NOT NULL,items TEXT NOT NULL,revision INTEGER NOT NULL,alive INTEGER NOT NULL);
CREATE TABLE deaths(character INTEGER PRIMARY KEY REFERENCES characters(id),snapshot TEXT NOT NULL,killed_by TEXT NOT NULL,died_at INTEGER NOT NULL);''')
            db.execute('INSERT INTO accounts VALUES(1,?,?)', ('GUEST_LOGIN_MUST_NEVER_BE_UPLOADED', 'Guest1'))
            db.execute('INSERT INTO characters VALUES(1,1,?,?,2,0)', (json.dumps(self.stats), json.dumps([-1] * 12)))
            db.execute('INSERT INTO deaths VALUES(1,?,?,123)', (json.dumps(self.stats), 'Tutorial Gun'))
        with closing(sqlite3.connect(self.discord)) as db, db:
            db.execute('CREATE TABLE levels(guild_id TEXT,user_id TEXT,username TEXT,xp INTEGER,last_xp_at INTEGER,last_message_id TEXT,PRIMARY KEY(guild_id,user_id))')
            db.execute("INSERT INTO levels VALUES('1','2','someone',210,5000,'9')")

    def add_hiscores(self):
        self.add_shore_state()
        with closing(sqlite3.connect(self.game)) as db, db:
            db.executescript('''
CREATE TABLE competitors(account INTEGER PRIMARY KEY REFERENCES accounts(id));
CREATE TABLE collections(account INTEGER NOT NULL REFERENCES accounts(id),item INTEGER NOT NULL,found_at INTEGER NOT NULL,PRIMARY KEY(account,item));
CREATE TABLE discord_links(account INTEGER PRIMARY KEY REFERENCES accounts(id),discord_id TEXT NOT NULL UNIQUE);
CREATE TABLE discord_codes(account INTEGER PRIMARY KEY REFERENCES accounts(id),hash TEXT NOT NULL UNIQUE,issued_at INTEGER NOT NULL,expires_at INTEGER NOT NULL);
CREATE TABLE credentials(account INTEGER PRIMARY KEY REFERENCES accounts(id),email TEXT NOT NULL,password_hash TEXT NOT NULL);
INSERT INTO credentials VALUES(1,'NEVER_EXPORT_EMAIL','NEVER_EXPORT_PASSWORD_HASH');
INSERT INTO competitors VALUES(1);
INSERT INTO collections VALUES(1,2594,123);
INSERT INTO discord_links VALUES(1,'1558662335076376578');
INSERT INTO discord_codes VALUES(1,'NEVER_EXPORT_LINK_CODE_HASH',100,400);''')

    def test_hiscores_roundtrip_keeps_records_and_ids_but_no_authentication(self):
        self.add_hiscores()
        before = self.export()
        self.assertEqual(before['schema'], 4)
        raw = gzip.decompress(self.output.read_bytes())
        self.assertNotIn(b'NEVER_EXPORT_LINK_CODE_HASH', raw)
        self.assertNotIn(b'discord_codes', raw)
        restored = self.root / 'hiscores-restored'
        backup.restore(self.output, restored)
        self.assertEqual(backup.snapshot(restored / 'game.db', backup.HISCORES_GAME), before['game'])
        with closing(sqlite3.connect(restored / 'game.db')) as db:
            self.assertTrue(all(r[0] is None for r in db.execute('SELECT guid FROM accounts')))
            tables = {r[0] for r in db.execute("SELECT name FROM sqlite_schema WHERE type='table'")}
            self.assertNotIn('credentials', tables)
            self.assertNotIn('discord_codes', tables)

    def test_hiscores_unknown_columns_and_nested_secrets_fail_closed(self):
        self.add_hiscores()
        before = self.export()
        for table in ['collections', 'competitors', 'discord_links']:
            malformed = deepcopy(before)
            malformed['game'][table][0]['token'] = 'SECRET'
            with self.subTest(table=table), self.assertRaisesRegex(ValueError, 'fields'):
                backup.validate(malformed)
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('ALTER TABLE discord_codes ADD COLUMN token TEXT')
        with self.assertRaisesRegex(ValueError, 'columns'):
            self.export()

    def test_hiscores_invalid_owner_duplicate_ids_and_items_rejected(self):
        self.add_hiscores()
        before = self.export()
        for field, value in [('item', -1), ('found_at', 0), ('account', 999999)]:
            malformed = deepcopy(before)
            malformed['game']['collections'][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                backup.validate(malformed)
        malformed = deepcopy(before)
        malformed['game']['discord_links'].append(malformed['game']['discord_links'][0])
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            backup.validate(malformed)

    def tearDown(self):
        self.temp.cleanup()

    def export(self):
        return backup.export(self.game, self.discord, self.output, self.revision, self.revision)

    def test_export_has_no_credentials_and_preserves_death_equipment(self):
        bundle = self.export()
        raw = gzip.decompress(self.output.read_bytes())
        self.assertNotIn(b'GUEST_LOGIN_MUST_NEVER_BE_UPLOADED', raw)
        self.assertNotIn(b'"guid"', raw)
        self.assertEqual(bundle['game']['accounts'], [{'id': 1, 'name': 'Guest1'}])
        self.assertEqual(json.loads(bundle['game']['characters'][0]['items']), [-1] * 12)
        self.assertEqual(json.loads(bundle['game']['deaths'][0]['snapshot'])['items'][:2], [2711, 2606])

    def test_restore_preserves_every_exported_field_and_disables_guest_login(self):
        before = self.export()
        restored = self.root / 'restored'
        backup.restore(self.output, restored)
        self.assertEqual(backup.snapshot(restored / 'game.db', backup.LEGACY_GAME), before['game'])
        self.assertEqual(backup.snapshot(restored / 'levels.sqlite', backup.DISCORD), before['discord'])
        with closing(sqlite3.connect(restored / 'game.db')) as db, db:
            self.assertEqual(db.execute('SELECT guid FROM accounts').fetchall(), [(None,)])
            self.assertEqual(db.execute('PRAGMA integrity_check').fetchone()[0], 'ok')
        with self.assertRaises(FileExistsError):
            backup.restore(self.output, restored)

    def test_new_password_column_fails_closed(self):
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('ALTER TABLE accounts ADD COLUMN password_hash TEXT')
            db.execute("UPDATE accounts SET password_hash='NOT_FOR_BACKUPS'")
        with self.assertRaisesRegex(ValueError, 'columns'):
            self.export()
        self.assertFalse(self.output.exists())

    def test_unknown_nested_field_fails_closed(self):
        stats = dict(self.stats, password_hash='NOT_FOR_BACKUPS')
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('UPDATE characters SET stats=?', (json.dumps(stats),))
        with self.assertRaisesRegex(ValueError, 'character payload'):
            self.export()
        self.assertFalse(self.output.exists())

    def test_unknown_table_fails_closed(self):
        with closing(sqlite3.connect(self.discord)) as db, db:
            db.execute('CREATE TABLE sessions(token TEXT)')
        with self.assertRaisesRegex(ValueError, 'tables'):
            self.export()

    def test_missing_database_is_not_created(self):
        missing = self.root / 'missing.db'
        with self.assertRaises(sqlite3.OperationalError):
            backup.snapshot(missing, backup.GAME)
        self.assertFalse(missing.exists())

    def test_credential_field_in_downloaded_backup_rejected(self):
        bundle = self.export()
        bundle['game']['accounts'][0]['guid'] = 'BAD'
        self.output.write_bytes(gzip.compress(json.dumps(bundle).encode()))
        with self.assertRaisesRegex(ValueError, 'fields'):
            backup.restore(self.output, self.root / 'bad-restore')
        self.assertFalse((self.root / 'bad-restore').exists())

    def test_wal_source_and_new_gameplay_tables_are_not_silently_omitted(self):
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('PRAGMA journal_mode=WAL')
            db.execute("UPDATE accounts SET name='SavedName'")
            db.commit()
            self.assertEqual(self.export()['game']['accounts'][0]['name'], 'SavedName')
            db.execute('CREATE TABLE vaults(account INTEGER,items TEXT)')
            db.commit()
            with self.assertRaisesRegex(ValueError, 'columns'):
                self.export()

    def add_latest_state(self):
        stats = dict(self.stats, schema=1, **{'class': 782}, base=[100, 100, 12, 1, 10, 15, 12, 12], pirate_caves=2)
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('CREATE TABLE vaults(account INTEGER PRIMARY KEY REFERENCES accounts(id),items TEXT NOT NULL CHECK(json_array_length(items)=8),revision INTEGER NOT NULL DEFAULT 0)')
            db.execute('INSERT INTO vaults VALUES(1,?,3)', (json.dumps([2639] + [-1] * 7),))
            db.execute('UPDATE characters SET stats=?', (json.dumps(stats),))

    def test_latest_export_restore_preserves_vault_and_progression_with_legacy_death(self):
        self.add_latest_state()
        before = self.export()
        self.assertEqual(before['schema'], 2)
        restored = self.root / 'latest-restored'
        backup.restore(self.output, restored)
        self.assertEqual(backup.snapshot(restored / 'game.db', backup.GAME), before['game'])
        self.assertEqual(backup.snapshot(restored / 'levels.sqlite', backup.DISCORD), before['discord'])
        with closing(sqlite3.connect(restored / 'game.db')) as db:
            self.assertEqual(db.execute('SELECT guid FROM accounts').fetchall(), [(None,)])
        raw = gzip.decompress(self.output.read_bytes())
        self.assertNotIn(b'GUEST_LOGIN_MUST_NEVER_BE_UPLOADED', raw)
        self.assertNotIn(b'"guid"', raw)

    def test_registered_credentials_are_never_selected_exported_or_restored(self):
        self.add_latest_state()
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('CREATE TABLE credentials(account INTEGER PRIMARY KEY REFERENCES accounts(id),email TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL)')
            db.execute("INSERT INTO credentials VALUES(1,'PRIVATE_EMAIL@example.org','PRIVATE_PASSWORD_VERIFIER')")
        bundle = self.export()
        raw = gzip.decompress(self.output.read_bytes())
        for secret in [b'PRIVATE_EMAIL', b'PRIVATE_PASSWORD_VERIFIER', b'password_hash', b'GUEST_LOGIN_MUST_NEVER_BE_UPLOADED']:
            self.assertNotIn(secret, raw)
        self.assertEqual(bundle['schema'], 2)
        self.assertNotIn('credentials', bundle['game'])
        restored = self.root / 'registered-restore'
        backup.restore(self.output, restored)
        with closing(sqlite3.connect(restored / 'game.db')) as db:
            self.assertEqual(db.execute('SELECT guid FROM accounts').fetchall(), [(None,)])
        self.assertEqual(backup.snapshot(restored / 'game.db', backup.GAME), bundle['game'])
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('ALTER TABLE credentials ADD COLUMN recovery_token TEXT')
        with self.assertRaisesRegex(ValueError, 'columns'):
            self.export()

    def test_latest_nested_credentials_and_invalid_stats_fail_closed(self):
        self.add_latest_state()
        with closing(sqlite3.connect(self.game)) as db, db:
            original = json.loads(db.execute('SELECT stats FROM characters').fetchone()[0])
            for change in [{'password_hash': 'NEVER_EXPORT'}, {'base': [0] * 7}, {'pirate_caves': True}, {'schema': 2}]:
                db.execute('UPDATE characters SET stats=?', (json.dumps(dict(original, **change)),))
                db.commit()
                with self.subTest(change=change), self.assertRaises(ValueError):
                    self.export()
                self.assertFalse(self.output.exists())

    def test_vault_invalid_inventory_and_credential_column_fail_closed(self):
        self.add_latest_state()
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('UPDATE vaults SET items=?', (json.dumps([True] + [-1] * 7),))
            db.commit()
            with self.assertRaisesRegex(ValueError, 'vault inventory'):
                self.export()
            db.execute('ALTER TABLE vaults ADD COLUMN token TEXT')
            db.commit()
            with self.assertRaisesRegex(ValueError, 'columns'):
                self.export()
        self.assertFalse(self.output.exists())

    def test_backup_version_must_match_exact_table_set(self):
        self.add_latest_state()
        bundle = self.export()
        bundle['schema'] = 1
        with self.assertRaisesRegex(ValueError, 'tables'):
            backup.validate(bundle)

    def add_shore_state(self):
        self.add_latest_state()
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('CREATE TABLE shore_bots(name TEXT PRIMARY KEY,account INTEGER NOT NULL UNIQUE REFERENCES accounts(id))')
            for account, name in enumerate(['Cautious', 'Confident', 'Greedy'], 2):
                db.execute('INSERT INTO accounts VALUES(?,?,?)', (account, f'BOT_LOGIN_MUST_NEVER_BE_UPLOADED_{account}', name))
                db.execute('INSERT INTO shore_bots VALUES(?,?)', (name, account))
                stats = dict(self.stats, id=account, account=account, name=name, alive=account != 2,
                             schema=1, **{'class': 782}, base=[100, 100, 12, 0, 10, 15, 12, 12], pirate_caves=0)
                db.execute('INSERT INTO characters VALUES(?,?,?,?,1,?)',
                           (account, account, json.dumps(stats), json.dumps(stats['items']), int(stats['alive'])))
                if not stats['alive']:
                    db.execute('INSERT INTO deaths VALUES(?,?,?,456)', (account, json.dumps(stats), 'Shore enemy'))

    def test_shore_export_restore_preserves_registry_and_dead_characters_without_credentials(self):
        self.add_shore_state()
        originals = {path: path.read_bytes() for path in [self.game, self.discord]}
        before = self.export()
        self.assertEqual(before['schema'], 3)
        self.assertEqual(before['game']['shore_bots'], [{'name': name, 'account': account}
                         for name, account in [('Cautious', 2), ('Confident', 3), ('Greedy', 4)]])
        raw = gzip.decompress(self.output.read_bytes())
        for credential in [b'GUEST_LOGIN_MUST_NEVER_BE_UPLOADED', b'BOT_LOGIN_MUST_NEVER_BE_UPLOADED', b'"guid"']:
            self.assertNotIn(credential, raw)
        self.assertEqual({path: path.read_bytes() for path in originals}, originals)
        restored = self.root / 'shore-restored'
        backup.restore(self.output, restored)
        self.assertEqual(backup.snapshot(restored / 'game.db', backup.SHORE_GAME), before['game'])
        self.assertEqual(backup.snapshot(restored / 'levels.sqlite', backup.DISCORD), before['discord'])
        with closing(sqlite3.connect(restored / 'game.db')) as db:
            self.assertEqual(db.execute('SELECT guid FROM accounts ORDER BY id').fetchall(), [(None,)] * 4)
            self.assertEqual(db.execute('PRAGMA foreign_key_check').fetchall(), [])
            self.assertEqual(db.execute('PRAGMA integrity_check').fetchone()[0], 'ok')
            db.execute('PRAGMA foreign_keys=ON')
            with self.assertRaises(sqlite3.IntegrityError):
                db.execute("INSERT INTO shore_bots VALUES('Unknown',999)")
            with self.assertRaises(sqlite3.IntegrityError):
                db.execute("INSERT INTO shore_bots VALUES('DuplicateAccount',2)")

    def test_empty_disabled_shore_registry_still_exports_schema_three(self):
        self.add_latest_state()
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('CREATE TABLE shore_bots(name TEXT PRIMARY KEY,account INTEGER NOT NULL UNIQUE REFERENCES accounts(id))')
        before = self.export()
        self.assertEqual(before['schema'], 3)
        self.assertEqual(before['game']['shore_bots'], [])
        restored = self.root / 'disabled-restored'
        backup.restore(self.output, restored)
        self.assertEqual(backup.snapshot(restored / 'game.db', backup.SHORE_GAME), before['game'])

    def test_shore_schema_version_must_match_exact_tables(self):
        self.add_shore_state()
        before = self.export()
        for version in [1, 2]:
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, 'tables'):
                backup.validate(dict(before, schema=version))
        for table in ['shore_bots', 'vaults']:
            malformed = deepcopy(before)
            del malformed['game'][table]
            with self.subTest(missing=table), self.assertRaisesRegex(ValueError, 'tables'):
                backup.validate(malformed)
        for version in [5, True, '3']:
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, 'envelope'):
                backup.validate(dict(before, schema=version))

    def test_shore_credential_column_and_unknown_table_fail_before_export(self):
        self.add_shore_state()
        with closing(sqlite3.connect(self.game)) as db, db:
            db.execute('ALTER TABLE shore_bots ADD COLUMN token TEXT')
            db.commit()
            with self.assertRaisesRegex(ValueError, 'columns'):
                self.export()
            self.assertFalse(self.output.exists())
            db.execute('ALTER TABLE shore_bots DROP COLUMN token')
            db.execute('CREATE TABLE bot_memory(password_hash TEXT)')
            db.commit()
            with self.assertRaisesRegex(ValueError, 'tables'):
                self.export()
            self.assertFalse(self.output.exists())

    def test_shore_downloaded_credentials_are_rejected_before_restore(self):
        self.add_shore_state()
        before = self.export()
        for field in ['guid', 'token', 'password_hash', 'memory']:
            malformed = deepcopy(before)
            malformed['game']['shore_bots'][0][field] = 'MUST_NEVER_BE_EXPORTED'
            self.output.write_bytes(gzip.compress(json.dumps(malformed).encode()))
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'fields'):
                backup.restore(self.output, self.root / 'bad-shore-restore')
            self.assertFalse((self.root / 'bad-shore-restore').exists())

    def test_shore_invalid_identity_and_duplicate_bindings_are_rejected(self):
        self.add_shore_state()
        before = self.export()
        for change in [{'name': ''}, {'name': 1}, {'account': True}, {'account': '2'}, {'account': 0}, {'account': 999}]:
            malformed = deepcopy(before)
            malformed['game']['shore_bots'][0].update(change)
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, 'shore bot'):
                backup.validate(malformed)
        for field in ['name', 'account']:
            malformed = deepcopy(before)
            malformed['game']['shore_bots'][1][field] = malformed['game']['shore_bots'][0][field]
            with self.subTest(duplicate=field), self.assertRaisesRegex(ValueError, 'Duplicate shore bot'):
                backup.validate(malformed)


if __name__ == '__main__':
    unittest.main()
