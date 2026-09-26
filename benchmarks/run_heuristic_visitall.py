#!/usr/bin/env python3
"""Compare folplan A*, folplan BFS, and pinned Powerlifted on HTG VisitAll."""

import argparse
import csv
import hashlib
import json
import os
import platform
import re
import resource
import signal
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path


DEFAULT_CASES = (0, 1, 2, 3, 4, 5, 8, 9)
SOLVERS = ("folplan_astar", "folplan_bfs", "powerlifted")
OBJECTS = {0: 6, 1: 8, 2: 10, 3: 12, 4: 14, 5: 16, 8: 22, 9: 24}
PWL_COMMIT = "bf54e169e782465b9bf74aa49424a6ffd8252838"
PWL_PATCH = Path(__file__).with_name("powerlifted-gcc16-compat.patch")


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--instances", type=Path, required=True,
                        help="Pinned HTG 5-dim-visitall-CLOSE-g1 directory")
    parser.add_argument("--folplan", type=Path, required=True,
                        help="Release folplan binary from the revision under test")
    parser.add_argument("--powerlifted", type=Path, required=True,
                        help="Powerlifted powerlifted.py from clean pinned worktree plus compatibility patch")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cases", default=",".join(map(str, DEFAULT_CASES)))
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--timeout", type=int, default=15)
    parser.add_argument("--memory-gib", type=float, default=1)
    parser.add_argument("--max-states", type=int, default=100_000)
    parser.add_argument("--max-ground-actions", type=int, default=1_000_000)
    parser.add_argument("--folplan-revision", required=True,
                        help="Git revision used to build the folplan binary")
    args = parser.parse_args()
    if args.runs < 1 or args.timeout <= 0 or args.memory_gib <= 0:
        parser.error("runs, timeout, and memory-gib must be positive")
    if args.max_states <= 0 or args.max_ground_actions <= 0:
        parser.error("state and ground-action limits must be positive")
    try:
        args.cases = tuple(int(value) for value in args.cases.split(","))
    except ValueError:
        parser.error("cases must be comma-separated problem indices")
    if not args.cases or any(index not in OBJECTS for index in args.cases):
        parser.error("cases must be a nonempty subset of 0,1,2,3,4,5,8,9")
    for name in ("instances", "folplan", "powerlifted", "output"):
        setattr(args, name, getattr(args, name).resolve())
    for path in (args.instances / "domain.pddl",
                 *(args.instances / f"p{i}.pddl" for i in args.cases),
                 args.folplan, args.powerlifted, PWL_PATCH):
        if not path.is_file():
            parser.error(f"missing input: {path}")
    if not os.access(args.folplan, os.X_OK):
        parser.error(f"folplan binary is not executable: {args.folplan}")
    args.output.mkdir(parents=True, exist_ok=True)
    return args


def file_sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def cap_resources(memory_gib):
    memory = int(memory_gib * 1024**3)
    resource.setrlimit(resource.RLIMIT_AS, (memory, memory))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def command(args, solver, problem, run):
    domain = args.instances / "domain.pddl"
    instance = args.instances / f"p{problem}.pddl"
    tag = f"{solver}-p{problem}-r{run}"
    if solver.startswith("folplan"):
        algorithm = "astar" if solver.endswith("astar") else "bfs"
        return [str(args.folplan), "pddl", str(domain), str(instance),
                "--search", algorithm,
                "--max-states", str(args.max_states),
                "--max-ground-actions", str(args.max_ground_actions)]
    return ["python3", str(args.powerlifted), "-d", str(domain), "-i", str(instance),
            "-s", "alt-bfws1", "-e", "ff", "-g", "yannakakis", "--unit-cost",
            "--time-limit", str(args.timeout), "--plan-file", str(args.output / f"{tag}.plan"),
            "--translator-output-file", str(args.output / f"{tag}.lifted")]


def classify(solver, returncode, timed_out, log):
    if timed_out:
        return "timeout"
    lower = log.lower()
    if solver == "powerlifted" and "time limit" in lower and "solution found." not in lower:
        return "planner_timeout"
    if any(text in lower for text in ("std::bad_alloc", "cannot allocate memory",
                                      "memory allocation", "out of memory")):
        return "memory_limit"
    if returncode == 0:
        return "solved"
    if returncode < 0:
        try:
            return f"signal_{signal.Signals(-returncode).name}"
        except ValueError:
            return f"signal_{-returncode}"
    if solver == "powerlifted" and returncode == 124:
        return "planner_timeout"
    return f"exit_{returncode}"


