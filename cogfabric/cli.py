"""English-only CLI for local orchestration, evaluations and gated promotion."""
from __future__ import annotations

import argparse
import asyncio
import json
import pathlib
import sys

from .config import Config
from .engine import CognitiveEngine
from .evaluation import benchmark, read_cases, integrity_check, quality_check, sha256_file
from .models import create_backend
from .security import SecurityKernel
from .service import FabricService, request
from .skills import bootstrap, ingest
from .storage import Store


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="cogfabric", description="Cognitive Fabric 1.0 research runtime")
    p.add_argument("--home", help="Writable state directory (defaults to ~/.local/share/cogfabric)")
    p.add_argument("--backend", choices=["demo", "openai-local"], help="Override COGFABRIC_BACKEND")
    p.add_argument("--json", action="store_true", help="Output JSON")
    sub = p.add_subparsers(dest="command", required=True)
    sub.add_parser("init")
    sub.add_parser("serve")
    ask = sub.add_parser("ask")
    ask.add_argument("query")
    ask.add_argument("--mode", choices=["fast", "standard", "deep", "research"], default="standard")
    ask.add_argument("--socket", action="store_true", help="Send to running Unix-socket daemon")
    sub.add_parser("status")
    sub.add_parser("skills")
    ingest_p = sub.add_parser("ingest")
    ingest_p.add_argument("file", type=pathlib.Path)
    ingest_p.add_argument("--skill", required=True)
    ingest_p.add_argument("--source", required=True)
    ingest_p.add_argument("--license", required=True)
    search_p = sub.add_parser("search")
    search_p.add_argument("query")
    search_p.add_argument("--skill")
    show = sub.add_parser("task")
    show.add_argument("task_id")
    bench = sub.add_parser("bench")
    bench.add_argument("file", type=pathlib.Path)
    bench.add_argument("--mode", choices=["fast", "standard", "deep", "research"], default="fast")
    cand = sub.add_parser("candidate")
    cand_sub = cand.add_subparsers(dest="action", required=True)
    cand_sub.add_parser("list")
    create = cand_sub.add_parser("create")
    create.add_argument("--skill", required=True)
    create.add_argument("--kind", choices=["rag", "lora"], required=True)
    create.add_argument("--artifact", type=pathlib.Path, required=True)
    evaluate = cand_sub.add_parser("evaluate")
    evaluate.add_argument("id")
    evaluate.add_argument("--suite", choices=["quality", "integrity"], required=True)
    evaluate.add_argument("--dataset", type=pathlib.Path, help="Required for quality checks")
    promote = cand_sub.add_parser("promote")
    promote.add_argument("id")
    promote.add_argument("--human-approve", action="store_true", help="Explicit operator approval")
    sub.add_parser("self-test")
    return p


def emit(output: object, json_mode: bool = False) -> None:
    if json_mode or not isinstance(output, str):
        print(json.dumps(output, indent=2, ensure_ascii=False))
    else:
        print(output)


async def execute(args: argparse.Namespace) -> None:
    cfg = Config.load(args.home, args.backend)
    cfg.prepare()
    if args.command == "serve":
        print(f"Listening on {cfg.socket_path}", flush=True)
        await FabricService(cfg).serve()
        return
    if args.command == "ask" and args.socket:
        output = await request(cfg, "ask", query=args.query, mode=args.mode)
        emit(output if args.json else output["answer"], args.json)
        return
    store = Store(cfg.db_path)
    bootstrap(store)
    try:
        if args.command == "init":
            output = {"state": str(cfg.data_dir), "skills": len(store.skills()), "version": "1.0.0"}
        elif args.command == "ask":
            output = (await CognitiveEngine(cfg, store).run(args.query, args.mode)).as_dict()
            if not args.json:
                output = output["answer"]
        elif args.command == "status":
            output = {"state": str(cfg.data_dir), "backend": cfg.backend,
                      "model": cfg.model, "skills": len(store.skills()),
                      "candidates": len(store.candidates())}
        elif args.command == "skills":
            output = [{k: s[k] for k in ("id", "description", "tags", "active_version")} for s in store.skills()]
        elif args.command == "ingest":
            output = ingest(store, SecurityKernel(), args.file, args.skill, args.source, args.license)
        elif args.command == "search":
            output = store.search(args.query, args.skill)
        elif args.command == "task":
            output = {"task": store.task(args.task_id), "events": store.events(args.task_id)}
        elif args.command == "bench":
            output = await benchmark(CognitiveEngine(cfg, store), read_cases(args.file), args.mode)
        elif args.command == "candidate":
            SecurityKernel().authorize("human-cli", "candidate." + ("evaluate" if args.action == "evaluate" else "create" if args.action == "create" else "promote" if args.action == "promote" else "create"))
            if args.action == "list":
                output = store.candidates()
            elif args.action == "create":
                if args.artifact.is_symlink() or not args.artifact.is_file():
                    raise ValueError("Candidate artifact must be a regular file")
                output = store.create_candidate(args.skill, args.kind, str(args.artifact.resolve()),
                                                sha256_file(args.artifact))
            elif args.action == "evaluate":
                if args.suite == "integrity":
                    output = integrity_check(store, args.id)
                else:
                    if not args.dataset:
                        raise ValueError("--dataset is required for quality evaluation")
                    output = await quality_check(store, CognitiveEngine(cfg, store), args.id, args.dataset)
            elif args.action == "promote":
                if not args.human_approve:
                    raise PermissionError("Promotion requires --human-approve")
                output = store.promote(args.id)
        elif args.command == "self-test":
            output = {"arithmetic": (await CognitiveEngine(cfg, store).run("calc: (7 + 5) * 3", "fast")).answer == "36",
                      "skills": len(store.skills()), "binary": "messagepack/v1"}
        else:
            raise ValueError("Unknown command")
        emit(output, args.json)
    finally:
        store.close()


def main(argv: list[str] | None = None) -> None:
    args = parser().parse_args(argv)
    try:
        asyncio.run(execute(args))
    except (ValueError, PermissionError, OSError, RuntimeError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        raise SystemExit(2) from exc


if __name__ == "__main__":
    main()
