# Architettura e contratti

Un crate Rust `pddl_fol`, binario `folplan`, nessuna dipendenza esterna.

```mermaid
classDiagram
    class Task {
        name: String
        types: Vec~String~
        objects: Vec~Binding~
        predicates: Vec~Predicate~
        actions: Vec~Action~
        initial: State
        goal: Formula
    }
    class Action {
        name: String
        parameters: Vec~Binding~
        precondition: Formula
        add: Vec~Atom~
        delete: Vec~Atom~
    }
    class Formula {
        Atom
        Equal
        Not
        And
        Or
        Implies
        Exists
        Forall
    }
    class Plan {
        steps: Vec~GroundAction~
        final_state: State
        explored: usize
        situation(): String
    }
    Task o-- Action
    Task o-- Formula
    Action o-- Formula
    Plan o-- GroundAction
```

```mermaid
flowchart LR
    F[DSL .fol] --> P[parser]
    D[Dominio + problema PDDL] --> P
    P --> T[Task condiviso]
    T --> V[Validazione semantica]
    V --> G[Grounding tipato]
    G --> B[BFS]
    B --> E[Valutazione FOL + transizioni]
    E --> B
    B --> R[Piano / impossibile / limite]
    R --> C[CLI e rustdoc]
```

```mermaid
sequenceDiagram
    actor Utente
    Utente->>CLI: solve modello.fol
    CLI->>Parser: parse_dsl(source)
    Parser-->>CLI: Task oppure Error
    CLI->>Solver: solve(task, limits)
    Solver->>Validator: validate(task)
    Solver->>Evaluator: goal(initial)
    loop Stati BFS finché goal o esaurimento
        Solver->>Evaluator: precondition(state, binding)
        Evaluator-->>Solver: bool
        Solver->>Solver: delete, add, deduplica
    end
    Solver-->>CLI: SearchOutcome
    CLI-->>Utente: piano + situazione + statistiche
```

## Proprietà dei moduli

`model.rs`: strutture dati e `Error`; `logic.rs`: validazione/evaluazione;
`planner.rs`: grounding, BFS, riproduzione piani; `parser.rs`: lexer S-expression
con coordinate, DSL e PDDL; `main.rs`: I/O e argomenti; `lib.rs`: API/rustdoc.

API fissata prima delle deleghe (campi pubblici per costruzione programmatica):

```text
Binding { name: String, ty: String }
Term::{Variable(String), Constant(String)}
Atom { predicate: String, terms: Vec<Term> }
GroundAtom { predicate: String, arguments: Vec<String> }
State = BTreeSet<GroundAtom>
Predicate { name: String, parameters: Vec<String> } // tipi, non nomi
Formula::{Atom(Atom), Equal(Term,Term), Not(Box<Formula>), And(Vec<Formula>),
 Or(Vec<Formula>), Implies(Box<Formula>,Box<Formula>),
 Exists(Vec<Binding>,Box<Formula>), Forall(Vec<Binding>,Box<Formula>)}
Action/Task: campi nel diagramma, delete (non del)
Error { message: String }, Error::new(impl Into<String>), Display, std::error::Error
parse_dsl(&str) -> Result<Task, Error>
parse_pddl(domain: &str, problem: &str) -> Result<Task, Error>
validate(&Task) -> Result<(), Error>
evaluate(&Task, &State, &Formula) -> Result<bool, Error> // formula chiusa
SearchLimits { max_states: usize, max_ground_actions: usize }, Default: 100000 entrambi
GroundAction { name: String, arguments: Vec<String> }, Display: (name args...)
Plan { steps: Vec<GroundAction>, final_state: State, explored: usize }
Plan::situation(&self) -> String
SearchOutcome::{Solved(Plan), Unsolvable { explored: usize }, LimitReached { explored: usize }}
solve(&Task, SearchLimits) -> Result<SearchOutcome, Error>
replay(&Task, &[GroundAction]) -> Result<State, Error>
```

`solve` e `replay` validano sempre Task. `evaluate` controlla formula, stato e
modello; il motore usa internamente una valutazione già validata. Nomi DSL e PDDL
ASCII case-insensitive, normalizzati in minuscolo. Nessuno `unsafe`.
