use std::collections::BTreeSet;
use std::fmt;

/// A typed name used for an object, an action parameter, or a quantified variable.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Binding {
    pub name: String,
    pub ty: String,
}

/// A term is either a variable name or a named object.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Term {
    /// A bare variable name; the parser removes the DSL's leading `?`.
    Variable(String),
    /// The name of a declared object.
    Constant(String),
}

/// A possibly non-ground predicate application.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Atom {
    pub predicate: String,
    pub terms: Vec<Term>,
}

/// A ground predicate application.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroundAtom {
    pub predicate: String,
    pub arguments: Vec<String>,
}

/// The set of ground atoms true in a state; all other atoms are false (CWA).
pub type State = BTreeSet<GroundAtom>;

/// A predicate declaration. `parameters` contains argument types, not names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Predicate {
    pub name: String,
    pub parameters: Vec<String>,
}

/// A classical deterministic action schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub name: String,
    pub parameters: Vec<Binding>,
    pub precondition: Formula,
    pub add: Vec<Atom>,
    pub delete: Vec<Atom>,
}

/// A first-order formula interpreted over the finite object domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Formula {
    Atom(Atom),
    Equal(Term, Term),
    Not(Box<Formula>),
    And(Vec<Formula>),
    Or(Vec<Formula>),
    Implies(Box<Formula>, Box<Formula>),
    Exists(Vec<Binding>, Box<Formula>),
    Forall(Vec<Binding>, Box<Formula>),
}

/// A complete finite planning task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub name: String,
    pub types: Vec<String>,
    pub objects: Vec<Binding>,
    pub predicates: Vec<Predicate>,
    pub actions: Vec<Action>,
    pub initial: State,
    pub goal: Formula,
}

/// The category of a planning error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidInput,
    GroundingLimit,
}

/// An error raised while validating, evaluating, grounding, or executing a task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    kind: ErrorKind,
}

impl Error {
    /// Creates an invalid-input error with a human-readable explanation.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::InvalidInput,
        }
    }

    /// Returns the category used by callers to distinguish bounded failures.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub(crate) fn grounding_limit(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::GroundingLimit,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// A fully instantiated action.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroundAction {
    pub name: String,
    pub arguments: Vec<String>,
}

impl fmt::Display for GroundAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.name)?;
        for (index, argument) in self.arguments.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            f.write_str(argument)?;
        }
        f.write_str(")")
    }
}

/// Limits on the number of states stored and ground actions generated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchLimits {
    pub max_states: usize,
    pub max_ground_actions: usize,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            max_states: 100_000,
            max_ground_actions: 100_000,
        }
    }
}

/// A shortest plan and the state reached after executing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<GroundAction>,
    pub final_state: State,
    pub explored: usize,
}

impl Plan {
    /// Renders the situation-calculus witness for this action sequence.
    pub fn situation(&self) -> String {
        let mut situation = "S0".to_owned();
        for step in &self.steps {
            situation = format!("do({step}, {situation})");
        }
        situation
    }
}

/// The result of an exhaustive bounded breadth-first search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchOutcome {
    Solved(Plan),
    Unsolvable { explored: usize },
    LimitReached { explored: usize },
}
