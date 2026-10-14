import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

spec = importlib.util.spec_from_file_location('update', Path(__file__).with_name('update.py'))
update = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update)


class UpdateTest(unittest.TestCase):
    def test_unpack_accepts_exact_files_and_rejects_hash_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = root / 'server.tar.gz'
            contents = {'openrotmg': b'binary', 'REVISION': b'a' * 40 + b'\n'}
            with tarfile.open(archive, 'w:gz') as tar:
                for name, value in contents.items():
                    info = tarfile.TarInfo(name)
                    info.size = len(value)
                    tar.addfile(info, io.BytesIO(value))
            metadata = {'revision': 'a' * 40, 'binary_sha256': hashlib.sha256(b'binary').hexdigest(),
                        'manifest': {k: hashlib.sha256(v).hexdigest() for k, v in contents.items()}}
            update.unpack(archive, root / 'valid', metadata)
            self.assertEqual((root / 'valid/openrotmg').stat().st_mode & 0o777, 0o755)
            metadata['manifest']['REVISION'] = '0' * 64
            with self.assertRaisesRegex(RuntimeError, 'manifest mismatch'):
                update.unpack(archive, root / 'invalid', metadata)

    def test_archive_traversal_and_links_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for index, (name, kind) in enumerate([('../escape', tarfile.REGTYPE), ('link', tarfile.SYMTYPE)]):
                archive = root / f'{index}.tar.gz'
                with tarfile.open(archive, 'w:gz') as tar:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    member.linkname = '/etc/passwd'
                    tar.addfile(member)
                with self.assertRaisesRegex(RuntimeError, 'Unsafe release'):
                    update.unpack(archive, root / str(index), {})

    def test_missing_release_is_visible_waiting_not_deployment(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            live = root / 'live'
            live.mkdir()
            (live / 'REVISION').write_text('b' * 40)
            pending = urllib.error.HTTPError('https://example.invalid', 404, 'not built', {}, None)
            with patch.object(update, 'ROOT', root / 'state'), patch.object(update, 'LIVE', live), \
                    patch.object(update.subprocess, 'check_output', return_value='a' * 40 + '\trefs/heads/main'), \
                    patch.object(update, 'download', side_effect=pending), patch.object(update, 'deploy') as deploy:
                update.main()
                state = json.loads((root / 'state/status.json').read_text())
                self.assertEqual((state['state'], state['running'], state['desired']), ('waiting_for_build', 'b' * 40, 'a' * 40))
                deploy.assert_not_called()

    def test_transport_failure_is_failed_and_raised(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            live = root / 'live'
            live.mkdir()
            (live / 'REVISION').write_text('b' * 40)
            with patch.object(update, 'ROOT', root / 'state'), patch.object(update, 'LIVE', live), \
                    patch.object(update.subprocess, 'check_output', return_value='a' * 40 + '\trefs/heads/main'), \
                    patch.object(update, 'download', side_effect=TimeoutError('network timeout')):
                with self.assertRaises(TimeoutError):
                    update.main()
                self.assertEqual(json.loads((root / 'state/status.json').read_text())['state'], 'failed')

    def test_up_to_date_requires_actual_health(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'REVISION').write_text('a' * 40)
            config = root / 'config.json'
            config.write_text('{"bind":"127.0.0.1:18923"}')
            with patch.object(update, 'ROOT', root / 'state'), patch.object(update, 'LIVE', root), \
                    patch.object(update, 'CONFIG', config), \
                    patch.object(update.subprocess, 'check_output', return_value='a' * 40), \
                    patch.object(update, 'ready', side_effect=RuntimeError('dead process')):
                with self.assertRaisesRegex(RuntimeError, 'dead process'):
                    update.main()
                self.assertFalse((root / 'state/status.json').exists())

    def test_save_failure_prevents_source_exchange(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with patch.object(update, 'run'), patch.object(update, 'install_backups'), \
                    patch.object(update, 'rehearse'), patch.object(update, 'files', return_value={}), \
                    patch.object(update.subprocess, 'check_output', return_value='failed'), \
                    patch.object(update, 'main_revision', return_value='a' * 40), \
                    patch.object(update.ctypes, 'CDLL') as exchange:
                with self.assertRaisesRegex(RuntimeError, 'Shutdown save failed'):
                    update.deploy(root, root, {'manifest': {}, 'revision': 'a' * 40}, {'state': str(root)})
                exchange.assert_not_called()


if __name__ == '__main__':
    unittest.main()
