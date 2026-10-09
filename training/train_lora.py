#!/usr/bin/env python3
"""Human-invoked, offline-only LoRA training experiment.

This script NEVER publishes, activates or merges learned weights. Validate
provenance/licensing and benchmark independently before manual promotion.
Dependencies: pip install -e '.[training]'. For --qlora also install bitsandbytes.
"""
from __future__ import annotations

import argparse
from hashlib import sha256
import json
from pathlib import Path
import random


def validate_data(path: Path) -> list[dict]:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 8_000_000:
        raise ValueError("Dataset must be a regular file smaller than 8 MB")
    rows = []
    seen = set()
    for line in path.read_text("utf-8").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        if not isinstance(row, dict) or not all(isinstance(row.get(k), str) and row[k].strip()
                                                  for k in ("prompt", "response", "source", "license")):
            raise ValueError("Each row must have prompt, response, source and license")
        if len(row["prompt"]) > 8000 or len(row["response"]) > 8000:
            raise ValueError("Oversized training example")
        digest = sha256(row["prompt"].strip().encode()).hexdigest()
        if digest not in seen:
            rows.append(row)
            seen.add(digest)
    if len(rows) < 4:
        raise ValueError("At least four distinct, provenance-labelled examples required")
    return rows


def main() -> None:
    p = argparse.ArgumentParser(description="Offline LoRA adapter training (human-invoked)")
    p.add_argument("--base-model", type=Path, required=True, help="Existing local HF model directory")
    p.add_argument("--dataset", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--max-steps", type=int, default=30)
    p.add_argument("--max-length", type=int, default=512)
    p.add_argument("--rank", type=int, default=8)
    p.add_argument("--qlora", action="store_true")
    p.add_argument("--dry-run", action="store_true")
    args = p.parse_args()
    rows = validate_data(args.dataset)
    if not args.base_model.is_dir() or args.base_model.is_symlink():
        p.error("--base-model must be a local directory")
    if args.max_steps < 1 or args.max_steps > 10000 or args.max_length not in range(64, 4097):
        p.error("Invalid training limits")
    if args.rank < 1 or args.rank > 128:
        p.error("Invalid LoRA rank")
    if args.output.resolve() == args.base_model.resolve():
        p.error("Output must be different from base-model directory")
    if args.dry_run:
        print(json.dumps({"status": "validated", "examples": len(rows),
                          "base_model": str(args.base_model), "training_started": False}, indent=2))
        return

    import torch
    from torch.utils.data import DataLoader, Dataset
    from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig
    from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training

    if args.qlora and not torch.cuda.is_available():
        p.error("QLoRA requires a supported CUDA device")
    tokenizer = AutoTokenizer.from_pretrained(str(args.base_model), local_files_only=True)
    if tokenizer.pad_token_id is None:
        tokenizer.pad_token = tokenizer.eos_token
    kwargs = {"local_files_only": True, "trust_remote_code": False}
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
    args.output.mkdir(parents=True, exist_ok=False)
    model.save_pretrained(str(args.output), safe_serialization=True)
    metadata = {"base_model": str(args.base_model.resolve()), "examples": len(rows),
                "steps": step, "final_loss": losses[-1], "dataset_sha256": sha256(args.dataset.read_bytes()).hexdigest(),
                "status": "unreviewed_candidate", "production_ready": False}
    (args.output / "training_manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    print(json.dumps(metadata, indent=2))


if __name__ == "__main__":
    main()
