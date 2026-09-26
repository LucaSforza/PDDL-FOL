# Verification plan

## Acceptance criteria

- DSL and PDDL parse into the same Task, with localized errors where possible.
- DSL accepts ASCII word connectives and quantifiers, preserves supported ASCII
  symbolic spellings, and both DSL and PDDL reject non-ASCII input in comments.
- FOL: CWA, UNA, not/and/or/imply, quantifiers, empty types, lexical scope.
- Validation: duplicate declarations, unknown types/predicates/objects,
  arity, free variables, and ill-typed effects and initialization.
- BFS and A*: minimal plan, valid replay, frame, Add takes precedence over
  Delete, empty plan, unsolvable cycle, limits distinguished from impossibility.
- A* `max(h_max, h_cover)`: never overestimates exact remaining distance on small
  exhaustive graphs; handles negative/quantified goals with zero fallback; preserves
  optimal path under duplicate discovery and reopens cheaper paths in Agent.
- Both search modes use lifted successor joins. Sparse high-arity models avoid
  Cartesian grounding; candidate limits retain the same meaning under A* and BFS.
- Large action universes skip complete `h_max` grounding and keep admissible
  goal-cover guidance. Compare on selected HTG VisitAll instances and report
  full-process latency and coverage against the same external planners.
- Agent convenience API hides `bumpalo` from callers; existing Agent examples
  and tests still compile. Distinct-state budget produces a limit outcome.
- Examples actually run: travel, blocks, logistics, quantifiers; complete PDDL
  models, with at least one impossible instance and one already satisfied.
- Rustdoc with compilable examples and an embedded DSL guide.
- Run `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo doc --no-deps` in both crates; run the CLI on all examples.
- Benchmark paired release builds on identical instances, recording plan
  length, expanded states, and elapsed time; avoid speed claims from tiny
  samples or different machines.

## Incremental deliverables

1. KB Markdown/Mermaid before implementation.
2. Shared model and FOL/BFS engine.
3. DSL/PDDL parser, CLI, examples, and documentation.
4. Integration tests, fixes arising from them, and an execution report.

The current request authorizes `gpt-6-luna` subagents at high reasoning effort
and commits. One agent owns Agent's search API/correctness, another owns the
planner's Agent adapter and heuristic, and a third owns CLI/tests if needed.
The main agent owns KB contracts, integration, benchmarks, and report.
