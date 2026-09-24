# FOL Planner

FOL Planner is a Rust library and CLI for deterministic classical planning over
finite, typed domains. Write tasks in the FOLPlan DSL or import the documented
subset of PDDL. The planner grounds actions and uses breadth-first search (BFS)
to find a shortest plan by number of actions.

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

BFS merges paths that reach the same state and retains one predecessor chain
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

Search stores at most 100,000 states and grounds at most 100,000 actions by
default. Both limits can be changed on search commands. These are storage and
grounding limits, not time limits; nested quantifiers may still be expensive.
Input nesting, action parameters, and combined formula nesting and quantified
bindings are bounded at 256.

## CLI

```text
cargo run -- solve file.fol [--max-states N] [--max-ground-actions N]
cargo run -- pddl domain.pddl problem.pddl [--max-states N] [--max-ground-actions N]
cargo run -- check file.fol
cargo run -- check-pddl domain.pddl problem.pddl
```

`check` commands parse and validate without searching. Search reports the plan
and situation term, proves unreachability only after exhausting the reachable
state graph, or reports when a limit stops the search. Exit codes: `0` for a
plan or valid input, `1` for input or argument errors, `2` for unreachable
goals, and `3` for a search or grounding limit.

Requires Rust 1.85 or later (edition 2024); no external dependencies.

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