def measure(args, solver, problem, run):
    def child_setup():
        cap_resources(args.memory_gib)

    start = time.perf_counter()
    process = subprocess.Popen(command(args, solver, problem, run), cwd=args.output,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                               start_new_session=True, preexec_fn=child_setup)
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=args.timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
    wall = time.perf_counter() - start
    log = stdout + "\n" + stderr
    tag = f"{solver}-p{problem}-r{run}"
    (args.output / f"{tag}.log").write_text(log)
    if solver.startswith("folplan"):
        length_match = re.search(r"Plan \((\d+) steps?\)", log)
        expanded_match = re.search(r"States explored: (\d+)", log)
        internal_match = None
    else:
        length_match = re.search(r"Plan length: (\d+) step", log)
        expanded_match = re.search(r"Expanded (\d+) state", log)
        internal_match = re.search(r"Total time: ([0-9.]+)", log)
    length = int(length_match.group(1)) if length_match else ""
    expanded = int(expanded_match.group(1)) if expanded_match else ""
    internal = float(internal_match.group(1)) if internal_match else ""
    status = classify(solver, process.returncode, timed_out, log)
    if status == "solved" and (length == "" or (solver == "powerlifted" and "Solution found." not in log)):
        status = "no_plan_reported"
    return {"problem": problem, "objects": OBJECTS[problem], "solver": solver, "run": run,
            "status": status, "wall_s": round(wall, 6), "plan_length": length,
            "expanded": expanded, "internal_search_s": internal,
            "exit_code": process.returncode}


def write_metadata(args):
    metadata = {
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "host": platform.node(),
        "platform": platform.platform(),
        "processor": platform.processor(),
        "cpu_model": next((line.split(":", 1)[1].strip()
                           for line in Path("/proc/cpuinfo").read_text().splitlines()
                           if line.startswith("model name")), "unknown"),
        "folplan_revision": args.folplan_revision,
        "folplan_sha256": file_sha256(args.folplan),
        "htg_commit": "c5670ed289f58d3d6e104d7b06f311cf2f87b354",
        "powerlifted_commit": PWL_COMMIT,
        "powerlifted_compatibility_patch": str(PWL_PATCH.name),
        "powerlifted_compatibility_patch_sha256": file_sha256(PWL_PATCH),
        "powerlifted_build": "Release; GCC 16.2.1; CXXFLAGS=-Wno-error=stringop-overflow; build.py",
        "powerlifted_patch_scope": "std::allocator_traits compatibility only; no search or heuristic changes",
        "cases": list(args.cases),
        "runs": args.runs,
        "timeout_s": args.timeout,
        "address_space_limit_gib": args.memory_gib,
        "max_states": args.max_states,
        "max_ground_actions": args.max_ground_actions,
    }
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")


def summarize(args, rows):
    path = args.output / "astar-bfs-powerlifted-visitall-summary.csv"
    fields = ["instance", "objects", "solver", "outcome", "successful_runs", "attempted_runs",
              "requested_runs",
              "median_wall_s", "min_wall_s", "max_wall_s", "median_expanded",
              "median_internal_search_s", "plan_length"]
    with path.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        for problem in args.cases:
            for solver in SOLVERS:
                matching = [row for row in rows if row["problem"] == problem and row["solver"] == solver]
                solved = [row for row in matching if row["status"] == "solved"]
                failures = sorted({row["status"] for row in matching if row["status"] != "solved"})
                if solved and not failures:
                    outcome = "solved"
                elif solved:
                    outcome = "mixed_" + "+".join(failures)
                else:
                    outcome = failures[0] if failures else "not_run"
                times = [row["wall_s"] for row in solved]
                expansions = [row["expanded"] for row in solved if row["expanded"] != ""]
                internal = [row["internal_search_s"] for row in solved
                            if row["internal_search_s"] != ""]
                lengths = sorted({row["plan_length"] for row in solved})
                writer.writerow({
                    "instance": f"p{problem}", "objects": OBJECTS[problem], "solver": solver,
                    "outcome": outcome, "successful_runs": len(solved),
                    "attempted_runs": len(matching), "requested_runs": args.runs,
                    "median_wall_s": f"{sorted(times)[len(times) // 2]:.6f}" if times else "",
                    "min_wall_s": f"{min(times):.6f}" if times else "",
                    "max_wall_s": f"{max(times):.6f}" if times else "",
                    "median_expanded": sorted(expansions)[len(expansions) // 2] if expansions else "",
                    "median_internal_search_s": f"{sorted(internal)[len(internal) // 2]:.6f}" if internal else "",
                    "plan_length": lengths[0] if len(lengths) == 1 else "",
                })


def main():
    args = parse_args()
    write_metadata(args)
    raw = args.output / "astar-bfs-powerlifted-visitall-raw.csv"
    fields = ["problem", "objects", "solver", "run", "status", "wall_s",
              "plan_length", "expanded", "internal_search_s", "exit_code"]
    rows = []
    failed = set()
    with raw.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        for problem in args.cases:
            for run in range(1, args.runs + 1):
                shift = (run + problem) % len(SOLVERS)
                rotated = SOLVERS[shift:] + SOLVERS[:shift]
                for solver in rotated:
                    if (problem, solver) in failed:
                        continue
                    row = measure(args, solver, problem, run)
                    writer.writerow(row)
                    stream.flush()
                    rows.append(row)
                    print(row, flush=True)
                    if row["status"] != "solved":
                        failed.add((problem, solver))
    summarize(args, rows)


if __name__ == "__main__":
    main()
