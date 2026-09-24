# Verification record

Verified on 2026-09-24 with Rust/Cargo 1.92.0, without external dependencies.

## Quality checks

```sh
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
```

33 tests pass: 10 unit tests, 15 semantic integration tests, 4 robustness tests,
3 CLI test groups (running every supplied model), and 1 executable rustdoc.
CLI argument failures, unknown files, and both search limits are tested too.
Documentation builds with warnings treated as errors. Generated guide:
`target/doc/pddl_fol/guide/index.html`.

## Actual CLI runs

All files were parsed, validated, and solved using the built `folplan` binary.
Exit 2 is the expected result for the two impossible tasks.

| Input | Example | Plan length / outcome | States explored | Exit |
| --- | --- | ---: | ---: | ---: |
| DSL | travel | 1 | 2 | 0 |
| DSL | blocks | 3 | 12 | 0 |
| DSL | blocks-quantified | 3 | 13 | 0 |
| DSL | logistics | 3 | 5 | 0 |
| DSL | quantified-inspection | 2 | 4 | 0 |
| DSL | impossible | unreachable | 1 | 2 |
| DSL | satisfied | 0 | 1 | 0 |
| PDDL | travel | 1 | 2 | 0 |
| PDDL | blocks | 3 | 12 | 0 |
| PDDL | logistics | 3 | 5 | 0 |
| PDDL | inspection | 2 | 4 | 0 |
| PDDL | impossible | unreachable | 1 | 2 |
| PDDL | satisfied | 0 | 1 | 0 |

## Slide blocks demonstration

```sh
cargo run -- solve examples/blocks-quantified.fol
```

```text
Plan (3 steps):
  1. move(c, a, table)
  2. move(b, table, c)
  3. move(a, table, b)
Situation: do(move(a, table, b), do(move(b, table, c), do(move(c, a, table), S0)))
States explored: 13
```

The explicit-Clear PDDL model also returns three actions: `unstack(c,a)`,
`stack(b,c)`, `stack(a,b)`. Integration tests replay returned plans and verify
that the final state satisfies the FOL goal. Separate regressions check
frame persistence, Add/Delete overlap, lexical shadowing, empty types,
shortest paths, cycles, and distinction between a limit and a proof of
unreachability.

## Scope of the evidence

These are small finite models; no claim is made about industrial performance.
The planner implements successor-state semantics operationally with BFS, not
general resolution over situation-calculus axioms. Full PDDL/ADL, numeric and
conditional effects, and type hierarchies are unsupported and rejected.
