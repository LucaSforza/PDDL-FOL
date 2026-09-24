# FOL Planner

FOL Planner is a small Rust library and command-line planner for deterministic
classical planning over finite, typed domains. Models can be written in the
slide-like FOLPlan DSL or imported from the supported PDDL subset. The
planner finds shortest plans with breadth-first search and evaluates
first-order preconditions and goals on finite models.

```text
action Move(b: block, from: object, to: object) {
  pre: On(b, from) ∧ b ≠ to ∧ from ≠ to
       ∧ (∀ z: block . ¬On(z, b))
       ∧ (to ≠ table → (∀ z: block . ¬On(z, to)));
  effect: ¬On(b, from) ∧ On(b, to);
}
```

This action is part of the complete [quantified blocks model](examples/blocks-quantified.fol).
ASCII equivalents such as `&`, `!`, `!=`, and `forall` are supported too.

```sh
cargo run -- solve examples/travel.fol
cargo run -- pddl examples/pddl/travel-domain.pddl examples/pddl/travel-problem.pddl
cargo run -- check examples/quantified-inspection.fol
cargo run -- solve examples/blocks-quantified.fol
cargo doc --no-deps --open
```

Requires Rust 1.85 or later (edition 2024); no external dependencies. Run
`cargo test` for unit, integration, CLI, and executable documentation tests.

Situation calculus is implemented through finite successor-state semantics
and action histories. Search is BFS over resulting states, not general FOL
resolution. Unsupported PDDL features fail explicitly; see the guide for
the supported subset and resource limits.

See the [DSL and modeling guide](docs/guide.md) for syntax, semantics, examples,
and limitations. The [`kb/`](kb/README.md) directory records the design
contracts.
