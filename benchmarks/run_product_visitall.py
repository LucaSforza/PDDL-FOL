#!/usr/bin/env python3
"""Run the preregistered 54-task product-VisitAll planner comparison.

Each successful solver/task cell receives three interleaved attempts. A cell
stops after its first failure, so failures are attempted once in total.
"""

import argparse
import csv
import hashlib
import json
import os
import platform
import re
import resource
import shlex
import signal
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TYR = Path.home() / ".local/bin/gbfs_lazy"
DEFAULT_POWERLIFTED = Path("/tmp/powerlifted-pinned-bf54e169/powerlifted.py")
HTG_COMMIT = "c5670ed289f58d3d6e104d7b06f311cf2f87b354"
POWERLIFTED_COMMIT = "bf54e169e782465b9bf74aa49424a6ffd8252838"
TYR_COMMIT = "e0ea47a328e63ae2f35e1fc1d1b69c63d5505279"
POWERLIFTED_PATCH_SHA256 = "85734a150f3a9bfc23984ff3c66f56ec8d5cdbcb1c7b466b0f74bde8468ba414"
POWERLIFTED_PATCH = ROOT / "benchmarks/powerlifted-gcc16-compat.patch"
TIMEOUT_S = 15
MEMORY_BYTES = 1024**3
CASES = (0, 5, 9)
SOLVERS = ("folplan_astar", "powerlifted", "tyr")


def families():
    return [(dimension, distance, goal)
            for dimension in (3, 4, 5)
            for distance in ("CLOSE", "FAR")
            for goal in (1, 2, 3)]


def family_name(family):
    dimension, distance, goal = family
    return f"{dimension}-dim-visitall-{distance}-g{goal}"


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--instances-root", type=Path, required=True,
                        help="Root containing the 18 pinned HTG family directories")
    parser.add_argument("--folplan", type=Path, required=True,
                        help="Optimized folplan binary")
    parser.add_argument("--folplan-revision", required=True,
                        help="Exact source revision used to build folplan")
    parser.add_argument("--corpus-revision", default=HTG_COMMIT,
                        help="Pinned HTG Git revision")
    parser.add_argument("--powerlifted", type=Path, default=DEFAULT_POWERLIFTED)
    parser.add_argument("--tyr", type=Path, default=DEFAULT_TYR)
    parser.add_argument("--powerlifted-python", default=sys.executable)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--oracle-csv", type=Path,
                        help="Optional external oracle CSV for secondary plan-length checks")
    parser.add_argument("--dry-run", action="store_true",
                        help="Write preregistration/metadata and print commands without starting solvers")
    args = parser.parse_args()
    for name in ("instances_root", "folplan", "powerlifted", "tyr", "output"):
        setattr(args, name, getattr(args, name).expanduser().resolve())
    if args.oracle_csv:
        args.oracle_csv = args.oracle_csv.expanduser().resolve()
        if not args.oracle_csv.is_file():
            parser.error(f"oracle CSV does not exist: {args.oracle_csv}")
    if not args.folplan.is_file() or not os.access(args.folplan, os.X_OK):
        parser.error(f"folplan binary is missing or not executable: {args.folplan}")
    for path in (args.powerlifted, args.tyr):
        if not path.is_file():
            parser.error(f"missing solver binary/script: {path}")
    if not POWERLIFTED_PATCH.is_file() or sha256(POWERLIFTED_PATCH) != POWERLIFTED_PATCH_SHA256:
        parser.error(f"Powerlifted compatibility patch does not match pinned SHA256: {POWERLIFTED_PATCH}")
    missing = []
    for family in families():
        directory = args.instances_root / family_name(family)
        for name in ("domain.pddl", *(f"p{case}.pddl" for case in CASES)):
            if not (directory / name).is_file():
                missing.append(directory / name)
    if missing:
        parser.error(f"missing {len(missing)} required corpus files; first: {missing[0]}")
    args.output.mkdir(parents=True, exist_ok=True)
    return args


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def preregistration_rows(args):
    rows = []
    for family in families():
        directory = args.instances_root / family_name(family)
        for case in CASES:
            rows.append({
                "family": family_name(family), "dimension": family[0],
                "distance": family[1], "goal_group": f"g{family[2]}",
                "problem": f"p{case}",
                "domain_sha256": sha256(directory / "domain.pddl"),
                "problem_sha256": sha256(directory / f"p{case}.pddl"),
                "attempt_policy": "3 on success; stop after first failure (1 total)",
                "timeout_s": TIMEOUT_S, "address_space_limit_bytes": MEMORY_BYTES,
            })
    return rows


