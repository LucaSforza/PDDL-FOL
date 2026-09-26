#!/usr/bin/env python3
"""Compute exact route lengths for the pinned HTG multidimensional VisitAll corpus.

This reads only the corpus's simple ground init/goal atoms; it is an independent
route-length check, not a general PDDL parser or plan validator.
"""

import argparse
import csv
import itertools
import re
from collections import deque
from pathlib import Path


def atoms(section, predicate):
    pattern = rf"\({re.escape(predicate)}\s+([^)]*)\)"
    return [tuple(match.split()) for match in re.findall(pattern, section)]


def distance(graph, source, target):
    if source == target:
        return 0
    queue = deque([(source, 0)])
    seen = {source}
    while queue:
        node, cost = queue.popleft()
        for neighbor in graph.get(node, ()):
            if neighbor == target:
                return cost + 1
            if neighbor not in seen:
                seen.add(neighbor)
                queue.append((neighbor, cost + 1))
    return None


def optimal_length(path):
    initial, goal = path.read_text().split("(:goal", 1)
    locations = atoms(initial, "at-robot")
    if len(locations) != 1:
        raise ValueError(f"expected one initial robot location: {path}")
    start = locations[0]
    visited = set(atoms(initial, "visited"))
    targets = tuple(dict.fromkeys(target for target in atoms(goal, "visited")
                                  if target not in visited))
    graph = {}
    for source, destination in atoms(initial, "neighbor"):
        graph.setdefault(source, set()).add(destination)

    def product_distance(source, destination):
        if len(source) != len(destination):
            raise ValueError(f"mismatched tuple arity: {path}")
        legs = [distance(graph, a, b) for a, b in zip(source, destination)]
        return None if any(leg is None for leg in legs) else sum(legs)

    routes = []
    for order in itertools.permutations(targets):
        legs = [product_distance(a, b) for a, b in zip((start,) + order, order)]
        if all(leg is not None for leg in legs):
            routes.append(sum(legs))
    if not targets:
        return 0, 0
    return len(targets), min(routes, default=None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    rows = []
    for dimension, placement, goal_count in itertools.product(
        (3, 4, 5), ("CLOSE", "FAR"), (1, 2, 3)
    ):
        family = f"{dimension}-dim-visitall-{placement}-g{goal_count}"
        for index in range(10):
            problem = f"p{index}"
            targets, cost = optimal_length(args.corpus_root / family / f"{problem}.pddl")
            rows.append((family, problem, targets, "" if cost is None else cost))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="") as stream:
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(("family", "problem", "remaining_targets", "optimal_length"))
        writer.writerows(rows)
    print(f"{len(rows)} tasks; {sum(cost == '' for _, _, _, cost in rows)} unreachable")


if __name__ == "__main__":
    main()
