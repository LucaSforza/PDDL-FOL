//! Classical planning with finite-domain first-order logic.
//!
//! Parse the FOLPlan DSL or a documented PDDL subset, validate the model, then
//! find a shortest sequential plan using breadth-first search. The planner uses
//! closed-world and unique-name semantics; it is not a general FOL prover.
//!
//! See the [`guide`] for the language tutorial and executable Rust examples.

#![forbid(unsafe_code)]

mod dsl;
mod logic;
mod model;
mod parser;
mod planner;

pub use dsl::parse_dsl;
pub use logic::{evaluate, validate};
pub use model::*;
pub use parser::parse_pddl;
pub use planner::{replay, solve};

/// Language reference and modeling tutorial.
#[doc = include_str!("../docs/guide.md")]
pub mod guide {}