def write_csv(path, rows, fields):
    with path.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def oracle_lengths(path):
    if path is None:
        return {}
    lengths = {}
    with path.open(newline="") as stream:
        for row in csv.DictReader(stream):
            family = row.get("family") or row.get("directory") or ""
            family = family.removeprefix("d-")
            problem = row.get("problem") or row.get("instance") or ""
            match = re.search(r"(?:^|/)p?(\d+)(?:\.pddl)?$", problem)
            length = row.get("optimal_length") or row.get("plan_length") or row.get("length")
            if family and match and length not in (None, ""):
                lengths[(family, f"p{int(match.group(1))}")] = int(length)
    return lengths


def command(args, solver, family, problem, attempt):
    directory = args.instances_root / family_name(family)
    domain, instance = directory / "domain.pddl", directory / f"{problem}.pddl"
    tag = f"{solver}-{family_name(family)}-{problem}-r{attempt}"
    plan = args.output / f"{tag}.plan"
    if solver == "folplan_astar":
        return [str(args.folplan), "pddl", str(domain), str(instance), "--search", "astar",
                "--max-states", "100000", "--max-ground-actions", "1000000"]
    if solver == "powerlifted":
        return [args.powerlifted_python, str(args.powerlifted), "-d", str(domain), "-i", str(instance),
                "-s", "alt-bfws1", "-e", "ff", "-g", "yannakakis", "--unit-cost",
                "--time-limit", str(TIMEOUT_S), "--plan-file", str(plan),
                "--translator-output-file", str(args.output / f"{tag}.lifted")]
    return [str(args.tyr), "-D", str(domain), "-P", str(instance), "-O", str(plan),
            "-N", "1", "-M", "1", "-H", "rpg_ff", "--heuristic-cost-type", "unit",
            "--search-cost-type", "unit", "-V", "0"]


def classify(solver, returncode, timed_out, log):
    lower = log.lower()
    if timed_out:
        return "timeout"
    if solver == "powerlifted" and "time limit" in lower and "solution found." not in lower:
        return "planner_timeout"
    if any(text in lower for text in ("std::bad_alloc", "cannot allocate memory",
                                      "memory allocation", "out of memory",
                                      "memory limit has been reached")):
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


def measure(args, solver, family, problem, attempt):
    cmd = command(args, solver, family, problem, attempt)
    tag = f"{solver}-{family_name(family)}-{problem}-r{attempt}"
    plan_path = args.output / f"{tag}.plan"
    plan_path.unlink(missing_ok=True)

    def cap_resources():
        resource.setrlimit(resource.RLIMIT_AS, (MEMORY_BYTES, MEMORY_BYTES))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    start = time.perf_counter()
    try:
        process = subprocess.Popen(cmd, cwd=args.output, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True, start_new_session=True,
                                   preexec_fn=cap_resources)
    except OSError as error:
        wall = time.perf_counter() - start
        log = f"{type(error).__name__}: {error}"
        process_returncode = None
        status = "launch_error"
        stdout = stderr = ""
    else:
        timed_out = False
        try:
            stdout, stderr = process.communicate(timeout=TIMEOUT_S)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        wall = time.perf_counter() - start
        log = stdout + "\n" + stderr
        process_returncode = process.returncode
        status = classify(solver, process.returncode, timed_out, log)

    (args.output / f"{tag}.log").write_text(log)
    length = ""
    if solver == "folplan_astar":
        match = re.search(r"Plan \((\d+) steps?\)", log)
        length = int(match.group(1)) if match else ""
        if status == "solved" and length == "":
            status = "no_plan_reported"
    elif solver == "powerlifted":
        match = re.search(r"Plan length: (\d+) step", log)
        length = int(match.group(1)) if match else ""
        if status == "solved" and (length == "" or "Solution found." not in log):
            status = "no_plan_reported"
    else:
        length = (sum(line.strip().startswith("(") for line in plan_path.read_text().splitlines())
                  if plan_path.is_file() else "")
        if status == "solved" and not plan_path.is_file():
            status = "no_plan_reported"
    return {
        "family": family_name(family), "dimension": family[0], "distance": family[1],
        "goal_group": f"g{family[2]}", "problem": problem, "solver": solver,
        "attempt": attempt, "status": status, "wall_s": f"{wall:.6f}",
        "plan_length": length, "exit_code": process_returncode,
        "command": shlex.join(cmd),
        "plan_validation": ("optimized plan replayed internally by folplan"
                            if solver == "folplan_astar" else
                            "not independently validated; length parsed from solver output/plan file"),
    }


