"""Reproduce the Tyr single-core GBFS + RPG-FF HTG VisitAll comparison (Linux only)."""

import argparse
import csv
import os
import resource
import shlex
import signal
import subprocess
import time
from pathlib import Path


CASES = (0, 1, 2, 3, 4, 5, 8, 9)
RUNS = 5
TIMEOUT_S = 15
MEMORY_BYTES = 1024**3
SOLVER = "tyr_gbfs_rpg_ff_1core"
FIELDS = ("problem", "solver", "run", "status", "wall_s", "plan_length", "exit_code", "command")


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--instances", type=Path, required=True,
                        help="HTG 5-dim-visitall-CLOSE-g1 directory")
    parser.add_argument("--tyr", type=Path, required=True, help="Built Tyr gbfs_lazy executable")
    parser.add_argument("--output", type=Path, required=True, help="Directory for the raw CSV")
    args = parser.parse_args()
    for name in ("instances", "tyr", "output"):
        setattr(args, name, getattr(args, name).resolve())
    for path in (args.instances / "domain.pddl",
                 *(args.instances / f"p{i}.pddl" for i in CASES), args.tyr):
        if not path.is_file():
            parser.error(f"missing input: {path}")
    args.output.mkdir(parents=True, exist_ok=True)
    return args


def command(args, problem, run):
    plan = args.output / f"{SOLVER}-p{problem}-r{run}.plan"
    return [str(args.tyr), "-D", str(args.instances / "domain.pddl"),
            "-P", str(args.instances / f"p{problem}.pddl"), "-O", str(plan),
            "-N", "1", "-M", "1", "-H", "rpg_ff",
            "--heuristic-cost-type", "unit", "--search-cost-type", "unit", "-V", "0"]


def measure(args, problem, run):
    cmd = command(args, problem, run)
    plan = args.output / f"{SOLVER}-p{problem}-r{run}.plan"
    log_path = args.output / f"{SOLVER}-p{problem}-r{run}.log"
    plan.unlink(missing_ok=True)

    def cap_resources():
        resource.setrlimit(resource.RLIMIT_AS, (MEMORY_BYTES, MEMORY_BYTES))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    start = time.perf_counter()
    process = subprocess.Popen(cmd, cwd=args.output, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, start_new_session=True,
                               preexec_fn=cap_resources)
    try:
        stdout, stderr = process.communicate(timeout=TIMEOUT_S)
        status = "solved" if process.returncode == 0 and plan.is_file() else f"exit_{process.returncode}"
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
        status = "timeout"

    wall = time.perf_counter() - start
    log = stdout + "\n" + stderr
    plan_length = sum(line.strip().startswith("(") for line in plan.read_text().splitlines()) if plan.is_file() else ""
    if status == "solved" and plan_length == "":
        status = "no_plan_reported"
    if status == "solved":
        plan.unlink(missing_ok=True)
    else:
        log_path.write_text(log)

    return {"problem": problem, "solver": SOLVER, "run": run, "status": status,
            "wall_s": f"{wall:.6f}", "plan_length": plan_length,
            "exit_code": process.returncode, "command": shlex.join(cmd)}


def main():
    args = arguments()
    csv_path = args.output / "htg-visitall-raw.csv"
    failed = set()
    with csv_path.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=FIELDS)
        writer.writeheader()
        for run in range(1, RUNS + 1):
            order = CASES[run - 1:] + CASES[:run - 1]
            for problem in order:
                if problem in failed:
                    continue
                row = measure(args, problem, run)
                writer.writerow(row)
                stream.flush()
                print(row, flush=True)
                if row["status"] != "solved":
                    failed.add(problem)


if __name__ == "__main__":
    main()
