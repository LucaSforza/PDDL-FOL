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
DSL effect literals become separate Add/Delete lists. Search derives candidate
action bindings by joining necessary positive conjuncts with state facts and
completes unbound parameters over compatible objects. Preconditions and
quantifiers are evaluated with bindings on demand. No CNF, SAT, or resolution
conversion is performed.
The implementation enforces successor-state axioms operationally through set
updates; it does not export or prove an unrestricted axiom theory. A situation
is an action history, distinct from its resulting state. Graph search merges histories
with identical resulting states, while retaining one predecessor chain to
construct the returned situation witness.

## Search

Forward graph search, with lifted candidate joins and bounded Cartesian fallback
for preconditions without necessary positive atoms, canonical state deduplication,
and predecessors for plan reconstruction. The default is A* with unit action
costs and an admissible bound described below. Explicit BFS remains available
for comparison. Both return shortest plans and are complete on this finite
state space if no limit is reached. Outcomes are distinct: plan (including the
empty plan), unreachable after graph exhaustion, state limit reached, model
error, or excessive grounding.

For a goal that is a conjunction of positive ground atoms, `h_max` starts with
cost 0 for atoms in the current state. For each action in a complete small grounding, positive
conjunctive precondition atoms have relaxed action cost
`1 + max(cost(precondition atoms))`; unsupported precondition structure is
ignored, making the relaxation weaker but safe. Add effects receive the
minimum such cost over actions. Delete effects are ignored. The goal estimate
is the maximum cost of its atoms; a goal atom unreachable even in this
complete relaxation makes the state a safe dead end. Large Cartesian groundings
skip `h_max` and retain the goal-cover bound. For other FOL goal structures,
the estimate is 0. No negative or quantified goal is silently reinterpreted.
The exact evaluator still decides applicability and goal satisfaction.

For the same positive conjunctive goal, let `m` be the number of currently
false distinct goal atoms and `k` an upper bound on how many distinct goal
atoms one action can add. The goal-cover lower bound is `ceil(m/k)`;
when `m > 0` and `k = 0`, the goal is unreachable. One action can make at
most `k` missing goal atoms true, so this bound is admissible and consistent.
The maximum with `h_max` remains admissible and consistent. This deliberately
counts actions shared across goals once, avoiding the inadmissible sum of
individual relaxed costs.

A* keeps the cheapest discovered path per state and reopens a state if a
cheaper path arrives. It tests goals when removing a state from the frontier,
uses deterministic tie breaking, and counts stored distinct states against
`max_states`. Agent provides graph search machinery; this crate owns
lifted candidate generation, FOL evaluation, transitions, and heuristic
evaluation. `max_ground_actions` counts distinct candidates emitted across
search states, before the full precondition check. Returned
plans are replayed before delivery.

Configurable limits on stored states and ground actions prevent unbounded growth
of the main data structures. They are not time limits: evaluating nested
quantifiers can still be expensive. These algorithms remain educational, not
suited to industrial instances. The chosen algorithm is visible in API and CLI.

Input nesting is bounded at 256 levels. Semantic validation bounds action
parameters and combined formula nesting/quantifier bindings at 256, before
recursive grounding or evaluation. Oversized models produce input errors.

## Exclusions

No numeric fluents, costs/durations, functions, conditional or quantified
effects, derived predicates, type hierarchies, uncertainty, concurrency, or
external SAT/SMT solvers. Unsupported constructs must produce errors and must
never be ignored.
