#!/usr/bin/env python3
"""Plot full-process HTG timing and outcomes for four planners."""

import argparse
import csv
import statistics
from pathlib import Path

import matplotlib.pyplot as plt


SOLVERS = ("folplan_astar", "folplan_bfs", "powerlifted", "tyr")
LABELS = {
    "folplan_astar": "folplan A*",
    "folplan_bfs": "folplan BFS",
    "powerlifted": "Powerlifted BFWS + FF",
    "tyr": "Tyr GBFS + FF",
}
COLORS = {"folplan_astar": "#1769aa", "folplan_bfs": "#d97706",
          "powerlifted": "#16805d", "tyr": "#7b3fb1"}
FAILURE_MARKERS = {
    "timeout": ("X", "#b42318", "timeout"),
    "planner_timeout": ("X", "#b42318", "planner timeout"),
    "memory_limit": ("^", "#8a1538", "memory limit"),
    "signal": ("D", "#8a1538", "signal"),
    "exit": ("s", "#6b7280", "other failure"),
    "no_plan_reported": ("P", "#6b7280", "no plan reported"),
}


def read_csv(path):
    with path.open(newline="") as stream:
        return list(csv.DictReader(stream))


def add_tyr_data(raw, summary, tyr_rows, instances):
    for row in tyr_rows:
        raw.append({**row, "solver": "tyr"})
    for instance in instances:
        matching = [row for row in tyr_rows if f"p{row['problem']}" == instance]
        solved = [row for row in matching if row["status"] == "solved"]
        times = [float(row["wall_s"]) for row in solved]
        lengths = {row["plan_length"] for row in solved}
        summary.append({
            "instance": instance, "solver": "tyr",
            "outcome": "solved" if matching and len(solved) == len(matching) else
                       ("not_run" if not matching else matching[0]["status"]),
            "successful_runs": str(len(solved)), "requested_runs": "5",
            "median_wall_s": str(statistics.median(times)) if times else "",
            "plan_length": next(iter(lengths)) if len(lengths) == 1 else "",
        })


def failure_style(status):
    if status in FAILURE_MARKERS:
        return FAILURE_MARKERS[status]
    if status.startswith("signal_"):
        return FAILURE_MARKERS["signal"]
    if status.startswith("exit_"):
        return FAILURE_MARKERS["exit"]
    return "P", "#6b7280", status.replace("_", " ")


