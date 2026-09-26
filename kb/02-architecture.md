# Architecture and contracts

A Rust crate `pddl_fol`, with a `folplan` binary and a path dependency on the
`agent` Git submodule at `vendor/agent`. Agent owns the generic search frontier
and node lifetime; `pddl_fol` owns planning semantics and limits.

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
    D[Domain + PDDL problem] --> P
    P --> T[Shared Task]
    T --> V[Semantic validation]
    V --> B[A* or BFS via Agent]
    B --> G[Lifted candidate joins]
    G --> E[FOL evaluation + transitions]
    E --> B
    B --> R[Plan / impossible / limit]
    R --> C[CLI and rustdoc]
```

```mermaid
sequenceDiagram
    actor User
    User->>CLI: solve model.fol
    CLI->>Parser: parse_dsl(source)
    Parser-->>CLI: Task or Error
    CLI->>Solver: solve(task, limits)
    Solver->>Validator: validate(task)
    Solver->>Evaluator: goal(initial)
    loop A* or BFS states until goal or exhaustion
        Solver->>Solver: join positive conjuncts with state facts
        Solver->>Solver: complete remaining typed parameters
        Solver->>Evaluator: precondition(state, binding)
        Evaluator-->>Solver: bool
        Solver->>Solver: delete, add, deduplicate
    end
    Solver-->>CLI: SearchOutcome
    CLI-->>User: plan + situation + statistics
```

## Module responsibilities

`model.rs`: data structures and `Error`; `logic.rs`: validation/evaluation;
`planner.rs`: lifted candidate generation, Agent adapter, admissible heuristic,
plan replay; `dsl.rs`: coordinate-aware infix DSL
lexer/parser; `parser.rs`: standard PDDL S-expression parser; `main.rs`: I/O and arguments; `lib.rs`: API
and rustdoc.

API fixed before delegation (public fields for programmatic construction):

```text
Binding { name: String, ty: String } // variables stored without a '?' prefix
Term::{Variable(String), Constant(String)}
Atom { predicate: String, terms: Vec<Term> }
GroundAtom { predicate: String, arguments: Vec<String> }
State = BTreeSet<GroundAtom>
Predicate { name: String, parameters: Vec<String> } // types, not names
Formula::{Atom(Atom), Equal(Term,Term), Not(Box<Formula>), And(Vec<Formula>),
 Or(Vec<Formula>), Implies(Box<Formula>,Box<Formula>),
 Exists(Vec<Binding>,Box<Formula>), Forall(Vec<Binding>,Box<Formula>)}
Action/Task: fields from diagram, delete (not del)
Error { message: String }, Error::new(impl Into<String>), Display, std::error::Error
ErrorKind::{InvalidInput, GroundingLimit}; Error::kind() -> ErrorKind
parse_dsl(&str) -> Result<Task, Error>
parse_pddl(domain: &str, problem: &str) -> Result<Task, Error>
validate(&Task) -> Result<(), Error>
evaluate(&Task, &State, &Formula) -> Result<bool, Error> // closed formula
SearchLimits { max_states: usize, max_ground_actions: usize }, Default: 100000 each
GroundAction { name: String, arguments: Vec<String> }, Display: name(arg1, arg2)
Plan { steps: Vec<GroundAction>, final_state: State, explored: usize }
Plan::situation(&self) -> String
SearchOutcome::{Solved(Plan), Unsolvable { explored: usize }, LimitReached { explored: usize }}
SearchAlgorithm::{AStar, Bfs}
solve(&Task, SearchLimits) -> Result<SearchOutcome, Error> // defaults to AStar
solve_with_algorithm(&Task, SearchLimits, SearchAlgorithm) -> Result<SearchOutcome, Error>
replay(&Task, &[GroundAction]) -> Result<State, Error>
```

`solve` and `replay` always validate the Task. `evaluate` checks the formula,
state, and model; the engine internally uses an already-validated evaluation.
`max_ground_actions` caps distinct schema/argument candidates emitted across
search states, after positive conjunctive filtering and before full formula
evaluation. Other precondition forms use bounded Cartesian fallback.
All search algorithms use this lifted candidate contract. A* uses the
goal-cover bound for positive conjunctive ground goals. It may combine this
with delete-relaxed `h_max` only when the full typed Cartesian grounding is
small enough to materialize safely without changing the candidate budget.
For other goals, the heuristic is zero. A* must preserve optimality and
not discard valid states when a heuristic estimate cannot be computed.
Grounding-limit errors have kind `ErrorKind::GroundingLimit`; all other model
and input errors have kind `ErrorKind::InvalidInput`. DSL and PDDL names are
ASCII case-insensitive and normalized to lowercase. No `unsafe`.

Agent's public convenience search entry point owns its arena, supports a
distinct-state limit and reports exhausted/limited/solved separately. It
preserves its existing lower-level `Explorer` API. Its A* graph search tracks
best costs and reopens improved states; frontier replacement never loses an
improved path. `pddl_fol` maps Agent outcomes to `SearchOutcome` and verifies
each returned plan by replay. Other Agent algorithms and examples retain their
current contracts.
