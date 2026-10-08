from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
import pre_commit_check  # noqa: E402


class PreCommitCheckTests(unittest.TestCase):
    def test_clean_paths_run_docs_validator(self) -> None:
        with patch("pre_commit_check.subprocess.run") as run:
            run.return_value.returncode = 0
            self.assertEqual(pre_commit_check.main([]), 0)

        command = run.call_args.args[0]
        self.assertEqual(command[0], sys.executable)
        self.assertEqual(Path(command[1]).name, "validate_docs.py")
        self.assertEqual(Path(command[2]), Path(pre_commit_check.__file__).resolve().parents[1])

    def test_validator_failure_rejects_commit(self) -> None:
        with patch("pre_commit_check.subprocess.run") as run:
            run.return_value.returncode = 1
            self.assertEqual(pre_commit_check.main([]), 1)

    def test_invalid_staged_file_short_circuits_docs_validator(self) -> None:
        with tempfile.NamedTemporaryFile(suffix=".grib2") as dataset:
            with patch("pre_commit_check.subprocess.run") as run:
                self.assertEqual(pre_commit_check.main([dataset.name]), 1)
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
