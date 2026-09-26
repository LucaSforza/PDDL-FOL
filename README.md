# FOL Planner

FOL Planner is a Rust library and CLI for deterministic classical planning over
finite, typed domains. Write tasks in the FOLPlan DSL or import the documented
subset of PDDL. The planner uses A* by default and retains breadth-first search
(BFS) as an explicit option; both find shortest plans by number of actions.

## Build and install

With Rust 1.85+ and `just` installed:

```sh
just installation   # install CLI, .fol LSP, and Neovim configuration
just build          # debug build of both crates
just build-release  # release build of both crates
just test
just check          # formatting and Clippy
```

See [justfile](justfile) for all recipes and [LSP guide](lsp/README.md) for
Neovim details.

## Example

```fol
problem travel {
  types place;
  objects { home, office: place; }
  predicates { At(place); Road(place, place); }
  init: At(home) and Road(home, office);
  goal: At(office);

  action Move(from: place, to: place) {
    pre: At(from) and Road(from, to) and from != to;
    effect: not At(from) and At(to);
  }
}
```

Save as `travel.fol`, then run:

```sh
cargo run -- solve travel.fol
```

DSL formulas support atoms, equality, `not`, `and`, `or`, `implies`, `forall`,
and `exists`. ASCII symbols `!`, `&`, `&&`, `|`, `||`, and `->` are also
accepted. Effects are conjunctions of positive and negative atoms, interpreted
as simultaneous add and delete lists.

## Semantics and scope

Objects form a finite domain. Names are unique, and facts absent from a state
are false (closed-world assumption). Flat object types constrain grounding and
quantifier bindings. The engine evaluates formulas over these finite models,
applies action effects to states, and preserves all unaffected facts.

Search merges paths that reach the same state and retains one predecessor chain
for the returned plan. The plan can also be rendered as a situation-calculus
term such as `do(move(home, office), S0)`. This is a constructive witness for
the action history; FOL Planner is not a general FOL prover and does not derive
plans by resolving an unrestricted situation-calculus axiom theory.

The PDDL importer supports flat typing, predicates, STRIPS-style positive and
negative effects, and FOL preconditions and goals with equality, connectives,
and typed quantifiers. Unsupported constructs fail explicitly. The planner
does not support costs, numeric fluents, functions, conditional or quantified
effects, derived predicates, type hierarchies, uncertainty, concurrency, or
external SAT/SMT solvers.

Search stores at most 100,000 states and emits at most 100,000 distinct
grounded action candidates by default. Positive conjunctive preconditions
filter candidates against facts in each state; other precondition forms use a
bounded typed Cartesian fallback. A* is the default. For positive conjunctive
ground goals it combines goal coverage with delete-relaxed `h_max` when the
complete Cartesian grounding is small enough; larger models use goal coverage
alone. Use
`--search bfs` for breadth-first search. Both algorithms return shortest plans,
and the selected algorithm is shown in CLI output. These are storage and
candidate limits, not time limits; nested quantifiers may still be expensive.
Input nesting, action parameters, and combined formula nesting and quantified
bindings are bounded at 256.

## CLI

```text
cargo run -- solve file.fol [--search astar|bfs] [--max-states N] [--max-ground-actions N]
cargo run -- pddl domain.pddl problem.pddl [--search astar|bfs] [--max-states N] [--max-ground-actions N]
cargo run -- check file.fol
cargo run -- check-pddl domain.pddl problem.pddl
```

`check` commands parse and validate without searching. Search reports the plan
and situation term, proves unreachability only after exhausting the reachable
state graph, or reports when a limit stops the search. Exit codes: `0` for a
plan or valid input, `1` for input or argument errors, `2` for unreachable
goals, and `3` for a search or grounding limit.

Requires Rust 1.85 or later (edition 2024). Graph search uses the local Agent
crate in `vendor/agent`.

```sh
cargo run -- pddl examples/pddl/travel-domain.pddl examples/pddl/travel-problem.pddl
cargo run -- check examples/quantified-inspection.fol
cargo run -- solve examples/blocks-quantified.fol
cargo test
cargo doc --no-deps --open
```

See the [DSL and modeling guide](docs/guide.md) for full syntax, semantics,
examples, and limitations. The [`kb/`](kb/README.md) directory records design
contracts; [`docs/validation.md`](docs/validation.md) summarizes verification.
