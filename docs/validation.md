# Verification record

Verified on 2026-09-26 with Rust/Cargo 1.92.0. `pddl_fol` uses the local
`vendor/agent` crate for graph-search machinery.

## Quality checks

```sh
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
cargo test --manifest-path vendor/agent/Cargo.toml
```

All 38 root-crate tests pass: 14 unit tests, 4 CLI tests, 4 robustness tests,
15 semantic integration tests, and 1 executable rustdoc. Agent's crate tests
are run separately as well. CLI tests cover both search choices, the A* default,
argument errors, unknown files, and both search limits. Documentation builds
with warnings treated as errors. Generated guide:
`target/doc/pddl_fol/guide/index.html`.

## Actual CLI runs

All files were parsed, validated, and solved using the built `folplan` binary
with the default A* search. Exit 2 is the expected result for the two impossible
tasks.

| Input | Example | Plan length / outcome | States explored | Search | Exit |
| --- | --- | ---: | ---: | --- | ---: |
| DSL | travel | 1 | 2 | A* | 0 |
| DSL | blocks | 3 | 4 | A* | 0 |
| DSL | blocks-quantified | 3 | 6 | A* | 0 |
| DSL | logistics | 3 | 4 | A* | 0 |
| DSL | quantified-inspection | 2 | 4 | A* | 0 |
| DSL | impossible | unreachable | 1 | A* | 2 |
| DSL | satisfied | 0 | 1 | A* | 0 |
| PDDL | travel | 1 | 2 | A* | 0 |
| PDDL | blocks | 3 | 4 | A* | 0 |
| PDDL | logistics | 3 | 4 | A* | 0 |
| PDDL | inspection | 2 | 4 | A* | 0 |
| PDDL | impossible | unreachable | 1 | A* | 2 |
| PDDL | satisfied | 0 | 1 | A* | 0 |

## Slide blocks demonstration

```sh
cargo run -- solve examples/blocks-quantified.fol
```

```text
Search: A*
Plan (3 steps):
  1. move(c, a, table)
  2. move(b, table, c)
  3. move(a, table, b)
Situation: do(move(a, table, b), do(move(b, table, c), do(move(c, a, table), S0)))
States explored: 6
```

The explicit-Clear PDDL model also returns three actions: `unstack(c,a)`,
`stack(b,c)`, `stack(a,b)`. Integration tests replay returned plans and verify
that the final state satisfies the FOL goal. Separate regressions check
frame persistence, Add/Delete overlap, lexical shadowing, empty types,
shortest paths, cycles, and distinction between a limit and a proof of
unreachability.

## Scope of the evidence

These are small finite models; no claim is made about industrial performance.
The planner implements successor-state semantics operationally, uses A* by
default, and retains BFS through `--search bfs`. It is not a general resolution
engine for situation-calculus axioms. Full PDDL/ADL, numeric and conditional
effects, and type hierarchies are unsupported and rejected.
