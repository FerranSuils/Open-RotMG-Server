import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class Credential(unittest.TestCase):
    def run_helper(self, host='github.com', path='OpenRotMG/Open-RotMG-Backups.git', token='github_pat_test', operation='get'):
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, 'github-token').write_text(token)
            return subprocess.run([sys.executable, str(Path(__file__).with_name('openrotmg-backup-credential.py')), operation],
                                  input=f'protocol=https\nhost={host}\npath={path}\n\n', text=True, capture_output=True,
                                  env=dict(os.environ, CREDENTIALS_DIRECTORY=directory))

    def test_exact_repository_credential(self):
        result = self.run_helper()
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, 'username=dginovker\npassword=github_pat_test\n')

    def test_other_repository_or_host_gets_no_credential(self):
        for args in [{'host': 'other.example'}, {'path': 'OpenRotMG/Open-RotMG-Server.git'}]:
            result = self.run_helper(**args)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, '')

    def test_broad_token_rejected_and_git_cache_not_used(self):
        result = self.run_helper(token='ghp_not_a_real_token')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, '')
        for operation in ['store', 'erase']:
            result = self.run_helper(operation=operation)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, '')


if __name__ == '__main__':
    unittest.main()
