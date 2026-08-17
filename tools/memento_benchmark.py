#!/usr/bin/env python3
"""Benchmark Memento query quality against a JSONL dataset via Unix socket."""

from __future__ import annotations

import argparse
import json
import os
import socket
import statistics
import unicodedata
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", required=True, help="Path to benchmark dataset JSONL.")
    parser.add_argument("--output", required=True, help="Path to write report JSON.")
    parser.add_argument("--top-k", type=int, default=5, help="Number of results to request.")
    parser.add_argument(
        "--socket-path",
        default=None,
        help="Unix socket path. Defaults to $MEMENTO_DATA_DIR/memento.sock or ~/.memento/memento.sock.",
    )
    return parser.parse_args()


def default_socket_path() -> Path:
    data_dir = os.environ.get("MEMENTO_DATA_DIR")
    if data_dir:
        return Path(data_dir) / "memento.sock"
    return Path.home() / ".memento" / "memento.sock"


def canonicalize_loose(path: str) -> str:
    try:
        return str(Path(path).resolve())
    except OSError:
        return str(Path(path))


def term_recall(text: str, expected_terms: list[str]) -> float:
    if not expected_terms:
        return 0.0
    haystack = normalize_recall_text(text)
    compact_haystack = "".join(character for character in haystack if character.isalnum())
    hits = 0
    for term in expected_terms:
        needle = normalize_recall_text(term)
        digits = "".join(character for character in needle if character.isdigit())
        numeric_only = all(character.isdigit() or character.isspace() for character in needle)
        if needle in haystack or (digits and numeric_only and digits in compact_haystack):
            hits += 1
    return hits / len(expected_terms)


def normalize_recall_text(text: str) -> str:
    decomposed = unicodedata.normalize("NFD", text.lower())
    folded = "".join(character for character in decomposed if not unicodedata.combining(character))
    alphanumeric = "".join(character if character.isalnum() else " " for character in folded)
    return " ".join(alphanumeric.split())


def query_memento(socket_path: Path, query: str, top_k: int) -> dict[str, Any]:
    payload = json.dumps({"query": query, "top_k": top_k}).encode("utf-8")
    request = (
        b"POST /query HTTP/1.1\r\n"
        b"Host: localhost\r\n"
        b"Content-Type: application/json\r\n"
        b"Connection: close\r\n" + f"Content-Length: {len(payload)}\r\n\r\n".encode() + payload
    )

    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(str(socket_path))
        client.sendall(request)
        response = bytearray()
        while True:
            chunk = client.recv(65536)
            if not chunk:
                break
            response.extend(chunk)

    header_bytes, _, body = bytes(response).partition(b"\r\n\r\n")
    header_text = header_bytes.decode("utf-8", errors="replace")
    status_line = header_text.splitlines()[0] if header_text else ""
    if " 200 " not in status_line:
        raise RuntimeError(f"Query failed: {status_line or 'missing status line'}")
    return json.loads(body.decode("utf-8"))


def load_cases(dataset_path: Path) -> list[dict[str, Any]]:
    cases = []
    for line in dataset_path.read_text().splitlines():
        if not line.strip():
            continue
        cases.append(json.loads(line))
    return cases


def benchmark(socket_path: Path, cases: list[dict[str, Any]], top_k: int) -> dict[str, Any]:
    per_case = []
    hits = 0
    reciprocal_rank_sum = 0.0
    answer_term_sum = 0.0
    result_term_sum = 0.0
    confidences: list[float] = []

    for case in cases:
        response = query_memento(socket_path, case["query"], top_k)
        results = response.get("results", [])
        answer = response.get("answer", "")
        confidence = float(response.get("confidence", 0.0))
        confidences.append(confidence)

        expected_path = canonicalize_loose(case["expected_path"])
        rank = None
        for index, result in enumerate(results):
            if canonicalize_loose(result.get("source_path", "")) == expected_path:
                rank = index + 1
                break

        if rank is not None:
            hits += 1
            reciprocal_rank_sum += 1.0 / rank

        combined_results = "\n".join(result.get("content", "") for result in results)
        answer_term_sum += term_recall(answer, case.get("expected_terms", []))
        result_term_sum += term_recall(combined_results, case.get("expected_terms", []))

        per_case.append(
            {
                "id": case["id"],
                "query": case["query"],
                "expected_path": expected_path,
                "hit": rank is not None,
                "rank": rank,
                "confidence": confidence,
                "top_result_path": results[0].get("source_path") if results else None,
                "answer_term_recall": term_recall(answer, case.get("expected_terms", [])),
                "result_term_recall": term_recall(combined_results, case.get("expected_terms", [])),
                "answer_preview": answer[:500],
            }
        )

    total = len(cases) or 1
    return {
        "cases": len(cases),
        "top_k": top_k,
        "hit_rate": hits / total,
        "mrr": reciprocal_rank_sum / total,
        "avg_answer_term_recall": answer_term_sum / total,
        "avg_result_term_recall": result_term_sum / total,
        "avg_confidence": statistics.fmean(confidences) if confidences else 0.0,
        "per_case": per_case,
    }


def main() -> int:
    args = parse_args()
    dataset_path = Path(args.dataset)
    output_path = Path(args.output)
    socket_path = Path(args.socket_path) if args.socket_path else default_socket_path()

    cases = load_cases(dataset_path)
    report = benchmark(socket_path, cases, args.top_k)
    report["dataset"] = str(dataset_path)
    report["socket_path"] = str(socket_path)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")

    print(f"benchmark report: {output_path}")
    print(f"cases: {report['cases']}")
    print(f"hit@{report['top_k']}: {report['hit_rate'] * 100:.1f}%")
    print(f"mrr: {report['mrr']:.3f}")
    print(f"answer term recall: {report['avg_answer_term_recall']:.3f}")
    print(f"result term recall: {report['avg_result_term_recall']:.3f}")
    print(f"avg confidence: {report['avg_confidence']:.3f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
