# Linguaggi

## DSL FOLPlan (.fol)

S-expression leggibili, commenti `;` fino a fine riga. Identificatori ASCII:
lettera iniziale, poi lettere, cifre, `_`, `-`; variabili con prefisso `?`.
Parole riservate per operatori: `and`, `or`, `not`, `imply`, `exists`, `forall`, `=`.
Tutte le sezioni indicate sono obbligatorie e uniche, eccetto `action` ripetibile.
`types` non include il supertipo implicito `object`.

```lisp
(planning viaggio
  (types place)
  (objects (casa place) (ufficio place))
  (predicates (at place) (road place place))
  (init (at casa) (road casa ufficio))
  (goal (at ufficio))
  (action move
    (params (?from place) (?to place))
    (pre (and (at ?from) (road ?from ?to) (not (= ?from ?to))))
    (add (at ?to))
    (del (at ?from))))
```

Atomo: `(p term...)`. Uguaglianza: `(= t1 t2)`. Formule:
`(and f...)`, `(or f...)`, `(not f)`, `(imply f g)`,
`(forall ((?x type) ...) f)`, `(exists ((?x type) ...) f)`.
`(and)` è vero, `(or)` è falso. `init`, `add`, `del` sono liste di atomi,
non formule generiche. I parametri hanno binding `(nome tipo)`.

## PDDL

Due file standard `(define (domain nome) ...)` e
`(define (problem nome) (:domain nome) ...)`.
Sezioni dominio: `:requirements` opzionale, `:types` opzionale piatto,
`:constants` opzionale, `:predicates` obbligatoria, `:action` ripetibile.
Sezioni problema: `:domain`, `:objects` opzionale, `:init`, `:goal`.
Liste tipate PDDL `a b - type`, senza annotazione = `object`.
Tipi piatti possono dichiarare `- object`; altri genitori rifiutati.
Formule FOL identiche, binding quantificati in sintassi PDDL `(?x - type)`.
Effetti: atomo, negazione di atomo, congiunzione ricorsiva di questi.

Requirements accettati: `:strips`, `:typing`, `:negative-preconditions`,
`:disjunctive-preconditions`, `:equality`, `:existential-preconditions`,
`:universal-preconditions`, `:quantified-preconditions`.
Non è richiesto dichiarare ogni feature usata; dichiarazioni non supportate
(anche `:adl`, perché include effetti condizionali) sono errori espliciti.
Niente negazioni in `:init`: usare CWA. Le sezioni sconosciute sono errori.

## CLI

```text
folplan solve file.fol [--max-states N] [--max-ground-actions N]
folplan pddl domain.pddl problem.pddl [--max-states N] [--max-ground-actions N]
folplan check file.fol
folplan check-pddl domain.pddl problem.pddl
folplan --help
```

Output umano: piano numerato, termine situazione, stati esplorati.
Exit code: 0 piano/validazione, 1 errore I/O/sintassi/semantica/argomenti,
2 impossibile, 3 limite di ricerca/grounding. CLI non panica su input errato.
