# Semantics and decisions

## Logical model

Deterministic, observable, sequential classical planning with unit action cost.
The domain is a finite set of named objects; types are flat, with the implicit
supertype `object`. There are no functions over terms: terms are objects or
variables. Unique names (UNA), domain closure, and the closed-world assumption
(CWA) apply. A state contains exactly the true ground atoms; all others are
false.

Formulas: atoms, equality, negation, conjunction, disjunction, implication,
typed existential and universal quantifiers. Free variables are allowed only
as action parameters. Goals must be closed: existential variables from the
slides are made explicit. Quantification over an empty type: `forall` is true,
`exists` is false. Scope is lexical, including shadowing.

## Transitions and FOL

`Poss(a,s) ↔ pre(a)` evaluated in the finite interpretation of `s`.
`Result(s,a) = (s \\ Del(a)) ∪ Add(a)`; Add takes precedence on overlap.
Effects are simultaneous, with only positive atoms in Add/Del and variables
bound by the action parameters. Persistence of other fluents solves the frame
problem. For every ground atom F:

`F(do(a,s)) ↔ F ∈ Add(a) ∨ (F(s) ∧ F ∉ Del(a))`.

The plan produces a constructive witness
`do(a_n, ... do(a_1, S0))` for `∃s Goal(s)`, verified by replaying the
transitions. It is **not** a general FOL prover or a resolution engine for the
situation calculus: FOL is used to specify and evaluate preconditions and goals
over finite models.

## Conversion pipeline

DSL infix syntax and PDDL prefix syntax become the same typed formula AST.
DSL effect literals become separate Add/Delete lists. Parameters are grounded
over compatible objects; preconditions and quantifiers are evaluated with
bindings on demand. No CNF, SAT, or resolution conversion is performed.
The implementation enforces successor-state axioms operationally through set
updates; it does not export or prove an unrestricted axiom theory. A situation
is an action history, distinct from its resulting state. BFS merges histories
with identical resulting states, while retaining one predecessor chain to
construct the returned situation witness.

## Search

Forward BFS, with ground actions obtained from the Cartesian product of typed
domains, a visited set of canonical states, and predecessors for plan
reconstruction. Plans are minimal in number of actions; search is complete if
no limit is reached. Outcomes are distinct: plan (including the empty plan),
unreachable after graph exhaustion, state limit reached, model error, or
excessive grounding.

Configurable limits on stored states and ground actions prevent unbounded growth
of the main data structures. They are not time limits: evaluating nested
quantifiers can still be expensive. BFS is educational, not suited to
industrial instances. There are no hidden heuristics or semantic variants.

## Exclusions

No numeric fluents, costs/durations, functions, conditional or quantified
effects, derived predicates, type hierarchies, uncertainty, concurrency, or
external SAT/SMT solvers. Unsupported constructs must produce errors and must
never be ignored.
