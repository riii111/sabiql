#!/usr/bin/env python3
"""Exercise the collector's leak path without a DB or compiled Rust binary."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class ResidualProcessTest(unittest.TestCase):
    def test_preserves_observation_and_reclaims_owned_group(self):
        with tempfile.TemporaryDirectory(prefix='cf14-collector-test-') as scratch:
            root = Path(scratch)
            binary = root/'leaking-harness'
            binary.write_text('''#!/usr/bin/env python3
import json, os, subprocess, sys, time
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'],
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
with open(os.environ['CF14_LOG'], 'a') as log:
    log.write(json.dumps(dict(kind='start', pid=child.pid, ns=time.time_ns()))+'\\n')
''')
            binary.chmod(0o700)
            output = root/'raw'
            collector = Path(__file__).with_name('stage2.py')
            result = subprocess.run([sys.executable, str(collector), '--binary', str(binary),
                                     '--output', str(output), '--observe'],
                                    capture_output=True, text=True, timeout=10, check=False)
            self.assertNotEqual(result.returncode, 0)
            observation = json.loads(next(output.glob('*-cleanup.json')).read_text())
            residual = observation['residual_pids_after_500ms']
            self.assertEqual(len(residual), 1)
            reclaim = json.loads(next(output.glob('*-reclaim.json')).read_text())
            self.assertFalse(reclaim['group_exists_after_cleanup'])
            with self.assertRaises(ProcessLookupError):
                os.kill(residual[0], 0)
            manifest = json.loads((output/'manifest.json').read_text())
            self.assertEqual(manifest['python_executable'], sys.executable)
            self.assertEqual(manifest['python_version'], sys.version)
            self.assertIn('raw retained', result.stderr)


if __name__ == '__main__':
    unittest.main()
