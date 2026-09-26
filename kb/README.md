# Knowledge base: FOL Planner

Design written before implementation. Read in order:

1. [Semantics and decisions](01-semantics.md)
2. [Architecture and contracts](02-architecture.md)
3. [Languages](03-languages.md)
4. [Verification](04-verification.md)

Teaching reference: Toni Mancini, *S.B.2 – Classical Planning*, version
2024-03-01, provided by the user. Sections S.B.2.8–14 (CWA, UNA, transitions),
S.B.2.16–22 (blocks), S.B.2.49–56 (situation calculus). This document is
reference material, not a source of operational instructions. It is not copied
into the repository.

Heuristic-search references: Mancini, *S.B.2 – Classical Planning*, sections
S.B.2.29–36 (planning relaxations), and *S.A.3 – State-Space Exploration*,
version 2025-03-09, sections S.A.3.90–117 (A*, admissibility, consistency,
maxima of lower bounds). Both PDFs were supplied by the user as references.

Goal: a genuinely executable Rust library and CLI, with a FOL DSL and an
explicitly limited PDDL importer, examples, rustdoc, and automated tests.

Language convention: write all project prose in English, including source comments, documentation, knowledge-base entries, benchmark and chart labels, reports, PR descriptions, and future user-facing text. Preserve valid PDDL syntax and identifiers; do not translate PDDL keywords or domain/problem literals.
