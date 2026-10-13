from contextlib import closing
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


if __name__ == '__main__':
    unittest.main()
