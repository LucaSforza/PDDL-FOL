"""Reproduce the HTG 5D VisitAll full-process comparison (Linux only)."""

import argparse
import csv
import os
import re
import resource
import signal
import subprocess
import time
from pathlib import Path


DEFAULT_CASES = (0, 1, 2, 3, 4, 5, 8, 9)
SOLVERS = ("folplan_indexed", "folplan_lifted", "fd_blind", "powerlifted")


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--instances", type=Path, required=True,
                        help="HTG 5-dim-visitall-CLOSE-g1 directory")
    parser.add_argument("--folplan-indexed", type=Path, required=True)
    parser.add_argument("--folplan-lifted", type=Path, required=True)
    parser.add_argument("--fast-downward", type=Path, required=True)
    parser.add_argument("--powerlifted", type=Path, required=True,
                        help="Powerlifted powerlifted.py driver")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cases", default=",".join(map(str, DEFAULT_CASES)),
                        help="comma-separated problem indices (default: 0,1,2,3,4,5,8,9)")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--timeout", type=int, default=15)
    parser.add_argument("--memory-gib", type=float, default=1)
    args = parser.parse_args()
    if args.runs < 1 or args.timeout <= 0 or args.memory_gib <= 0:
        parser.error("runs, timeout, and memory-gib must be positive")
    try:
        args.cases = tuple(int(value) for value in args.cases.split(","))
    except ValueError:
        parser.error("cases must be comma-separated problem indices")
    if not args.cases or any(index < 0 for index in args.cases):
        parser.error("cases must contain nonnegative problem indices")
    for name in ("instances", "folplan_indexed", "folplan_lifted", "fast_downward",
                 "powerlifted", "output"):
        setattr(args, name, getattr(args, name).resolve())
    for path in (args.instances / "domain.pddl", *(args.instances / f"p{i}.pddl" for i in args.cases),
                 args.folplan_indexed, args.folplan_lifted, args.fast_downward, args.powerlifted):
        if not path.is_file():
            parser.error(f"missing input: {path}")
    args.output.mkdir(parents=True, exist_ok=True)
    return args


def command(args, solver, problem, run):
    domain = args.instances / "domain.pddl"
    instance = args.instances / f"p{problem}.pddl"
    plan = args.output / f"{solver}-p{problem}-r{run}.plan"
    if solver.startswith("folplan"):
        binary = args.folplan_indexed if solver == "folplan_indexed" else args.folplan_lifted
        return [str(binary), "pddl", str(domain), str(instance), "--max-states", "100000",
                "--max-ground-actions", "1000000"]
    if solver == "fd_blind":
        return [str(args.fast_downward), "--plan-file", str(plan), str(domain), str(instance),
                "--search", "astar(blind())"]
    return ["python3", str(args.powerlifted), "-d", str(domain), "-i", str(instance),
            "-s", "alt-bfws1", "-e", "ff", "-g", "yannakakis", "--unit-cost",
            "--time-limit", str(args.timeout), "--plan-file", str(plan),
            "--translator-output-file", str(args.output / f"pwl-p{problem}-r{run}.lifted")]


def measure(args, solver, problem, run):
    def cap_resources():
        memory = int(args.memory_gib * 1024**3)
        resource.setrlimit(resource.RLIMIT_AS, (memory, memory))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    start = time.perf_counter()
    process = subprocess.Popen(command(args, solver, problem, run), cwd=args.output,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                               start_new_session=True, preexec_fn=cap_resources)
    try:
        stdout, stderr = process.communicate(timeout=args.timeout)
        status = "solved" if process.returncode == 0 else f"exit_{process.returncode}"
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
        status = "timeout"
    wall = time.perf_counter() - start
    log = stdout + "\n" + stderr
    (args.output / f"{solver}-p{problem}-r{run}.log").write_text(log)
    pattern = r"Plan \((\d+) steps?\)" if solver.startswith("folplan") else r"Plan length: (\d+) step"
    match = re.search(pattern, log)
    length = int(match.group(1)) if match else ""
    if status == "solved" and length == "":
        status = "no_plan_reported"
    return {"problem": problem, "solver": solver, "run": run, "status": status,
            "wall_s": round(wall, 6), "plan_length": length, "exit_code": process.returncode}


def main():
    args = arguments()
    with (args.output / "htg-visitall-raw.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=["problem", "solver", "run", "status",
                                                   "wall_s", "plan_length", "exit_code"])
        writer.writeheader()
        failed = set()
        for problem in args.cases:
            for run in range(1, args.runs + 1):
                rotated = SOLVERS[run % len(SOLVERS):] + SOLVERS[:run % len(SOLVERS)]
                for solver in rotated:
                    if (problem, solver) in failed:
                        continue
                    row = measure(args, solver, problem, run)
                    writer.writerow(row)
                    stream.flush()
                    print(row, flush=True)
                    if row["status"] != "solved":
                        failed.add((problem, solver))


if __name__ == "__main__":
    main()
