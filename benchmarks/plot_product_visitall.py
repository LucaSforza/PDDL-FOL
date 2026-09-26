#!/usr/bin/env python3
"""Plot coverage/timing and per-task outcomes for product-VisitAll runs."""

import argparse
import csv
from pathlib import Path

import matplotlib.pyplot as plt
from matplotlib.colors import ListedColormap
from matplotlib.patches import Patch


SOLVERS = ("folplan_astar", "powerlifted", "tyr")
LABELS = {"folplan_astar": "folplan A*", "powerlifted": "Powerlifted", "tyr": "Tyr"}
COLORS = {"folplan_astar": "#1769aa", "powerlifted": "#16805d", "tyr": "#d97706"}
TASK_ORDER = ("p0", "p5", "p9")
STATUS_CODES = {"solved": 0, "timeout": 1, "planner_timeout": 1, "memory_limit": 2,
                "no_plan_reported": 3, "launch_error": 4}
STATUS_COLORS = ["#d9efe1", "#b42318", "#7b1e3b", "#d6dbe1", "#6b7280", "#eeeeee"]


def read_csv(path):
    with path.open(newline="") as stream:
        return list(csv.DictReader(stream))


def status_code(outcome):
    if outcome == "solved":
        return 0, "OK"
    if outcome in ("timeout", "planner_timeout"):
        return 1, "TO"
    if outcome == "memory_limit":
        return 2, "MEM"
    if outcome == "no_plan_reported":
        return 3, "NOP"
    if outcome == "not_run":
        return 5, "—"
    return 4, "ERR"


