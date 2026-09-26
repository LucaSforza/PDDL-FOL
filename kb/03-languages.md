# Languages

## FOLPlan DSL (.fol)

Slide-style declarations, infix logical operators, and `//` comments through
the end of the line. The entire `.fol` source, including comments, is ASCII;
non-ASCII characters are input errors with a source location. ASCII identifiers:
initial letter, followed by letters, digits, `_`, or `-`; names normalize to
lowercase. Parameters and quantified variables use bare names, resolved by
lexical scope. Other terms name objects.
Every listed section is required and unique, except that `action` may repeat.
`types` does not include the implicit supertype `object`.

```text
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

| Meaning | DSL form | Also accepted (ASCII) |
| --- | --- | --- |
| Atom | `On(b, table)` | -- |
| Negation | `not F` | `!F` |
| Conjunction | `F and G` | `F & G` or `F && G` |
| Disjunction | `F or G` | `F \| G` or `F \|\| G` |
| Implication | `F implies G` | `F -> G` |
| Equality / inequality | `x = y`, `x != y` | -- |
| Universal | `forall x: block . F` | -- |
| Existential | `exists x: block . F` | -- |

Operator precedence, weakest first: implication (right associative),
disjunction, conjunction, negation/atoms. Keywords are case-insensitive and
reserved as identifiers. Parentheses group formulas, and
predicate calls use familiar comma-separated arguments. Quantifier bodies
extend to the right delimiter: use `(forall x: block . F)` to limit scope
inside a larger formula. Multiple variables: `forall x: block, y: block . F`.
`true` and `false` are Boolean formulas; zero-argument predicates use `Ready()`.

`init` permits positive atoms and conjunctions only; `true` denotes an empty
state. `effect` permits conjunctions of positive or negated atoms, or `true`
for no effects. The parser lowers positive literals into Add and negative
literals into Delete. `pre` permits any supported FOL formula.

```ebnf
task       = "problem", name, "{", { section | action }, "}" ;
types      = "types", [ name, { ",", name } ], ";" ;
objects    = "objects", "{", { names, ":", type, ";" }, "}" ;
predicates = "predicates", "{", { name, "(", [ types_list ], ")", ";" }, "}" ;
init       = "init", ":", formula, ";" ;
goal       = "goal", ":", formula, ";" ;
action     = "action", name, "(", [ bindings ], ")", "{", pre, effect, "}" ;
pre        = "pre", ":", formula, ";" ;
effect     = "effect", ":", formula, ";" ;
bindings   = name, ":", type, { ",", name, ":", type } ;
```

Top sections may appear in any order. Each action needs exactly one `pre`
and one `effect`. Empty declarations: `types;`, `objects {}`, `predicates {}`.
The old S-expression custom DSL is replaced, not retained as a second mode.

## PDDL

Two standard files: `(define (domain name) ...)` and
`(define (problem name) (:domain name) ...)`.
Both files use ASCII throughout, including comments; non-ASCII input is an
error with a source location.
Domain sections: optional `:requirements`, optional flat `:types`, optional
`:constants`, required `:predicates`, repeatable `:action`. Problem sections:
`:domain`, optional `:objects`, `:init`, `:goal`.
PDDL typed lists use `a b - type`; unannotated names have type `object`.
Flat types may declare `- object`; other parent types are rejected. The same FOL
operators use standard PDDL S-expressions and `?` variables, e.g.
`(forall (?x - block) (clear ?x))`. Effects:
an atom, a negated atom, or a recursive conjunction of these.

Accepted requirements: `:strips`, `:typing`, `:negative-preconditions`,
`:disjunctive-preconditions`, `:equality`, `:existential-preconditions`,
`:universal-preconditions`, `:quantified-preconditions`.
It is not necessary to declare every feature used; declarations of unsupported
features (including `:adl`, because it includes conditional effects) produce
explicit errors. No negation in `:init`: use CWA. Unknown sections are errors.

## CLI

```text
folplan solve file.fol [--search astar|bfs] [--max-states N] [--max-ground-actions N]
folplan pddl domain.pddl problem.pddl [--search astar|bfs] [--max-states N] [--max-ground-actions N]
folplan check file.fol
folplan check-pddl domain.pddl problem.pddl
folplan --help
```

Human-readable output: numbered plan, situation term, and states explored.
Exit codes: 0 for a plan or successful validation, 1 for I/O, syntax, semantic,
or argument errors, 2 for impossible, and 3 for a search or grounding limit.
Limits must be positive integers. The CLI must not panic on invalid input.
Default search is `astar`; `bfs` is retained for reference and for comparing
optimal plan lengths. Search selection never changes parsing or transition
semantics.
