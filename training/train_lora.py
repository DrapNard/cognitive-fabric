#!/usr/bin/env python3
"""Human-invoked, offline-only LoRA training experiment.

This script NEVER publishes, activates or merges learned weights. Validate
provenance/licensing and benchmark independently before manual promotion.
Dependencies: torch, transformers, peft, safetensors. QLoRA also needs bitsandbytes.
"""
from __future__ import annotations

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import random
import stat
from importlib.metadata import version
from uuid import uuid4


def load_data(path: Path) -> tuple[list[dict], str]:
    """Read once, bound allocation, and hash exactly the validated input snapshot."""
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 8_000_000:
            raise ValueError("Dataset must be a regular file smaller than 8 MB")
        data = stream.read(8_000_001)
        if len(data) > 8_000_000:
            raise ValueError("Dataset exceeds 8 MB")
    rows = validate_rows(data.decode("utf-8"))
    return rows, sha256(data).hexdigest()


def validate_data(path: Path) -> list[dict]:
    return load_data(path)[0]


def unique_object(pairs: list[tuple]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate dataset field")
        result[key] = value
    return result


def validate_rows(text: str) -> list[dict]:
    rows = []
    seen = {}
    for line in text.splitlines():
        if not line.strip():
            continue
        row = json.loads(line, object_pairs_hook=unique_object)
        if not isinstance(row, dict) or set(row) != {"prompt", "response", "source", "license"} or not all(isinstance(row.get(k), str) and row[k].strip()
                                                  for k in ("prompt", "response", "source", "license")):
            raise ValueError("Each row must have prompt, response, source and license")
        if len(row["prompt"]) > 8000 or len(row["response"]) > 8000:
            raise ValueError("Oversized training example")
        for key, maximum in (("source", 2048), ("license", 256)):
            if len(row[key]) > maximum or not row[key].isprintable():
                raise ValueError("Invalid provenance bounds")
        digest = sha256(" ".join(row["prompt"].split()).encode()).hexdigest()
        if digest in seen and row["response"] != seen[digest]:
            raise ValueError("Conflicting responses for a duplicate prompt")
        if digest not in seen:
            rows.append(row)
            seen[digest] = row["response"]
        if len(rows) > 4096:
            raise ValueError("Training example capacity exhausted")
    if len(rows) < 4:
        raise ValueError("At least four distinct, provenance-labelled examples required")
    return rows


def load_split_manifest(path: Path, training_path: Path) -> tuple[list[dict], dict]:
    """Verify reviewed split identities before importing any ML dependencies.

    Prompt disjointness detects exact normalized reuse, not semantic leakage.
    Only training rows are returned to the optimizer.
    """
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 32_000:
            raise ValueError("Split manifest must be a regular file at most 32 KB")
        data = stream.read(32_001)
    if len(data) > 32_000:
        raise ValueError("Split manifest exceeds 32 KB")
    manifest = json.loads(data.decode("utf-8"), object_pairs_hook=unique_object)
    if not isinstance(manifest, dict) or set(manifest) != {"format_version", "source", "license", "splits"}:
        raise ValueError("Invalid split manifest fields")
    if type(manifest["format_version"]) is not int or manifest["format_version"] != 1:
        raise ValueError("Unsupported split manifest version")
    for field, maximum in (("source", 2048), ("license", 256)):
        value = manifest[field]
        if not isinstance(value, str) or not value.strip() or len(value) > maximum or not value.isprintable():
            raise ValueError("Invalid split manifest provenance")
    splits = manifest["splits"]
    if not isinstance(splits, dict) or set(splits) != {"train", "validation", "test"}:
        raise ValueError("Require train, validation and test splits")
    seen = set()
    reports = {}
    training_rows = None
    for name in ("train", "validation", "test"):
        entry = splits[name]
        if not isinstance(entry, dict) or set(entry) != {"file", "sha256"}:
            raise ValueError("Invalid split entry")
        filename, expected = entry["file"], entry["sha256"]
        if (not isinstance(filename, str) or not 1 <= len(filename) <= 128
                or filename in {".", ".."}
                or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-" for c in filename)):
            raise ValueError("Split file must be a single bounded local filename")
        if not isinstance(expected, str) or len(expected) != 64 or any(c not in "0123456789abcdef" for c in expected):
            raise ValueError("Invalid split hash")
        split_path = path.parent / filename
        if name == "train" and split_path.resolve() != training_path.resolve():
            raise ValueError("Training dataset differs from reviewed split manifest")
        rows, digest = load_data(split_path)
        if digest != expected:
            raise ValueError("Split dataset hash mismatch")
        prompts = {sha256(" ".join(row["prompt"].split()).encode()).hexdigest() for row in rows}
        if seen.intersection(prompts):
            raise ValueError("Normalized prompt overlap across dataset splits")
        seen.update(prompts)
        reports[name] = {"file": filename, "sha256": digest, "examples": len(rows)}
        if name == "train":
            training_rows = rows
    return training_rows, {"manifest_sha256": sha256(data).hexdigest(),
                           "source": manifest["source"], "license": manifest["license"],
                           "splits": reports, "prompt_disjoint": True,
                           "semantic_independence_verified": False}


def require_progress(previous_step: int, current_step: int) -> None:
    if previous_step == current_step:
        raise ValueError("No supervised tokens survived truncation in a complete dataset pass")


def describe_artifacts(directory: Path) -> list[dict]:
    """Hash the inactive adapter bundle under the runtime's 64 MB import ceiling."""
    artifacts = []
    total = 0
    for path in sorted(directory.iterdir()):
        if len(artifacts) >= 16:
            raise ValueError("Adapter bundle entry capacity exhausted")
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK)
        with os.fdopen(fd, "rb") as stream:
            metadata = os.fstat(stream.fileno())
            if not stat.S_ISREG(metadata.st_mode) or path.suffix == ".bin":
                raise ValueError("Adapter outputs must be regular non-pickle files")
            total += metadata.st_size
            if total > 64_000_000:
                raise ValueError("Adapter bundle exceeds 64 MB")
            digest = sha256()
            copied = 0
            while chunk := stream.read(65536):
                copied += len(chunk)
                if copied > metadata.st_size:
                    raise ValueError("Adapter changed during hashing")
                digest.update(chunk)
            after = os.fstat(stream.fileno())
            if copied != metadata.st_size or after.st_mtime_ns != metadata.st_mtime_ns:
                raise ValueError("Adapter changed during hashing")
        artifacts.append({"name": path.name, "bytes": copied, "sha256": digest.hexdigest()})
    if not any(item["name"].endswith(".safetensors") for item in artifacts):
        raise ValueError("No safetensors adapter output")
    return artifacts


def main() -> None:
    p = argparse.ArgumentParser(description="Offline LoRA adapter training (human-invoked)")
    p.add_argument("--base-model", type=Path, required=True, help="Existing local HF model directory")
    p.add_argument("--dataset", type=Path, required=True)
    p.add_argument("--dataset-manifest", type=Path, required=True,
                   help="Reviewed hashed train/validation/test split manifest")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--max-steps", type=int, default=30)
    p.add_argument("--max-length", type=int, default=512)
    p.add_argument("--rank", type=int, default=8)
    p.add_argument("--qlora", action="store_true")
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--dry-run", action="store_true")
    args = p.parse_args()
    rows, dataset_review = load_split_manifest(args.dataset_manifest, args.dataset)
    dataset_hash = dataset_review["splits"]["train"]["sha256"]
    if not args.base_model.is_dir() or args.base_model.is_symlink():
        p.error("--base-model must be a local directory")
    if args.max_steps < 1 or args.max_steps > 10000 or args.max_length not in range(64, 4097):
        p.error("Invalid training limits")
    if args.rank < 1 or args.rank > 128:
        p.error("Invalid LoRA rank")
    base = args.base_model.resolve()
    output = args.output.resolve()
    if output == base or output in base.parents or base in output.parents:
        p.error("Output and base-model directories must not contain each other")
    if args.output.exists() or args.output.is_symlink():
        p.error("Output must be a new directory")
    if args.seed < 0 or args.seed > 2**32 - 1:
        p.error("Seed must fit an unsigned 32-bit integer")
    if args.dry_run:
        print(json.dumps({"status": "validated", "examples": len(rows),
                          "base_model": str(base), "dataset_sha256": dataset_hash,
                          "seed": args.seed, "dataset_review": dataset_review, "training_started": False}, indent=2))
        return

    import torch
    from torch.utils.data import DataLoader, Dataset
    from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig
    from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training

    random.seed(args.seed)
    torch.manual_seed(args.seed)
    args.output.mkdir(parents=True, exist_ok=False, mode=0o700)

    if args.qlora and not torch.cuda.is_available():
        p.error("QLoRA requires a supported CUDA device")
    tokenizer = AutoTokenizer.from_pretrained(str(args.base_model), local_files_only=True,
                                            trust_remote_code=False)
    if tokenizer.pad_token_id is None:
        tokenizer.pad_token = tokenizer.eos_token
    kwargs = {"local_files_only": True, "trust_remote_code": False, "use_safetensors": True}
    if args.qlora:
        kwargs["quantization_config"] = BitsAndBytesConfig(load_in_4bit=True,
            bnb_4bit_compute_dtype=torch.float16, bnb_4bit_quant_type="nf4")
        kwargs["device_map"] = "auto"
    else:
        kwargs["torch_dtype"] = torch.float16 if torch.cuda.is_available() else torch.float32
    model = AutoModelForCausalLM.from_pretrained(str(args.base_model), **kwargs)
    if args.qlora:
        model = prepare_model_for_kbit_training(model)
    elif torch.cuda.is_available():
        model = model.cuda()
    model.config.use_cache = False
    model = get_peft_model(model, LoraConfig(r=args.rank, lora_alpha=args.rank * 2,
        lora_dropout=0.05, bias="none", task_type="CAUSAL_LM", target_modules="all-linear"))

    class ExampleDataset(Dataset):
        def __len__(self):
            return len(rows)

        def __getitem__(self, index):
            row = rows[index]
            prefix = f"Instruction:\n{row['prompt']}\nResponse:\n"
            full = prefix + row["response"] + (tokenizer.eos_token or "")
            encoded = tokenizer(full, truncation=True, max_length=args.max_length,
                                padding="max_length", return_tensors="pt")
            prefix_ids = tokenizer(prefix, add_special_tokens=True)["input_ids"]
            input_ids = encoded["input_ids"][0]
            mask = encoded["attention_mask"][0]
            labels = input_ids.clone()
            labels[: min(len(prefix_ids), len(labels))] = -100
            labels[mask == 0] = -100
            return {"input_ids": input_ids, "attention_mask": mask, "labels": labels}

    loader = DataLoader(ExampleDataset(), batch_size=1, shuffle=True)
    optimizer = torch.optim.AdamW((p for p in model.parameters() if p.requires_grad), lr=1e-4)
    model.train()
    losses = []
    step = 0
    while step < args.max_steps:
        previous_step = step
        for batch in loader:
            device = next(model.parameters()).device
            batch = {k: v.to(device) for k, v in batch.items()}
            if not bool((batch["labels"] != -100).any()):
                continue
            loss = model(**batch).loss
            if not bool(torch.isfinite(loss)):
                raise RuntimeError("Nonfinite training loss")
            loss.backward()
            optimizer.step()
            optimizer.zero_grad(set_to_none=True)
            losses.append(float(loss.detach().cpu()))
            step += 1
            if step >= args.max_steps:
                break
        require_progress(previous_step, step)
    model.save_pretrained(str(args.output), safe_serialization=True)
    metadata = {"base_model": str(args.base_model.resolve()), "examples": len(rows),
                "run_id": str(uuid4()), "artifacts": describe_artifacts(args.output),
                "format_version": 1, "dataset_review": dataset_review, "steps": step, "final_loss": losses[-1], "dataset_sha256": dataset_hash,
                "consumed_rows_sha256": sha256(json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
                "seed": args.seed, "rank": args.rank, "max_length": args.max_length,
                "qlora": args.qlora,
                "dependencies": {name: version(name) for name in ("torch", "transformers", "peft", "safetensors")},
                "independently_evaluated": False, "isolated_execution_verified": False,
                "base_weights_attested": False,
                "status": "unreviewed_candidate", "production_ready": False}
    (args.output / "training_manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    print(json.dumps(metadata, indent=2))


if __name__ == "__main__":
    main()
