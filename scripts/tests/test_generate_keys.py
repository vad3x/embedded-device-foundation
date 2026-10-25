import importlib.util
from pathlib import Path
import tempfile
import unittest
import stat

spec = importlib.util.spec_from_file_location("edf_generate_keys", Path(__file__).resolve().parents[1] / "generate_keys.py")
keys = importlib.util.module_from_spec(spec)
spec.loader.exec_module(keys)

class KeyTests(unittest.TestCase):
    def test_existing_key_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "key.pem"
            path.write_bytes(b"existing")
            with self.assertRaises(SystemExit): keys.main(["--key", str(path)])
            self.assertEqual(path.read_bytes(), b"existing")

    def test_key_path_is_required(self):
        with self.assertRaises(SystemExit): keys.main([])

    def test_new_private_file_has_mode_0600(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "key.pem"
            keys.write_private_key(path, b"test-only-placeholder")
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)

if __name__ == "__main__": unittest.main()