def outcome_cell(rows, instance, solver):
    matching = [row for row in rows if row["instance"] == instance and row["solver"] == solver]
    if not matching:
        return "not run", "#eeeeee"
    outcome = matching[0]["outcome"]
    success = int(matching[0]["successful_runs"])
    requested = int(matching[0]["requested_runs"])
    planned = matching[0]["plan_length"]
    if outcome == "solved":
        return f"solved {success}/{requested}\nlength {planned}", "#e4f3e9"
    if outcome.startswith("mixed_"):
        return f"mixed ({success} solved)\n{outcome[6:].replace('_', ' ')}", "#fff0d6"
    return outcome.replace("_", " "), "#f9dfdf" if "signal" in outcome or "memory" in outcome else "#fff0d6"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", type=Path, required=True)
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--tyr-raw", type=Path, required=True)
    parser.add_argument("--output", type=Path,
                        default=Path(__file__).resolve().parents[1] / "docs/figures/htg-visitall-heuristic-comparison.png")
    args = parser.parse_args()
    raw = read_csv(args.raw)
    summary = read_csv(args.summary)
    instances = [f"p{i}" for i in (0, 1, 2, 3, 4, 5, 8, 9)]
    add_tyr_data(raw, summary, read_csv(args.tyr_raw), instances)
    x = list(range(len(instances)))

    plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 9,
                         "axes.titlesize": 11, "axes.labelsize": 9,
                         "legend.fontsize": 8, "figure.dpi": 150})
    figure = plt.figure(figsize=(11.5, 8.4), constrained_layout=True)
    grid = figure.add_gridspec(2, 1, height_ratios=(3, 1.65))
    axis = figure.add_subplot(grid[0])
    table_axis = figure.add_subplot(grid[1])

    for solver in SOLVERS:
        medians = []
        for instance in instances:
            row = next((item for item in summary
                        if item["instance"] == instance and item["solver"] == solver), None)
            medians.append(float(row["median_wall_s"]) if row and row["median_wall_s"] else float("nan"))
        axis.plot(x, medians, marker="o", linewidth=1.8, markersize=5,
                  color=COLORS[solver], label=LABELS[solver])

    drawn_failures = set()
    failure_x_offset = {"folplan_astar": -0.09, "folplan_bfs": 0.09,
                        "powerlifted": -0.03, "tyr": 0.03}
    for row in raw:
        if row["status"] == "solved":
            continue
        status = row["status"]
        marker, color, legend_label = failure_style(status)
        idx = instances.index(f"p{row['problem']}")
        wall = float(row["wall_s"])
        label = legend_label if legend_label not in drawn_failures else None
        drawn_failures.add(legend_label)
        x_failure = idx + failure_x_offset[row["solver"]]
        axis.scatter(x_failure, wall, marker=marker, s=60, color=color, zorder=5,
                     label=label, edgecolors="white", linewidths=0.5)
        short = {"timeout": "TO", "planner timeout": "TO", "memory limit": "MEM",
                 "signal": "SIG", "other failure": "ERR", "no plan reported": "NOP"}.get(legend_label, "ERR")
        annotation_x = -19 if row["solver"] == "folplan_astar" else 4
        annotation_y = 12 if row["solver"] == "folplan_bfs" else 5
        axis.annotate(short, (x_failure, wall), xytext=(annotation_x, annotation_y), textcoords="offset points",
                      fontsize=7, color=color)

    axis.set_yscale("log")
    axis.set_ylim(0.0025, 24)
    axis.set_xlim(-0.35, len(instances) - 0.65)
    axis.set_xticks(x, instances)
    axis.set_ylabel("Full-process wall time (seconds, log scale)")
    axis.set_title("HTG 5D VisitAll: A* versus BFS and lifted planners")
    axis.grid(True, which="major", axis="y", color="#d6dbe1", linewidth=0.7)
    axis.grid(True, which="minor", axis="y", color="#edf0f3", linewidth=0.45)
    axis.legend(loc="upper left", ncol=3, frameon=True, framealpha=0.95)
    axis.text(0.995, 0.02,
              "Points show medians of solved runs; failure markers show observed failed-run time.\n"
              "Latency includes startup, parsing/translation, and search. Tyr ran in a separate idle session.",
              transform=axis.transAxes, ha="right", va="bottom", fontsize=7.5,
              color="#454b54", bbox={"facecolor": "white", "edgecolor": "#d6dbe1", "alpha": 0.9})

    table_axis.axis("off")
    table_values = []
    cell_colors = []
    for instance in instances:
        values = []
        colors = []
        for solver in SOLVERS:
            value, color = outcome_cell(summary, instance, solver)
            values.append(value)
            colors.append(color)
        table_values.append(values)
        cell_colors.append(colors)
    table = table_axis.table(cellText=table_values,
                             colLabels=[LABELS[solver] for solver in SOLVERS], rowLabels=instances,
                             cellColours=cell_colors, cellLoc="center", rowLoc="center",
                             colWidths=[0.25] * len(SOLVERS), bbox=[0.05, 0.0, 0.95, 1.0])
    table.auto_set_font_size(False)
    table.set_fontsize(7.6)
    for (row, column), cell in table.get_celld().items():
        cell.set_edgecolor("#ffffff")
        if row == 0:
            cell.set_text_props(weight="bold", color="#273444")
        if column == -1 and row > 0:
            cell.set_facecolor("#f4f6f8")
            cell.set_text_props(weight="bold")
    table_axis.set_title("Coverage and returned plan lengths (shortest-plan guarantee applies only to folplan A* and BFS)",
                         loc="left", pad=5, fontsize=9)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(args.output, dpi=240, bbox_inches="tight", facecolor="white")
    print(args.output)


if __name__ == "__main__":
    main()