def summary_rows(raw_rows, oracle):
    result = []
    for family in families():
        family_label = family_name(family)
        for case in CASES:
            problem = f"p{case}"
            for solver in SOLVERS:
                matching = [row for row in raw_rows if row["family"] == family_label
                            and row["problem"] == problem and row["solver"] == solver]
                solved = [row for row in matching if row["status"] == "solved"]
                times = sorted(float(row["wall_s"]) for row in solved)
                lengths = sorted({int(row["plan_length"]) for row in solved
                                  if row["plan_length"] != ""})
                failures = sorted({row["status"] for row in matching if row["status"] != "solved"})
                if solved and not failures:
                    outcome = "solved"
                elif solved:
                    outcome = "mixed_" + "+".join(failures)
                else:
                    outcome = failures[0] if failures else "not_run"
                expected = oracle.get((family_label, problem), "")
                median_len = lengths[0] if len(lengths) == 1 else ""
                result.append({
                    "family": family_label, "dimension": family[0], "distance": family[1],
                    "goal_group": f"g{family[2]}", "problem": problem, "solver": solver,
                    "outcome": outcome, "successful_runs": len(solved),
                    "attempted_runs": len(matching), "requested_runs": 3,
                    "median_wall_s": f"{statistics.median(times):.6f}" if times else "",
                    "min_wall_s": f"{min(times):.6f}" if times else "",
                    "max_wall_s": f"{max(times):.6f}" if times else "",
                    "plan_length": median_len,
                    "oracle_plan_length": expected,
                    "matches_oracle_length": (str(median_len == expected).lower()
                                               if expected != "" and median_len != "" else ""),
                })
    return result


def family_summary_rows(task_rows):
    result = []
    for family in families():
        label = family_name(family)
        for solver in SOLVERS:
            matching = [row for row in task_rows
                        if row["family"] == label and row["solver"] == solver]
            solved = [row for row in matching if int(row["successful_runs"]) > 0]
            times = sorted(float(row["median_wall_s"]) for row in solved
                           if row["median_wall_s"] != "")
            lengths = {row["problem"]: row["plan_length"] for row in solved
                       if row["plan_length"] != ""}
            statuses = {row["problem"]: row["outcome"] for row in matching}
            result.append({
                "family": label, "dimension": family[0], "distance": family[1],
                "goal_group": f"g{family[2]}", "solver": solver,
                "solved_tasks": len(solved), "total_tasks": len(CASES),
                "successful_runs": sum(int(row["successful_runs"]) for row in matching),
                "attempted_runs": sum(int(row["attempted_runs"]) for row in matching),
            "median_task_wall_s": f"{statistics.median(times):.6f}" if times else "",
                "plan_lengths_p0_p5_p9": ";".join(
                    f"p{case}={lengths.get(f'p{case}', '')}" for case in CASES),
                "outcomes_p0_p5_p9": ";".join(
                    f"p{case}={statuses.get(f'p{case}', 'not_run')}" for case in CASES),
            })
    return result


RAW_FIELDS = ("family", "dimension", "distance", "goal_group", "problem", "solver",
              "attempt", "status", "wall_s", "plan_length", "exit_code", "command",
              "plan_validation")
SUMMARY_FIELDS = ("family", "dimension", "distance", "goal_group", "problem", "solver",
                  "outcome", "successful_runs", "attempted_runs", "requested_runs",
                  "median_wall_s", "min_wall_s", "max_wall_s", "plan_length",
                  "oracle_plan_length", "matches_oracle_length")
