# Modeling with FOLPlan

FOLPlan models finite classical planning problems with typed objects,
predicates, an initial situation, a goal, and STRIPS actions. Write models in
the slide-like `.fol` language or use the supported subset of standard PDDL.
The planner evaluates first-order formulas over a finite model and uses
forward breadth-first search to find a shortest plan.

## A small travel problem

This model has two places joined by a road. The traveler starts at home and
wants to reach the office.

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

Start by declaring the flat types and named objects. `object` is always an
implicit supertype; types do not form a hierarchy. In `predicates`, list the
type of each argument. `At(place)` is unary, while `Road(place, place)` relates
two places. Use `Ready()` for a predicate with no arguments.

`init` lists the ground facts true in the initial situation. `goal` is a closed
formula that must hold in a reached situation. Each action names its parameters,
precondition, and effect. Names used as parameters are variables in formulas;
unbound names are object constants. For example, `Move(from: place, to: place)`
makes `from` and `to` variables in the precondition and effects.

The same action in first-order notation has precondition
`At(from,s) and Road(from,to) and from != to`. It adds `At(to)` and deletes
`At(from)`. For each place `z`, its location in the resulting situation is:

```text
At(z, do(Move(from,to),s)) iff (z = to) or (At(z,s) and z != from)
```

Other facts such as `Road(home, office)` persist because the action does not
change them. This is the frame behavior of STRIPS transitions. FOLPlan builds
the finite transition relation directly; it does not prove arbitrary theorems
from a generated situation-calculus axiom theory.

## Syntax and formulas

Declarations end with semicolons. Types and objects can be empty, and empty
`init` is written `init: true;`. Empty object domains and types are permitted.
Parentheses group formulas and disambiguate quantifier scope.

| Form | Syntax | Meaning |
| --- | --- | --- |
| Problem | `problem name { ... }` | One planning task |
| Types | `types block, surface;` | Flat types; `object` is implicit |
| Objects | `objects { a, b: block; table: surface; }` | Typed named objects |
| Predicate | `On(block, object);` | Argument types; `Ready();` has arity zero |
| Initial state | `init: On(a, table) and Clear(a);` | Positive ground atoms, or `true` |
| Goal | `goal: formula;` | Closed first-order formula |
| Action | `action Move(x: block) { pre: ...; effect: ...; }` | Action schema |
| Atom | `On(a,b)` | Predicate application |
| Equality | `x = y`, `x != y` | Identity or distinctness of object names |
| Negation | `not formula` | Logical negation |
| Connectives | `and`, `or`, `implies` | Conjunction, disjunction, implication |
| Quantifiers | `forall x:block . formula`; `exists x:block . formula` | Typed universal or existential |
| Constants | `true`, `false` | Truth and falsity |
| Comment | `// comment` | Text through end of line |

The production structure is:

```text
problem    := "problem" name "{" section* "}"
section    := types | objects | predicates | init | goal | action
types      := "types" name-list? ";"
objects    := "objects" "{" (name-list ":" name ";")* "}"
predicates := "predicates" "{" (name "(" name-list? ")" ";")* "}"
init       := "init" ":" ("true" | atom ("and" atom)*) ";"
goal       := "goal" ":" formula ";"
action     := "action" name "(" (name ":" name ("," name ":" name)*)? ")"
              "{" "pre" ":" formula ";" "effect" ":" effect ";" "}"
effect     := "true" | atom | "not" atom | "(" effect ")" | effect ("and" effect)*
formula    := implication
implication := disjunction ("implies" implication)?
disjunction := conjunction ("or" conjunction)*
conjunction := unary ("and" unary)*
unary      := "not" unary | quantifier | atom | equality | "true" | "false"
              | "(" formula ")"
quantifier := ("forall" | "exists") name ":" name ("," name ":" name)* "." formula
atom       := name "(" name-list? ")"
equality   := name ("=" | "!=" ) name
name-list  := name ("," name)*
```

Each of `types`, `objects`, `predicates`, `init`, and `goal` is required once;
`action` may repeat. Empty `types;`, `objects {}`, `predicates {}`, and
`init: true;` are valid. An effect can also be `true` to express no change.
The grammar shows the preferred word spellings.

ASCII symbol spellings remain valid: `!` for `not`, `&` or `&&` for `and`,
`|` or `||` for `or`, and `->` for `implies`. The entire `.fol` file, including
comments, must contain ASCII characters; non-ASCII input reports its location.
Precedence, from weakest to strongest, is implication (right-associative), or,
and, then unary negation. Quantifier bodies extend across the following
formula, as described below. Parentheses make the intended grouping explicit.

Identifiers start with an ASCII letter and continue with letters, digits,
underscores, or hyphens. Names are case-insensitive and normalize to lowercase.
The logical keywords `true`, `false`, `not`, `and`, `or`, `implies`, `forall`,
and `exists` are reserved.

Variables are names bound by action parameters or quantifiers. A quantifier's
scope runs to the end of its containing formula; parenthesize it when the
scope should be smaller. Nested bindings may shadow an outer name. Variables
may be free only as action parameters, never in `init` or `goal`.

For example, `goal: exists x:block . Clear(x);` says that some block is clear.
`goal: forall x:item . Inspected(x);` requires every object of type `item` to
be inspected. Quantification over an empty type follows standard FOL: a
universal statement is true and an existential statement is false.

## Blocks and quantification

