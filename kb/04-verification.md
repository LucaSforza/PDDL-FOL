# Verification plan

## Acceptance criteria

- DSL and PDDL parse into the same Task, with localized errors where possible.
- FOL: CWA, UNA, not/and/or/imply, quantifiers, empty types, lexical scope.
- Validation: duplicate declarations, unknown types/predicates/objects,
  arity, free variables, and ill-typed effects and initialization.
- BFS: minimal plan, valid replay, frame, Add takes precedence over Delete,
  empty plan, unsolvable cycle, limits distinguished from impossibility.
- Examples actually run: travel, blocks, logistics, quantifiers; complete PDDL
  models, with at least one impossible instance and one already satisfied.
- Rustdoc with compilable examples and an embedded DSL guide.
- Run `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo doc --no-deps`; run the CLI on all examples.

## Incremental deliverables

1. KB Markdown/Mermaid before implementation.
2. Shared model and FOL/BFS engine.
3. DSL/PDDL parser, CLI, examples, and documentation.
4. Integration tests, fixes arising from them, and an execution report.

The user requested delegation to `gpt-6-luna` subagents at high reasoning effort.
Responsibilities are separated across engine, parser, and CLI/documentation;
the main agent handles integration and final verification. Subagents do not
commit.
