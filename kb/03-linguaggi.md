# Languages

## FOLPlan DSL (.fol)

Readable S-expressions, with `;` comments through the end of the line. ASCII
identifiers: initial letter, followed by letters, digits, `_`, or `-`; variables
have the `?` prefix. Reserved operator words: `and`, `or`, `not`, `imply`,
`exists`, `forall`, `=`. Every listed section is required and unique, except
that `action` may repeat. `types` does not include the implicit supertype
`object`.

```lisp
(planning travel
  (types place)
  (objects (home place) (office place))
  (predicates (at place) (road place place))
  (init (at home) (road home office))
  (goal (at office))
  (action move
    (params (?from place) (?to place))
    (pre (and (at ?from) (road ?from ?to) (not (= ?from ?to))))
    (add (at ?to))
    (del (at ?from))))
```

Atom: `(p term...)`. Equality: `(= t1 t2)`. Formulas:
`(and f...)`, `(or f...)`, `(not f)`, `(imply f g)`,
`(forall ((?x type) ...) f)`, `(exists ((?x type) ...) f)`.
`(and)` is true; `(or)` is false. `init`, `add`, and `del` are lists of atoms,
not general formulas. Parameters use bindings `(name type)`.

## PDDL

Two standard files: `(define (domain name) ...)` and
`(define (problem name) (:domain name) ...)`.
Domain sections: optional `:requirements`, optional flat `:types`, optional
`:constants`, required `:predicates`, repeatable `:action`. Problem sections:
`:domain`, optional `:objects`, `:init`, `:goal`.
PDDL typed lists use `a b - type`; unannotated names have type `object`.
Flat types may declare `- object`; other parent types are rejected. FOL formulas
are the same, with quantified bindings in PDDL syntax `(?x - type)`. Effects:
an atom, a negated atom, or a recursive conjunction of these.

Accepted requirements: `:strips`, `:typing`, `:negative-preconditions`,
`:disjunctive-preconditions`, `:equality`, `:existential-preconditions`,
`:universal-preconditions`, `:quantified-preconditions`.
It is not necessary to declare every feature used; declarations of unsupported
features (including `:adl`, because it includes conditional effects) produce
explicit errors. No negation in `:init`: use CWA. Unknown sections are errors.

## CLI

```text
folplan solve file.fol [--max-states N] [--max-ground-actions N]
folplan pddl domain.pddl problem.pddl [--max-states N] [--max-ground-actions N]
folplan check file.fol
folplan check-pddl domain.pddl problem.pddl
folplan --help
```

Human-readable output: numbered plan, situation term, and states explored.
Exit codes: 0 for a plan or successful validation, 1 for I/O, syntax, semantic,
or argument errors, 2 for impossible, and 3 for a search or grounding limit.
Limits must be positive integers. The CLI must not panic on invalid input.