`examples/blocks.fol` includes the slide's initial fact `On(c,a)` and explicit
`Clear` facts. Its goal is `On(a,b) & On(b,c) & OnTable(c)`, reached by
unstacking `c`, stacking `b` on `c`, then stacking `a` on `b`.

`examples/blocks-quantified.fol` defines clear space directly with FOL instead
of maintaining a `Clear` predicate:

```fol
pre: On(b, from)
  and b != to
  and from != to
  and (forall z:block . not On(z, b))
  and (to != table implies (forall z:block . not On(z, to)));
```

The first universal clause means no block is on top of `b`; the one about `to`
allows the table to hold multiple blocks while requiring a non-table
destination to be clear. The start and goal facts match the lecture slide.

An action applies only when its precondition is true. Effects are simultaneous
conjunctions of positive or negated atoms. Positive literals add facts;
negated literals delete facts. An atom present in both add and delete effects
ends up true. All other facts persist. `init` contains positive atoms only;
the closed-world assumption provides the negative facts.

## Finite-domain semantics

The listed objects are the entire domain (domain closure), and different
object names denote different objects (unique-name assumption, UNA). Any ground
atom absent from the current state is false under the closed-world assumption
(CWA). Equality compares object identity. These conventions make a model and
each search state finite and explicit.

The planner searches situations reachable by applying the action schemas. A
solution is a constructive witness `do(a_n, ... do(a_1, S0))` for a situation
satisfying the goal. It is not a general FOL prover or a resolution engine for
situation-calculus axioms. An already satisfied goal has the empty plan;
exhausting the reachable graph proves it unreachable in this finite model.
State and grounding limits report separately from an impossible goal.

Parsers reject syntax nested beyond 256 levels. Semantic validation also
limits action parameters to 256 and combined formula nesting and quantified
bindings to 256, protecting recursive evaluation from oversized input.

## PDDL subset

PDDL uses a domain file with predicates and actions, plus a problem file with
objects, initial facts, and a goal. Flat typing is supported; untyped names
belong to `object`. Preconditions support the same connectives and quantifiers
as the FOLPlan language. Effects support atoms, negated atoms, and conjunctions
of those forms. Domain and problem files use ASCII throughout, including
comments.

Accepted requirements are `:strips`, `:typing`, `:negative-preconditions`,
`:disjunctive-preconditions`, `:equality`, `:existential-preconditions`,
`:universal-preconditions`, and `:quantified-preconditions`. Requirements need
not list every feature used. Unsupported requirements or sections are errors;
`:adl` is rejected because it includes conditional effects. Type hierarchies,
functions, numeric fluents, conditional or quantified effects, and derived
predicates are outside the supported subset.

## Command line

```text
folplan solve file.fol [--max-states N] [--max-ground-actions N]
folplan pddl domain.pddl problem.pddl [--max-states N] [--max-ground-actions N]
folplan check file.fol
folplan check-pddl domain.pddl problem.pddl
folplan --help
```

Limits must be positive integers. `--max-ground-actions` caps distinct grounded
action candidates emitted across the search, after positive conjunctive
preconditions have filtered them. Schemas without such atoms fall back to
bounded Cartesian candidate generation. `check` parses and validates without search.
Exit status is `0` for a plan or successful validation, `1` for I/O, syntax,
semantic, or argument errors, `2` when search proves the goal unreachable,
and `3` when a state or grounding limit is reached.

## Rust library

The library exposes `parse_dsl`, `parse_pddl`, `validate`, `evaluate`, `solve`,
and `replay`, along with model and result types. This doctest parses the travel
model, checks its initial goal value, and verifies the returned plan by replay:

```rust
use pddl_fol::{evaluate, parse_dsl, replay, solve, SearchLimits, SearchOutcome};

let task = parse_dsl(r#"
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
"#).unwrap();

assert!(!evaluate(&task, &task.initial, &task.goal).unwrap());
let plan = match solve(&task, SearchLimits::default()).unwrap() {
    SearchOutcome::Solved(plan) => plan,
    other => panic!("expected a plan, got {other:?}"),
};
assert_eq!(plan.steps.len(), 1);
assert_eq!(plan.situation(), "do(move(home, office), S0)");
let reached = replay(&task, &plan.steps).unwrap();
assert_eq!(reached, plan.final_state);
assert!(evaluate(&task, &reached, &task.goal).unwrap());
```

The planner is a teaching-scale finite-state BFS. Nested quantifiers, wide
joins, Cartesian fallback, and large reachable state spaces can be expensive.
The model
has no action costs, uncertainty, concurrency, numeric fluents, or external
SAT/SMT solver.

## Example catalog

| File in `examples/` | Modeling idea | Expected result |
| --- | --- | --- |
| `travel.fol` | Static roads and changing location | One move |
| `blocks.fol` | Explicit `Clear` facts maintained by effects | Three actions |
| `blocks-quantified.fol` | `forall` derives clear space; `implies` exempts the table | Three moves |
| `logistics.fol` | Loading, transport, unloading | Three actions |
| `quantified-inspection.fol` | Universal goal with existential precondition | Two inspections |
| `impossible.fol` | Reachability exhausted without satisfying goal | No plan |
| `satisfied.fol` | Goal holds initially | Empty plan |

The `examples/pddl/` directory contains six matching domain/problem pairs:
travel, blocks, logistics, inspection, impossible, and satisfied. The
quantified blocks variant uses the same task semantics through the DSL.
