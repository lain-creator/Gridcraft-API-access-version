"""Public package resources, using a synthetic non-executable binary."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile

class PackageTests(unittest.TestCase):
    def test_public_dictionary_is_shipped_without_personal_settings(self):
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            binary = temporary / "synthetic.exe"
            binary.write_bytes(b"synthetic packaging fixture")
            output = temporary / "dist"
            command = [sys.executable, str(root / "scripts/package.py"), "--platform", "win64",
                       "--binary", str(binary), "--version", "0.12.1-test.1", "--output", str(output)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            archive = output / "GridCraft-0.12.1-test.1-win64.zip"
            with zipfile.ZipFile(archive) as zipped:
                self.assertIsNone(zipped.testzip())
                names = zipped.namelist()
                public = json.loads(zipped.read(next(n for n in names if n.endswith("/pcb-glossary-public.json"))))
                self.assertEqual(len(public["items"]), 798)
                self.assertEqual(public, json.loads((root / "gridcraft/assets/pcb-glossary-public.json").read_bytes()))
                self.assertTrue(any(n.endswith("/GridCraft.exe") for n in names))
                self.assertFalse(any(n.endswith(("/pcb-glossary.json", "/ai-api.json", "/.env")) for n in names))
            self.assertEqual((output / (archive.name + ".sha256")).read_text().split()[0],
                             hashlib.sha256(archive.read_bytes()).hexdigest())
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)

if __name__ == "__main__":
    unittest.main()
