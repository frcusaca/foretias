"""Foretias Performance Benchmark: Rust vs Python stamp/verify comparison.

Benchmarks PyTimeFamily (full stamp with calendar) against PyCryptoServer
(direct SHA-256) to measure the overhead of the orchestration layer.

Usage:
    python integration-tests/benchmark.py
    python integration-tests/benchmark.py --iterations 5000 --runs 10
"""

import argparse
import sys
import timeit

from foretias_p2p import PyTimeFamily, PyCryptoServer


def benchmark_stamp(iterations: int, runs: int) -> float:
    """Benchmark PyTimeFamily stamp operation.

    Creates a fresh PyTimeFamily per run to avoid tick counter effects.
    Returns best time across runs (standard benchmark idiom).

    Args:
        iterations: Number of stamp operations per run.
        runs: Number of runs to take the best of.

    Returns:
        Best total time in seconds.
    """

    def _run():
        tf = PyTimeFamily(tbn="bench-stamp")
        content = b"hello world"
        for _ in range(iterations):
            tf.stamp(content)

    times = timeit.repeat(_run, repeat=runs, number=1)
    return min(times)


def benchmark_verify(iterations: int, runs: int) -> float:
    """Benchmark PyTimeFamily verify operation.

    Stamps once, then verifies the same Foretis repeatedly.
    Returns best time across runs.

    Args:
        iterations: Number of verify operations per run.
        runs: Number of runs to take the best of.

    Returns:
        Best total time in seconds.
    """

    def _run():
        tf = PyTimeFamily(tbn="bench-verify")
        content = b"hello world"
        foretis = tf.stamp(content)
        for _ in range(iterations):
            tf.verify(content, foretis)

    times = timeit.repeat(_run, repeat=runs, number=1)
    return min(times)


def benchmark_sha256(iterations: int, runs: int) -> float:
    """Benchmark PyCryptoServer direct SHA-256 operation.

    Baseline: raw hashing without stamp orchestration overhead.
    Returns best time across runs.

    Args:
        iterations: Number of sha256 operations per run.
        runs: Number of runs to take the best of.

    Returns:
        Best total time in seconds.
    """

    def _run():
        cs = PyCryptoServer("ed25519")
        content = b"hello world"
        for _ in range(iterations):
            cs.sha256(content)

    times = timeit.repeat(_run, repeat=runs, number=1)
    return min(times)


def benchmark_sign(iterations: int, runs: int) -> float:
    """Benchmark PyCryptoServer direct sign operation.

    Baseline: raw signing without stamp orchestration overhead.
    Returns best time across runs.

    Args:
        iterations: Number of sign operations per run.
        runs: Number of runs to take the best of.

    Returns:
        Best total time in seconds.
    """

    def _run():
        cs = PyCryptoServer("ed25519")
        content = b"hello world"
        for _ in range(iterations):
            cs.sign(content)

    times = timeit.repeat(_run, repeat=runs, number=1)
    return min(times)


def print_results(iterations: int, runs: int) -> None:
    """Run all benchmarks and print a formatted results table.

    Args:
        iterations: Number of operations per benchmark run.
        runs: Number of repetitions to take the best of.
    """
    print("Foretias Performance Benchmark")
    print("=" * 65)
    print(f"  iterations: {iterations}")
    print(f"  runs:       {runs} (best-of)")
    print("=" * 65)
    print()

    benchmarks = [
        ("sha256 (PyCryptoServer)", benchmark_sha256),
        ("sign (PyCryptoServer)", benchmark_sign),
        ("stamp (PyTimeFamily)", benchmark_stamp),
        ("verify (PyTimeFamily)", benchmark_verify),
    ]

    # Column headers
    print(f"  {'operation':<30} {'iter':>6} {'best(s)':>10} {'ops/sec':>10}")
    print(f"  {'-' * 30} {'-' * 6} {'-' * 10} {'-' * 10}")

    for name, func in benchmarks:
        total_time = func(iterations, runs)
        ops_per_sec = iterations / total_time if total_time > 0 else 0
        print(
            f"  {name:<30} {iterations:>6} {total_time:>10.6f} {ops_per_sec:>10.1f}"
        )

    print()


def main() -> None:
    """Parse arguments and run benchmarks."""
    parser = argparse.ArgumentParser(
        description="Benchmark Foretias stamp/verify performance."
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=1000,
        help="Number of operations per benchmark run (default: 1000)",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=5,
        help="Number of runs to take the best of (default: 5)",
    )
    args = parser.parse_args()
    print_results(args.iterations, args.runs)


if __name__ == "__main__":
    main()
