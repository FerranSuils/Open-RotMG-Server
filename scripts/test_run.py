import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, MagicMock

spec = importlib.util.spec_from_file_location("launcher", Path(__file__).with_name("run.py"))
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.data = b"reference-client-test"
        self.patch = patch.multiple(launcher, ROOT=self.root, LOG=self.root / ".local/server.log",
                                    SHA256=hashlib.sha256(self.data).hexdigest())
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.temp.cleanup()

    def test_optional_port_is_forwarded_with_existing_options(self):
        client = self.root / "client"
        (client / "scripts").mkdir(parents=True)
        (client / "scripts/run.py").touch()
        cases = [
            ([], ["--hosted-client"]),
            (["8080"], ["--hosted-client", "--port", "8080"]),
            (["1", "--dev-control"], ["--hosted-client", "--port", "1", "--dev-control"]),
            (["--local-client", str(client), "65535"], ["--client", str(client), "--port", "65535"]),
        ]
        for args, expected in cases:
            with self.subTest(args=args), patch("sys.argv", ["run.py", *args]), \
                    patch.object(launcher.shutil, "which", return_value="/cargo"), \
                    patch.object(launcher, "prepare_client"), patch.object(launcher, "run", return_value=0) as run:
                self.assertEqual(launcher.main(), 0)
                self.assertEqual(run.call_args.args[0], ["cargo", "run", "--locked", "--", *expected])

    def test_invalid_ports_fail_before_preparing_or_starting(self):
        for port in ["0", "-1", "65536", "abc", "8080.5"]:
            with self.subTest(port=port), patch("sys.argv", ["run.py", port]), \
                    patch.object(launcher, "prepare_client") as prepare, patch.object(launcher, "run") as run, \
                    patch("sys.stderr", new_callable=io.StringIO) as stderr:
                with self.assertRaises(SystemExit) as error:
                    launcher.main()
                self.assertEqual(error.exception.code, 2)
                self.assertIn("port must be", stderr.getvalue())
                prepare.assert_not_called()
                run.assert_not_called()

    def test_verified_download_then_verified_cache(self):
        with patch.object(launcher, "urlopen", return_value=io.BytesIO(self.data)) as download:
            launcher.prepare_client()
            download.assert_called_once_with(launcher.URL, timeout=60)
        with patch.object(launcher, "urlopen", side_effect=AssertionError("cache should be used")):
            launcher.prepare_client()

    def test_corrupt_download_and_cache_fail_without_replacement(self):
        path = self.root / ".local/hosted-client/client.swf"
        with patch.object(launcher, "urlopen", return_value=io.BytesIO(b"corrupt")):
            with self.assertRaisesRegex(RuntimeError, "Client checksum mismatch"):
                launcher.prepare_client()
        self.assertFalse(path.exists())
        path.write_bytes(b"corrupt")
        with patch.object(launcher, "urlopen", side_effect=AssertionError("no silent redownload")):
            with self.assertRaisesRegex(RuntimeError, "Client checksum mismatch"):
                launcher.prepare_client()
        self.assertEqual(path.read_bytes(), b"corrupt")

    def test_failed_download_is_logged_with_url_and_cause(self):
        with patch("sys.argv", ["run.py"]), patch.object(launcher.shutil, "which", return_value="/cargo"), \
                patch.object(launcher, "urlopen", side_effect=OSError("connection refused")):
            self.assertEqual(launcher.main(), 1)
        log = launcher.LOG.read_text()
        self.assertIn(launcher.URL, log)
        self.assertIn("connection refused", log)

    def test_backend_error_preserves_log_and_fails_launcher(self):
        process = MagicMock()
        process.__enter__.return_value = process
        process.stdout = ["Error: cannot listen on 127.0.0.1:18923\n"]
        process.wait.return_value = 1
        with patch("sys.argv", ["run.py"]), patch.object(launcher.shutil, "which", return_value="/cargo"), \
                patch.object(launcher, "prepare_client"), patch.object(launcher.subprocess, "Popen", return_value=process) as start:
            self.assertEqual(launcher.main(), 1)
        self.assertIn("--hosted-client", start.call_args.args[0])
        self.assertIn("cannot listen", launcher.LOG.read_text())
        self.assertIn("status 1", launcher.LOG.read_text())


if __name__ == "__main__":
    unittest.main()