FAMILY_SUMMARY_FIELDS = ("family", "dimension", "distance", "goal_group", "solver",
                         "solved_tasks", "total_tasks", "successful_runs", "attempted_runs",
                         "median_task_wall_s", "plan_lengths_p0_p5_p9", "outcomes_p0_p5_p9")


def main():
    args = parse_args()
    prereg = preregistration_rows(args)
    write_csv(args.output / "preregistration.csv", prereg,
              tuple(prereg[0].keys()) if prereg else ())
    oracle = oracle_lengths(args.oracle_csv)
    metadata = {
        "protocol": "official HTG product-VisitAll families, cases p0/p5/p9; 3 interleaved successes or 1 attempt after first failure",
        "created_utc": datetime.now(timezone.utc).isoformat(),
        "host": platform.node(), "platform": platform.platform(),
        "cpu_model": next((line.split(":", 1)[1].strip()
                           for line in Path("/proc/cpuinfo").read_text().splitlines()
                           if line.startswith("model name")), "unknown"),
        "corpus_revision": args.corpus_revision,
        "families": [family_name(family) for family in families()],
        "problems_per_family": list(CASES), "task_count": len(prereg),
        "solvers": {
            "folplan_astar": {"revision": args.folplan_revision,
                              "path": str(args.folplan), "sha256": sha256(args.folplan)},
            "powerlifted": {"revision": POWERLIFTED_COMMIT,
                            "path": str(args.powerlifted), "sha256": sha256(args.powerlifted),
                            "compatibility_patch_sha256": POWERLIFTED_PATCH_SHA256,
                            "search": "alt-bfws1 + FF + Yannakakis + unit cost"},
            "tyr": {"revision": TYR_COMMIT, "path": str(args.tyr), "sha256": sha256(args.tyr),
                    "search": "single-core GBFS + RPG-FF + unit cost"},
        },
        "timeout_s": TIMEOUT_S, "address_space_limit_bytes": MEMORY_BYTES,
        "plan_validation": "No independent validation of competitor plans; output records returned lengths only.",
        "powerlifted_compatibility_patch": str(POWERLIFTED_PATCH),
        "oracle_csv": str(args.oracle_csv) if args.oracle_csv else None,
        "oracle_csv_sha256": sha256(args.oracle_csv) if args.oracle_csv else None,
        "dry_run": args.dry_run,
    }
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    raw_path = args.output / "raw.csv"
    if args.dry_run:
        for family in families():
            for case in CASES:
                for solver in SOLVERS:
                    print(shlex.join(command(args, solver, family, f"p{case}", 1)))
        write_csv(raw_path, [], RAW_FIELDS)
        task_rows = summary_rows([], oracle)
        write_csv(args.output / "summary.csv", task_rows, SUMMARY_FIELDS)
        write_csv(args.output / "family_summary.csv", family_summary_rows(task_rows),
                  FAMILY_SUMMARY_FIELDS)
        return

    rows = []
    failed = set()
    task_cells = [(family, f"p{case}") for family in families() for case in CASES]
    with raw_path.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=RAW_FIELDS, lineterminator="\n")
        writer.writeheader()
        for attempt in range(1, 4):
            # Rotate the initial solver order per task and sweep through all tasks
            # each round, keeping successful repeat attempts interleaved.
            for task_index, (family, problem) in enumerate(task_cells):
                shift = task_index % len(SOLVERS)
                solver_order = SOLVERS[shift:] + SOLVERS[:shift]
                for solver in solver_order:
                    key = (family_name(family), problem, solver)
                    if key in failed:
                        continue
                    row = measure(args, solver, family, problem, attempt)
                    writer.writerow(row)
                    stream.flush()
                    rows.append(row)
                    print(row, flush=True)
                    if row["status"] != "solved":
                        failed.add(key)
    task_rows = summary_rows(rows, oracle)
    write_csv(args.output / "summary.csv", task_rows, SUMMARY_FIELDS)
    write_csv(args.output / "family_summary.csv", family_summary_rows(task_rows),
              FAMILY_SUMMARY_FIELDS)


if __name__ == "__main__":
    main()