def plot_compact(summary, output):
    plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 9,
                         "axes.titlesize": 11, "axes.labelsize": 9,
                         "legend.fontsize": 8, "figure.dpi": 150})
    figure, (cactus_axis, coverage_axis) = plt.subplots(
        2, 1, figsize=(7.0, 9.9), gridspec_kw={"height_ratios": (1.25, 1)},
        constrained_layout=True)
    for solver in SOLVERS:
        times = sorted(float(row["median_wall_s"]) for row in summary
                       if row["solver"] == solver and int(row["successful_runs"]) > 0
                       and row["median_wall_s"])
        cactus_axis.step(range(1, len(times) + 1), times, where="post",
                         color=COLORS[solver], linewidth=2,
                         label=f"{LABELS[solver]} ({len(times)}/54)")
    cactus_axis.set_yscale("log")
    cactus_axis.set_xlim(1, 54)
    cactus_axis.set_xticks((1, 10, 20, 30, 40, 50, 54))
    cactus_axis.set_xlabel("Tasks solved, ordered by median wall time")
    cactus_axis.set_ylabel("Median full-process time (s, log scale)")
    cactus_axis.set_title("Runtime and solved-task coverage")
    cactus_axis.grid(True, which="both", axis="y", color="#d6dbe1", linewidth=0.6)
    cactus_axis.legend(loc="upper left", frameon=True, ncol=1)

    dimensions = (3, 4, 5)
    group_centers = list(range(len(dimensions)))
    bar_width = 0.23
    for offset, solver in enumerate(SOLVERS):
        coverages = [sum(int(row["dimension"]) == dimension
                         and int(row["successful_runs"]) > 0
                         for row in summary if row["solver"] == solver)
                     for dimension in dimensions]
        positions = [center + (offset - 1) * bar_width for center in group_centers]
        bars = coverage_axis.bar(positions, coverages, width=bar_width,
                                 color=COLORS[solver], label=LABELS[solver], zorder=3)
        for bar, count in zip(bars, coverages):
            coverage_axis.annotate(f"{count}/18", (bar.get_x() + bar.get_width() / 2, count),
                                   xytext=(0, 3), textcoords="offset points",
                                   ha="center", va="bottom", fontsize=8)
    coverage_axis.set_xticks(group_centers, [f"{dimension}D" for dimension in dimensions])
    coverage_axis.set_ylim(0, 21)
    coverage_axis.set_yticks((0, 5, 10, 15, 18))
    coverage_axis.set_ylabel("Tasks solved (of 18)")
    coverage_axis.set_xlabel("VisitAll dimension")
    coverage_axis.set_title("Coverage by dimension")
    coverage_axis.grid(True, axis="y", color="#d6dbe1", linewidth=0.6, zorder=0)
    coverage_axis.legend(loc="lower right", frameon=True, ncol=1)

    figure.suptitle("Product-VisitAll planner comparison", fontsize=14, weight="bold")
    figure.text(0.01, -0.005,
                "54 tasks: 3 dimensions × CLOSE/FAR × g1–g3 × p0/p5/p9. A failed task/solver cell is attempted once; successful cells receive three runs. "
                "Plans from Powerlifted and Tyr were not independently validated.",
                ha="left", va="top", fontsize=7.5, color="#454b54", wrap=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(output, dpi=300, bbox_inches="tight", facecolor="white")
    plt.close(figure)
    print(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--raw", type=Path, required=True)
    parser.add_argument("--output", type=Path,
                        default=Path(__file__).resolve().parents[1] /
                        "docs/figures/product-visitall-comparison.png")
    parser.add_argument("--compact-output", type=Path,
                        default=Path(__file__).resolve().parents[1] /
                        "docs/figures/product-visitall-compact.png")
    args = parser.parse_args()
    summary = read_csv(args.summary)
    raw = read_csv(args.raw)
    families = sorted({row["family"] for row in summary},
                      key=lambda family: (int(family.split("-", 1)[0]),
                                          family.split("-")[3], int(family.split("g")[-1])))
    family_map = {(row["family"], row["problem"], row["solver"]): row for row in summary}
    tasks = [(family, problem) for family in families for problem in TASK_ORDER]
    raw_map = {}
    for row in raw:
        raw_map.setdefault((row["family"], row["problem"], row["solver"]), []).append(row)

    plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 8,
                         "axes.titlesize": 11, "axes.labelsize": 9,
                         "legend.fontsize": 8, "figure.dpi": 150})
    figure, (cactus_axis, heat_axis) = plt.subplots(
        2, 1, figsize=(15, 12), gridspec_kw={"height_ratios": (1, 2)},
        constrained_layout=True)

    for solver in SOLVERS:
        times = sorted(float(row["median_wall_s"]) for row in summary
                       if row["solver"] == solver and int(row["successful_runs"]) > 0
                       and row["median_wall_s"])
        cactus_axis.step(range(1, len(times) + 1), times, where="post",
                         color=COLORS[solver], linewidth=1.8, label=f"{LABELS[solver]} ({len(times)}/54)")
    cactus_axis.set_yscale("log")
    cactus_axis.set_xlim(1, 54)
    cactus_axis.set_xlabel("Solved tasks, ordered by median full-process wall time")
    cactus_axis.set_ylabel("Median wall time (seconds, log scale)")
    cactus_axis.set_title("Coverage and runtime across the preregistered 54-task set")
    cactus_axis.grid(True, which="both", axis="y", color="#d6dbe1", linewidth=0.6)
    cactus_axis.legend(loc="upper left", ncol=3, frameon=True)

    codes, annotations = [], []
    for family, problem in tasks:
        code_row, annotation_row = [], []
        for solver in SOLVERS:
            row = family_map.get((family, problem, solver))
            outcome = row["outcome"] if row else "not_run"
            attempts = raw_map.get((family, problem, solver), [])
            failures = [attempt["status"] for attempt in attempts if attempt["status"] != "solved"]
            has_success = row and int(row["successful_runs"]) > 0
            if has_success:
                code = 0
                text = f"{int(row['successful_runs'])}/{int(row['requested_runs'])}"
                if failures:
                    code, failure_text = status_code(failures[-1])
                    text = f"{text} {failure_text}"
                elif row["median_wall_s"]:
                    text = f"{float(row['median_wall_s']):.2g}s"
            else:
                code, text = status_code(outcome)
            code_row.append(code)
            annotation_row.append(text)
        codes.append(code_row)
        annotations.append(annotation_row)
    heat_axis.imshow(codes, aspect="auto", interpolation="nearest",
                     cmap=ListedColormap(STATUS_COLORS), vmin=0, vmax=len(STATUS_COLORS) - 1)
    heat_axis.set_xticks(range(len(SOLVERS)), [LABELS[solver] for solver in SOLVERS])
    heat_axis.set_yticks(range(len(tasks)), [f"{family} · {problem}" for family, problem in tasks],
                         fontsize=6)
    heat_axis.set_title("Per-task status; solved cells show median seconds")
    heat_axis.set_xlabel("Planner")
    for y, row in enumerate(annotations):
        for x, text in enumerate(row):
            heat_axis.text(x, y, text, ha="center", va="center", fontsize=5.5,
                           color="#182230" if codes[y][x] in (0, 3, 5) else "white")
    heat_axis.set_xticks([index - 0.5 for index in range(1, len(SOLVERS))], minor=True)
    heat_axis.set_yticks([index - 0.5 for index in range(1, len(tasks))], minor=True)
    heat_axis.grid(which="minor", color="white", linewidth=0.7)
    heat_axis.tick_params(which="minor", bottom=False, left=False)
    legend = [Patch(facecolor=STATUS_COLORS[code], edgecolor="white", label=label)
              for code, label in ((0, "solved"), (1, "timeout"), (2, "memory limit"),
                                  (3, "no plan reported"), (4, "other failure"), (5, "not run"))]
    heat_axis.legend(handles=legend, loc="upper center", bbox_to_anchor=(0.5, -0.055),
                     ncol=6, frameon=False)

    figure.text(0.01, -0.005,
                "Wall time includes startup, parsing/translation, and search. Each task/solver has up to 3 successful attempts; a failure stops that cell. "
                "Powerlifted and Tyr plans were not independently validated. The folplan geometric optimizer may count replayed path states, so internal expansion counts are not compared.",
                ha="left", va="top", fontsize=7, color="#454b54", wrap=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(args.output, dpi=240, bbox_inches="tight", facecolor="white")
    print(args.output)
    plot_compact(summary, args.compact_output)


if __name__ == "__main__":
    main()
