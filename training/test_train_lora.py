"""Standard-library checks for the optional offline training preflight."""
from hashlib import sha256
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import train_lora


class TrainingPreflightTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="fabric-training-")
        self.root = Path(self.temporary.name)
        self.dataset = self.root / "data.jsonl"
        self.rows = [{"prompt": f"Task {index}", "response": f"Answer {index}",
                      "source": "operator-reviewed", "license": "CC0-1.0"}
                     for index in range(4)]
        self.dataset.write_text("\n".join(json.dumps(row) for row in self.rows), encoding="utf-8")
        self.manifest = self.root / "splits.json"
        splits = {}
        for name in ("train", "validation", "test"):
            split = self.dataset if name == "train" else self.root / f"{name}.jsonl"
            if name != "train":
                split.write_text("\n".join(json.dumps(dict(row, prompt=f"{name} {index}"))
                                          for index, row in enumerate(self.rows)), encoding="utf-8")
            splits[name] = {"file": split.name, "sha256": sha256(split.read_bytes()).hexdigest()}
        self.review = {"format_version": 1, "source": "reviewed fixture", "license": "CC0-1.0", "splits": splits}
        self.manifest.write_text(json.dumps(self.review), encoding="utf-8")
        self.base = self.root / "base"
        self.base.mkdir()

    def tearDown(self):
        self.temporary.cleanup()

    def test_snapshot_hash_is_stable_after_source_changes(self):
        expected = sha256(self.dataset.read_bytes()).hexdigest()
        rows, digest = train_lora.load_data(self.dataset)
        self.dataset.write_text("changed", encoding="utf-8")
        self.assertEqual(digest, expected)
        self.assertEqual(rows, self.rows)

    def test_invalid_and_conflicting_rows_fail(self):
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            train_lora.validate_rows('{"prompt":"first","prompt":"second"}')
        for modification in ({"license": "x\nunsafe"}, {"prompt": "x" * 8001},
                             {"source": ""}, {"extra": "ignored"}):
            rows = [dict(row) for row in self.rows]
            rows[0].update(modification)
            with self.assertRaises(ValueError):
                train_lora.validate_rows("\n".join(json.dumps(row) for row in rows))
        conflicting = dict(self.rows[0], prompt=" Task   0 ", response="different")
        with self.assertRaisesRegex(ValueError, "Conflicting"):
            train_lora.validate_rows("\n".join(json.dumps(row) for row in self.rows + [conflicting]))
        identical = dict(self.rows[0], prompt=" Task   0 ")
        deduplicated = train_lora.validate_rows("\n".join(json.dumps(row) for row in self.rows + [identical]))
        self.assertEqual(len(deduplicated), 4)

    def test_symlinks_and_oversized_files_fail(self):
        link = self.root / "linked.jsonl"
        link.symlink_to(self.dataset)
        with self.assertRaises(OSError):
            train_lora.load_data(link)
        with self.dataset.open("wb") as stream:
            stream.truncate(8_000_001)
        with self.assertRaises(ValueError):
            train_lora.load_data(self.dataset)

    def test_no_supervision_pass_fails_instead_of_looping(self):
        with self.assertRaisesRegex(ValueError, "No supervised tokens"):
            train_lora.require_progress(0, 0)
        train_lora.require_progress(0, 1)

    def test_adapter_inventory_is_hashed_and_rejects_unsafe_outputs(self):
        output = self.root / "bundle"
        output.mkdir()
        weights = output / "adapter_model.safetensors"
        weights.write_bytes(b"test inventory only, not valid weights")
        report = train_lora.describe_artifacts(output)
        self.assertEqual(report[0]["sha256"], sha256(weights.read_bytes()).hexdigest())
        weights.unlink()
        weights.symlink_to(self.dataset)
        with self.assertRaises(OSError):
            train_lora.describe_artifacts(output)
        weights.unlink()
        weights.write_bytes(b"test")
        (output / "unsafe.bin").write_bytes(b"pickle")
        with self.assertRaises(ValueError):
            train_lora.describe_artifacts(output)

    def test_split_manifest_pins_hashes_and_returns_only_training_rows(self):
        rows, review = train_lora.load_split_manifest(self.manifest, self.dataset)
        self.assertEqual(rows, self.rows)
        self.assertTrue(review["prompt_disjoint"])
        self.assertFalse(review["semantic_independence_verified"])
        self.assertEqual(review["manifest_sha256"], sha256(self.manifest.read_bytes()).hexdigest())
        self.assertEqual(set(review["splits"]), {"train", "validation", "test"})
        self.assertNotIn("response", json.dumps(review))
        test_split = self.root / "test.jsonl"
        test_split.write_text(test_split.read_text(encoding="utf-8") + "\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            train_lora.load_split_manifest(self.manifest, self.dataset)

    def test_split_overlap_and_unsafe_manifest_entries_fail(self):
        validation = self.root / "validation.jsonl"
        rows = [dict(row, prompt=f"validation {index}") for index, row in enumerate(self.rows)]
        rows[0]["prompt"] = " Task   0 "
        validation.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")
        self.review["splits"]["validation"]["sha256"] = sha256(validation.read_bytes()).hexdigest()
        self.manifest.write_text(json.dumps(self.review), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "overlap"):
            train_lora.load_split_manifest(self.manifest, self.dataset)
        for filename in ("../data.jsonl", "/tmp/data.jsonl", ".", "..", "sub/data.jsonl"):
            self.review["splits"]["train"]["file"] = filename
            self.manifest.write_text(json.dumps(self.review), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "filename"):
                train_lora.load_split_manifest(self.manifest, self.dataset)

    def test_split_manifest_rejects_versions_unknown_fields_and_symlinks(self):
        for change in ({"format_version": True}, {"format_version": 2}, {"extra": "ignored"}, {"license": "bad\nlicense"}):
            self.manifest.write_text(json.dumps(dict(self.review, **change)), encoding="utf-8")
            with self.assertRaises(ValueError):
                train_lora.load_split_manifest(self.manifest, self.dataset)
        self.manifest.write_text(json.dumps(self.review), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "differs"):
            train_lora.load_split_manifest(self.manifest, self.root / "other.jsonl")
        link = self.root / "linked-review.json"
        link.symlink_to(self.manifest)
        with self.assertRaises(OSError):
            train_lora.load_split_manifest(link, self.dataset)
        self.manifest.write_text('{"format_version":1,"format_version":1}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            train_lora.load_split_manifest(self.manifest, self.dataset)

    def test_dry_run_hash_and_output_separation(self):
        command = [sys.executable, str(Path(train_lora.__file__)), "--base-model", str(self.base),
                   "--dataset", str(self.dataset), "--dataset-manifest", str(self.manifest), "--output", str(self.root / "candidate"), "--dry-run"]
        result = subprocess.run(command, capture_output=True, text=True, timeout=10, check=True)
        report = json.loads(result.stdout)
        self.assertFalse(report["training_started"])
        self.assertEqual(report["dataset_sha256"], sha256(self.dataset.read_bytes()).hexdigest())
        self.assertFalse((self.root / "candidate").exists())
        command[command.index("--output") + 1] = str(self.base / "nested")
        result = subprocess.run(command, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must not contain each other", result.stderr)


if __name__ == "__main__":
    unittest.main()
