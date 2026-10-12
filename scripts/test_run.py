import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, MagicMock
from zipfile import ZipFile, ZipInfo

spec = importlib.util.spec_from_file_location("launcher", Path(__file__).with_name("run.py"))
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.prefix = f"Open-RotMG-Client-{launcher.REVISION}"
        self.client = self.root / '.local' / self.prefix
        self.data = self.archive({"client.swf": b"reference", "index.html": b"page", "scripts/run.py": b"prepare", "run.sh": b"#!/bin/sh\n"})
        self.patch = patch.multiple(launcher, ROOT=self.root, LOG=self.root / ".local/server.log",
                                    SHA256=hashlib.sha256(self.data).hexdigest())
        self.patch.start()

    def archive(self, files):
        stream = io.BytesIO()
        with ZipFile(stream, 'w') as archive:
            for name, content in files.items():
                info = ZipInfo(f'{self.prefix}/{name}')
                info.external_attr = (0o100755 if name == 'run.sh' else 0o100644) << 16
                archive.writestr(info, content)
        return stream.getvalue()

    def tearDown(self):
        self.patch.stop()
        self.temp.cleanup()

    def test_download_then_verified_cache(self):
        with patch.object(launcher, "urlopen", return_value=io.BytesIO(self.data)) as download:
            self.assertEqual(launcher.prepare_client(), self.client)
            download.assert_called_once_with(launcher.URL, timeout=60)
        self.assertEqual((self.client / 'run.sh').stat().st_mode & 0o777, 0o755)
        (self.client / 'run.sh').chmod(0o644)
        with patch.object(launcher, "urlopen", side_effect=AssertionError("no silent redownload")):
            self.assertEqual(launcher.prepare_client(), self.client)
        self.assertEqual((self.client / 'index.html').read_bytes(), b'page')
        self.assertEqual((self.client / 'run.sh').stat().st_mode & 0o777, 0o755)

    def test_corrupt_download_and_archive_cache_fail(self):
        cache = self.root / f'.local/client-{launcher.REVISION}.zip'
        with patch.object(launcher, "urlopen", return_value=io.BytesIO(b"corrupt")):
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                launcher.prepare_client()
        self.assertFalse(cache.exists())
        cache.write_bytes(b"corrupt")
        with patch.object(launcher, "urlopen", side_effect=AssertionError("no silent redownload")):
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                launcher.prepare_client()
        self.assertEqual(cache.read_bytes(), b"corrupt")

    def test_modified_cached_assets_fail_without_replacement(self):
        with patch.object(launcher, "urlopen", return_value=io.BytesIO(self.data)):
            launcher.prepare_client()
        (self.client / 'index.html').write_text('modified')
        with self.assertRaisesRegex(RuntimeError, 'missing or modified'):
            launcher.prepare_client()
        self.assertEqual((self.client / 'index.html').read_text(), 'modified')

    def test_archive_traversal_and_symlinks_are_rejected(self):
        stream = io.BytesIO()
        with ZipFile(stream, 'w') as archive:
            link = ZipInfo(f'{self.prefix}/link')
            link.external_attr = 0o120777 << 16
            archive.writestr(link, '../../outside')
        for data in [self.archive({'../../outside': b'bad'}), stream.getvalue()]:
            with patch.object(launcher, 'SHA256', hashlib.sha256(data).hexdigest()), \
                    patch.object(launcher, 'urlopen', return_value=io.BytesIO(data)):
                with self.assertRaisesRegex(RuntimeError, 'Unsafe client archive'):
                    launcher.prepare_client()
        self.assertFalse((self.root / 'outside').exists())

    def test_ports_and_custom_client_are_forwarded(self):
        (self.client / 'scripts').mkdir(parents=True)
        (self.client / 'scripts/run.py').touch()
        cases = [(['8080'], ['--port', '8080']),
                 (['1', '--dev-control'], ['--port', '1', '--dev-control']),
                 (['--local-client', str(self.client), '65535'], ['--port', '65535'])]
        for args, expected in cases:
            with self.subTest(args=args), patch('sys.argv', ['run.py', *args]), \
                    patch.object(launcher.shutil, 'which', return_value='/cargo'), \
                    patch.object(launcher, 'prepare_client', return_value=self.client) as download, \
                    patch.object(launcher, 'run', return_value=0) as run:
                self.assertEqual(launcher.main(), 0)
                self.assertEqual(run.call_count, 2)
                self.assertEqual(run.call_args.args[0], ['cargo', 'run', '--locked', '--', '--client', str(self.client), *expected])
                if '--local-client' in args:
                    download.assert_not_called()

    def test_missing_port_fails_before_setup(self):
        for args in [[], ['--dev-control'], ['--local-client', str(self.client)]]:
            with self.subTest(args=args), patch('sys.argv', ['run.py', *args]), \
                    patch.object(launcher.shutil, 'which') as cargo, \
                    patch.object(launcher, 'prepare_client') as prepare, \
                    patch('sys.stderr', new_callable=io.StringIO) as stderr:
                with self.assertRaises(SystemExit) as error:
                    launcher.main()
                self.assertEqual(error.exception.code, 2)
                self.assertIn('usage: run.sh', stderr.getvalue())
                self.assertIn('required: port', stderr.getvalue())
                cargo.assert_not_called()
                prepare.assert_not_called()
                self.assertFalse(launcher.LOG.exists())

    def test_invalid_ports_fail_before_download(self):
        for port in ['0', '-1', '65536', 'abc', '8080.5']:
            with self.subTest(port=port), patch('sys.argv', ['run.py', port]), \
                    patch.object(launcher, 'prepare_client') as prepare, patch('sys.stderr', new_callable=io.StringIO) as stderr:
                with self.assertRaises(SystemExit) as error:
                    launcher.main()
                self.assertEqual(error.exception.code, 2)
                self.assertIn('port must be', stderr.getvalue())
                prepare.assert_not_called()

    def test_failed_download_logs_url_and_cause(self):
        with patch('sys.argv', ['run.py', '8080']), patch.object(launcher.shutil, 'which', return_value='/cargo'), \
                patch.object(launcher, 'urlopen', side_effect=OSError('connection refused')):
            self.assertEqual(launcher.main(), 1)
        self.assertIn(launcher.URL, launcher.LOG.read_text())
        self.assertIn('connection refused', launcher.LOG.read_text())

    def test_failed_preparation_does_not_start_backend(self):
        (self.client / 'scripts').mkdir(parents=True)
        (self.client / 'scripts/run.py').touch()
        with patch('sys.argv', ['run.py', '8080']), patch.object(launcher.shutil, 'which', return_value='/cargo'), \
                patch.object(launcher, 'prepare_client', return_value=self.client), patch.object(launcher, 'run', return_value=37) as run:
            self.assertEqual(launcher.main(), 1)
            self.assertEqual(run.call_count, 1)
        self.assertIn('status 37', launcher.LOG.read_text())

    def test_backend_failure_is_printed_and_logged(self):
        (self.client / 'scripts').mkdir(parents=True)
        (self.client / 'scripts/run.py').touch()
        prepare = MagicMock()
        prepare.__enter__.return_value = prepare
        prepare.stdout = []
        prepare.wait.return_value = 0
        backend = MagicMock()
        backend.__enter__.return_value = backend
        backend.stdout = ['Error: cannot listen on 127.0.0.1:18923\n']
        backend.wait.return_value = 1
        with patch('sys.argv', ['run.py', '8080']), patch.object(launcher.shutil, 'which', return_value='/cargo'), \
                patch.object(launcher, 'prepare_client', return_value=self.client), \
                patch.object(launcher.subprocess, 'Popen', side_effect=[prepare, backend]):
            self.assertEqual(launcher.main(), 1)
        self.assertIn('cannot listen', launcher.LOG.read_text())
        self.assertIn('status 1', launcher.LOG.read_text())


if __name__ == '__main__':
    unittest.main()
