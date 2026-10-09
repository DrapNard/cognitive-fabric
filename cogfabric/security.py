"""Deterministic capability policy: model text never grants itself privileges."""
from __future__ import annotations

import ast
from dataclasses import dataclass, field
import operator
from pathlib import Path

MAX_DOCUMENT_BYTES = 256_000
OPS = {
    ast.Add: operator.add, ast.Sub: operator.sub, ast.Mult: operator.mul,
    ast.Div: operator.truediv, ast.FloorDiv: operator.floordiv,
    ast.Mod: operator.mod, ast.Pow: operator.pow,
}


class SecurityError(PermissionError):
    pass


@dataclass
class SecurityKernel:
    grants: dict[str, frozenset[str]] = field(default_factory=lambda: {
        "interface": frozenset({"llm.generate"}),
        "jev-r": frozenset({"llm.generate", "knowledge.search", "math.evaluate"}),
        "jev-plus": frozenset({"knowledge.search", "math.evaluate"}),
        "human-cli": frozenset({"knowledge.ingest", "candidate.create", "candidate.evaluate", "candidate.promote"}),
    })

    def authorize(self, principal: str, capability: str) -> None:
        if capability not in self.grants.get(principal, frozenset()):
            raise SecurityError(f"Denied: {principal!r} cannot use {capability!r}")

    def check_ingest_file(self, path: Path) -> Path:
        self.authorize("human-cli", "knowledge.ingest")
        if path.is_symlink() or not path.is_file() or path.suffix.lower() not in (".txt", ".md"):
            raise SecurityError("Only regular .md/.txt files (not symlinks) can be ingested")
        if path.stat().st_size > MAX_DOCUMENT_BYTES:
            raise SecurityError("Document exceeds size limit")
        return path


def safe_arithmetic(expression: str) -> float | int:
    """Evaluate a bounded arithmetic AST; do not use eval or Python execution."""
    if not isinstance(expression, str) or len(expression) > 120:
        raise SecurityError("Expression exceeds size limit")
    tree = ast.parse(expression, mode="eval")
    nodes = list(ast.walk(tree))
    if len(nodes) > 35:
        raise SecurityError("Expression exceeds complexity limit")

    def walk(node: ast.AST, depth: int = 0) -> float | int:
        if depth > 12:
            raise SecurityError("Expression depth exceeded")
        if isinstance(node, ast.Expression):
            return walk(node.body, depth + 1)
        if isinstance(node, ast.Constant) and type(node.value) in (float, int):
            if abs(node.value) > 1e12:
                raise SecurityError("Number exceeds range")
            return node.value
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.USub, ast.UAdd)):
            n = walk(node.operand, depth + 1)
            return -n if isinstance(node.op, ast.USub) else n
        if isinstance(node, ast.BinOp) and type(node.op) in OPS:
            left, right = walk(node.left, depth + 1), walk(node.right, depth + 1)
            if isinstance(node.op, ast.Pow) and (abs(right) > 8 or abs(left) > 1e6):
                raise SecurityError("Exponentiation limited")
            if isinstance(node.op, (ast.Div, ast.FloorDiv, ast.Mod)) and right == 0:
                raise SecurityError("Division by zero")
            output = OPS[type(node.op)](left, right)
            if abs(output) > 1e15:
                raise SecurityError("Result exceeds range")
            return output
        raise SecurityError("Only numeric arithmetic is permitted")

    return walk(tree)
