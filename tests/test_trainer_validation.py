import importlib.util
from pathlib import Path
import pytest

spec = importlib.util.spec_from_file_location("trainer", Path(__file__).parents[1] / "training" / "train_lora.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def test_provenance_required(tmp_path):
    dataset = tmp_path / "dataset.jsonl"
    dataset.write_text('{"prompt":"p","response":"r"}\n')
    with pytest.raises(ValueError):
        module.validate_data(dataset)


def test_valid_demo_dataset():
    path = Path(__file__).parents[1] / "examples" / "training_examples.jsonl"
    assert len(module.validate_data(path)) == 4
