use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use crate::logic::{eval_with_bindings, validate};
use crate::model::{
    Action, Atom, Error, Formula, GroundAction, GroundAtom, Plan, SearchLimits, SearchOutcome,
    State, Task, Term,
};

const MAX_JOIN_ATOMS: usize = 256;

/// Finds a shortest plan with forward breadth-first search.
pub fn solve(task: &Task, limits: SearchLimits) -> Result<SearchOutcome, Error> {
    validate(task)?;
    if limits.max_states == 0 {
        return Ok(SearchOutcome::LimitReached { explored: 0 });
    }
    if eval_with_bindings(task, &task.initial, &task.goal, &mut HashMap::new())? {
        return Ok(SearchOutcome::Solved(Plan {
            steps: Vec::new(),
            final_state: task.initial.clone(),
            explored: 1,
        }));
    }
    let object_order: HashMap<&str, usize> = task
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| (object.name.as_str(), index))
        .collect();
    let mut generated = BTreeSet::new();

    struct Node {
        state: State,
        parent: Option<(usize, GroundAction)>,
    }

    let mut nodes = vec![Node {
        state: task.initial.clone(),
        parent: None,
    }];
    let mut seen = BTreeMap::from([(task.initial.clone(), 0usize)]);
    let mut queue = VecDeque::from([0usize]);
    let mut explored = 0;
    while let Some(index) = queue.pop_front() {
        explored += 1;
        let state = nodes[index].state.clone();
        if eval_with_bindings(task, &state, &task.goal, &mut HashMap::new())? {
            let mut steps = Vec::new();
            let mut cursor = index;
            while let Some((parent, action)) = nodes[cursor].parent.clone() {
                steps.push(action);
                cursor = parent;
            }
            steps.reverse();
            return Ok(SearchOutcome::Solved(Plan {
                steps,
                final_state: nodes[index].state.clone(),
                explored,
            }));
        }

        let candidates = applicable_actions(
            task,
            &state,
            limits.max_ground_actions,
            &object_order,
            &mut generated,
        )?;
        for (schema_index, ground) in &candidates {
            let schema = &task.actions[*schema_index];
            let env = action_environment(schema, ground);
            if !eval_with_bindings(task, &state, &schema.precondition, &mut env.clone())? {
                continue;
            }
            let next = apply(schema, ground, &state)?;
            if seen.contains_key(&next) {
                continue;
            }
            if nodes.len() >= limits.max_states {
                return Ok(SearchOutcome::LimitReached { explored });
            }
            let next_index = nodes.len();
            seen.insert(next.clone(), next_index);
            nodes.push(Node {
                state: next,
                parent: Some((index, ground.clone())),
            });
            queue.push_back(next_index);
        }
    }
    Ok(SearchOutcome::Unsolvable { explored })
}

/// Replays ground actions from the initial state, rejecting inapplicable steps.
pub fn replay(task: &Task, steps: &[GroundAction]) -> Result<State, Error> {
    validate(task)?;
    let mut state = task.initial.clone();
    for (step_index, ground) in steps.iter().enumerate() {
        let schema = task
            .actions
            .iter()
            .find(|action| action.name == ground.name)
            .ok_or_else(|| {
                Error::new(format!(
                    "step {} names unknown action `{}`",
                    step_index + 1,
                    ground.name
                ))
            })?;
        let env = checked_action_environment(task, schema, ground)?;
        if !eval_with_bindings(task, &state, &schema.precondition, &mut env.clone())? {
            return Err(Error::new(format!(
                "action `{ground}` is not applicable at step {}",
                step_index + 1
            )));
        }
        state = apply(schema, ground, &state)?;
    }
    Ok(state)
}

// Only atoms reached through conjunction must hold in every applicable state.
fn positive_conjuncts<'a>(formula: &'a Formula, atoms: &mut Vec<&'a Atom>) {
    // Remaining conjuncts stay in the full formula check; this only bounds join recursion.
    if atoms.len() == MAX_JOIN_ATOMS {
        return;
    }
    match formula {
        Formula::Atom(atom) => atoms.push(atom),
        Formula::And(parts) => {
            for part in parts {
                if atoms.len() == MAX_JOIN_ATOMS {
                    break;
                }
                positive_conjuncts(part, atoms);
            }
        }
        _ => {}
    }
}

fn applicable_actions(
    task: &Task,
    state: &State,
    limit: usize,
    object_order: &HashMap<&str, usize>,
    generated: &mut BTreeSet<(usize, Vec<String>)>,
) -> Result<Vec<(usize, GroundAction)>, Error> {
    let mut candidates = BTreeMap::new();
    for (schema_index, schema) in task.actions.iter().enumerate() {
        let mut atoms = Vec::new();
        positive_conjuncts(&schema.precondition, &mut atoms);
        // Bind from the most selective available relation first.
        atoms.sort_by_key(|atom| {
            state
                .iter()
                .filter(|fact| {
                    fact.predicate == atom.predicate && fact.arguments.len() == atom.terms.len()
                })
                .count()
        });
        let mut emit_binding = |binding: &HashMap<String, String>| {
            let mut binding = binding.clone();
            complete_binding(task, schema, 0, &mut binding, &mut |binding| {
                let arguments = schema
                    .parameters
                    .iter()
                    .map(|parameter| binding[&parameter.name].clone())
                    .collect::<Vec<_>>();
                let key = (schema_index, arguments.clone());
                if generated.insert(key.clone()) && generated.len() > limit {
                    return Err(Error::grounding_limit(format!(
                        "ground action limit ({limit}) exceeded"
                    )));
                }
                candidates.entry(key).or_insert_with(|| GroundAction {
                    name: schema.name.clone(),
                    arguments,
                });
                Ok(())
            })
        };
        if atoms.is_empty() {
            emit_binding(&HashMap::new())?;
        } else {
            join_atoms(state, &atoms, 0, &mut HashMap::new(), &mut emit_binding)?;
        }
    }

    let mut result = candidates
        .into_iter()
        .map(|((schema_index, arguments), ground)| (schema_index, arguments, ground))
        .collect::<Vec<_>>();
    result.sort_by_key(|(schema_index, arguments, _)| {
        (
            *schema_index,
            arguments
                .iter()
                .map(|argument| object_order[argument.as_str()])
                .collect::<Vec<_>>(),
        )
    });
    Ok(result
        .into_iter()
        .map(|(schema_index, _, ground)| (schema_index, ground))
        .collect())
}

fn join_atoms(
    state: &State,
    atoms: &[&Atom],
    index: usize,
    binding: &mut HashMap<String, String>,
    emit: &mut impl FnMut(&HashMap<String, String>) -> Result<(), Error>,
) -> Result<(), Error> {
    if index == atoms.len() {
        return emit(binding);
    }
    let atom = atoms[index];
    for fact in state
        .iter()
        .filter(|fact| fact.predicate == atom.predicate && fact.arguments.len() == atom.terms.len())
    {
        let mut inserted = Vec::new();
        let matches = atom
            .terms
            .iter()
            .zip(&fact.arguments)
            .all(|(term, value)| match term {
                Term::Constant(name) => name == value,
                Term::Variable(name) => match binding.get(name) {
                    Some(bound) => bound == value,
                    None => {
                        binding.insert(name.clone(), value.clone());
                        inserted.push(name.clone());
                        true
                    }
                },
            });
        if matches {
            join_atoms(state, atoms, index + 1, binding, emit)?;
        }
        for name in inserted {
            binding.remove(&name);
        }
    }
    Ok(())
}

fn complete_binding(
    task: &Task,
    schema: &Action,
    index: usize,
    binding: &mut HashMap<String, String>,
    emit: &mut impl FnMut(&HashMap<String, String>) -> Result<(), Error>,
) -> Result<(), Error> {
    if index == schema.parameters.len() {
        return emit(binding);
    }
    let parameter = &schema.parameters[index];
    if let Some(value) = binding.get(&parameter.name) {
        let type_matches = task.objects.iter().any(|object| {
            object.name == *value && (parameter.ty == "object" || object.ty == parameter.ty)
        });
        if !type_matches {
            return Ok(());
        }
        return complete_binding(task, schema, index + 1, binding, emit);
    }
    for object in task
        .objects
        .iter()
        .filter(|object| parameter.ty == "object" || object.ty == parameter.ty)
    {
        binding.insert(parameter.name.clone(), object.name.clone());
        complete_binding(task, schema, index + 1, binding, emit)?;
        binding.remove(&parameter.name);
    }
    Ok(())
}

fn action_environment(schema: &Action, ground: &GroundAction) -> HashMap<String, String> {
    schema
        .parameters
        .iter()
        .zip(&ground.arguments)
        .map(|(parameter, object)| (parameter.name.clone(), object.clone()))
        .collect()
}

fn checked_action_environment(
    task: &Task,
    schema: &Action,
    ground: &GroundAction,
) -> Result<HashMap<String, String>, Error> {
    if schema.parameters.len() != ground.arguments.len() {
        return Err(Error::new(format!(
            "action `{}` expects {} arguments, got {}",
            schema.name,
            schema.parameters.len(),
            ground.arguments.len()
        )));
    }
    let mut env = HashMap::new();
    for (parameter, argument) in schema.parameters.iter().zip(&ground.arguments) {
        let object = task
            .objects
            .iter()
            .find(|object| object.name == *argument)
            .ok_or_else(|| {
                Error::new(format!("unknown object `{argument}` in action `{ground}`"))
            })?;
        if parameter.ty != "object" && parameter.ty != object.ty {
            return Err(Error::new(format!(
                "object `{argument}` has type `{}`, expected `{}` for parameter `{}`",
                object.ty, parameter.ty, parameter.name
            )));
        }
        env.insert(parameter.name.clone(), argument.clone());
    }
    Ok(env)
}

fn apply(schema: &Action, ground: &GroundAction, state: &State) -> Result<State, Error> {
    let env = action_environment(schema, ground);
    let mut next = state.clone();
    for atom in &schema.delete {
        next.remove(&instantiate(atom, &env)?);
    }
    for atom in &schema.add {
        next.insert(instantiate(atom, &env)?);
    }
    Ok(next)
}

fn instantiate(atom: &Atom, env: &HashMap<String, String>) -> Result<GroundAtom, Error> {
    let arguments = atom
        .terms
        .iter()
        .map(|term| match term {
            Term::Constant(name) => Ok(name.clone()),
            Term::Variable(name) => env
                .get(name)
                .cloned()
                .ok_or_else(|| Error::new(format!("unbound effect variable `?{name}`"))),
        })
        .collect::<Result<_, _>>()?;
    Ok(GroundAtom {
        predicate: atom.predicate.clone(),
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Binding, Formula, Predicate};
    use std::collections::BTreeSet;

    fn travel_task() -> Task {
        let at = |name: &str| Atom {
            predicate: "at".into(),
            terms: vec![Term::Constant(name.into())],
        };
        Task {
            name: "travel".into(),
            types: vec!["place".into()],
            objects: ["a", "b", "c"]
                .iter()
                .map(|name| Binding {
                    name: (*name).into(),
                    ty: "place".into(),
                })
                .collect(),
            predicates: vec![Predicate {
                name: "at".into(),
                parameters: vec!["place".into()],
            }],
            actions: vec![Action {
                name: "move".into(),
                parameters: vec![
                    Binding {
                        name: "from".into(),
                        ty: "place".into(),
                    },
                    Binding {
                        name: "to".into(),
                        ty: "place".into(),
                    },
                ],
                precondition: Formula::Atom(Atom {
                    predicate: "at".into(),
                    terms: vec![Term::Variable("from".into())],
                }),
                add: vec![Atom {
                    predicate: "at".into(),
                    terms: vec![Term::Variable("to".into())],
                }],
                delete: vec![Atom {
                    predicate: "at".into(),
                    terms: vec![Term::Variable("from".into())],
                }],
            }],
            initial: BTreeSet::from([GroundAtom {
                predicate: "at".into(),
                arguments: vec!["a".into()],
            }]),
            goal: Formula::Atom(at("c")),
        }
    }

    #[test]
    fn bfs_finds_minimal_plan_and_replay_matches_it() {
        let task = travel_task();
        let plan = match solve(&task, SearchLimits::default()).unwrap() {
            SearchOutcome::Solved(plan) => plan,
            other => panic!("expected plan, got {other:?}"),
        };
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].to_string(), "move(a, c)");
        assert_eq!(replay(&task, &plan.steps).unwrap(), plan.final_state);
        assert_eq!(plan.situation(), "do(move(a, c), S0)");
    }

    #[test]
    fn disjunction_does_not_filter_applicable_actions() {
        let mut task = travel_task();
        task.actions[0].precondition = Formula::Or(vec![
            Formula::Atom(Atom {
                predicate: "at".into(),
                terms: vec![Term::Constant("b".into())],
            }),
            Formula::And(vec![]),
        ]);
        let SearchOutcome::Solved(plan) = solve(&task, SearchLimits::default()).unwrap() else {
            panic!("true disjunct must keep every action applicable");
        };
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(replay(&task, &plan.steps).unwrap(), plan.final_state);
    }

    #[test]
    fn sparse_high_arity_precondition_avoids_cartesian_grounding() {
        let objects = (0..32)
            .map(|index| Binding {
                name: format!("o{index}"),
                ty: "object".into(),
            })
            .collect::<Vec<_>>();
        let parameters = (0..8)
            .map(|index| Binding {
                name: format!("x{index}"),
                ty: "object".into(),
            })
            .collect::<Vec<_>>();
        let vars = parameters
            .iter()
            .map(|parameter| Term::Variable(parameter.name.clone()))
            .collect::<Vec<_>>();
        let task = Task {
            name: "sparse-high-arity".into(),
            types: vec![],
            objects,
            predicates: vec![
                Predicate {
                    name: "tuple".into(),
                    parameters: vec!["object".into(); 8],
                },
                Predicate {
                    name: "done".into(),
                    parameters: vec![],
                },
            ],
            actions: vec![Action {
                name: "finish".into(),
                parameters,
                precondition: Formula::Atom(Atom {
                    predicate: "tuple".into(),
                    terms: vars,
                }),
                add: vec![Atom {
                    predicate: "done".into(),
                    terms: vec![],
                }],
                delete: vec![],
            }],
            initial: BTreeSet::from([GroundAtom {
                predicate: "tuple".into(),
                arguments: (0..8).map(|index| format!("o{index}")).collect(),
            }]),
            goal: Formula::Atom(Atom {
                predicate: "done".into(),
                terms: vec![],
            }),
        };
        let SearchOutcome::Solved(plan) = solve(
            &task,
            SearchLimits {
                max_states: 2,
                max_ground_actions: 1,
            },
        )
        .unwrap() else {
            panic!("the sparse high-arity action should solve within one candidate");
        };
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(
            plan.steps[0].arguments,
            (0..8).map(|i| format!("o{i}")).collect::<Vec<_>>()
        );
    }

    #[test]
    fn joined_atoms_respect_action_parameter_types() {
        let task = Task {
            name: "typed-join".into(),
            types: vec!["place".into(), "item".into()],
            objects: vec![
                Binding {
                    name: "home".into(),
                    ty: "place".into(),
                },
                Binding {
                    name: "foreign".into(),
                    ty: "item".into(),
                },
            ],
            predicates: vec![
                Predicate {
                    name: "p".into(),
                    parameters: vec!["object".into()],
                },
                Predicate {
                    name: "done".into(),
                    parameters: vec![],
                },
            ],
            actions: vec![Action {
                name: "finish".into(),
                parameters: vec![Binding {
                    name: "x".into(),
                    ty: "place".into(),
                }],
                precondition: Formula::Atom(Atom {
                    predicate: "p".into(),
                    terms: vec![Term::Variable("x".into())],
                }),
                add: vec![Atom {
                    predicate: "done".into(),
                    terms: vec![],
                }],
                delete: vec![],
            }],
            initial: BTreeSet::from([GroundAtom {
                predicate: "p".into(),
                arguments: vec!["foreign".into()],
            }]),
            goal: Formula::Atom(Atom {
                predicate: "done".into(),
                terms: vec![],
            }),
        };
        assert!(matches!(
            solve(&task, SearchLimits::default()).unwrap(),
            SearchOutcome::Unsolvable { .. }
        ));
    }

    #[test]
    fn large_flat_conjunction_uses_bounded_join_and_full_evaluation() {
        let mut task = travel_task();
        task.actions[0].precondition = Formula::And(
            (0..300)
                .map(|_| {
                    Formula::Atom(Atom {
                        predicate: "at".into(),
                        terms: vec![Term::Variable("from".into())],
                    })
                })
                .collect(),
        );
        let SearchOutcome::Solved(plan) = solve(&task, SearchLimits::default()).unwrap() else {
            panic!("all 300 positive conjuncts hold for the source location");
        };
        assert_eq!(plan.steps.len(), 1);
    }

    #[test]
    fn disjunction_and_quantifier_fall_back_to_bounded_grounding() {
        let fallback_preconditions = [
            Formula::Or(vec![
                Formula::Atom(Atom {
                    predicate: "at".into(),
                    terms: vec![Term::Constant("b".into())],
                }),
                Formula::And(vec![]),
            ]),
            Formula::Exists(
                vec![Binding {
                    name: "witness".into(),
                    ty: "place".into(),
                }],
                Box::new(Formula::Equal(
                    Term::Variable("witness".into()),
                    Term::Variable("to".into()),
                )),
            ),
        ];
        for precondition in fallback_preconditions {
            let mut task = travel_task();
            task.actions[0].precondition = precondition;
            let error = solve(
                &task,
                SearchLimits {
                    max_states: 100,
                    max_ground_actions: 1,
                },
            )
            .unwrap_err();
            assert_eq!(error.kind(), crate::model::ErrorKind::GroundingLimit);
        }
    }

    #[test]
    fn grounding_and_state_limits_are_distinct_from_unsolvable() {
        let task = travel_task();
        let error = solve(
            &task,
            SearchLimits {
                max_states: 100,
                max_ground_actions: 1,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind(), crate::model::ErrorKind::GroundingLimit);
        assert!(matches!(
            solve(
                &task,
                SearchLimits {
                    max_states: 1,
                    max_ground_actions: 100
                }
            )
            .unwrap(),
            SearchOutcome::LimitReached { .. }
        ));
    }

    #[test]
    fn replay_rejects_inapplicable_actions() {
        let task = travel_task();
        let error = replay(
            &task,
            &[GroundAction {
                name: "move".into(),
                arguments: vec!["b".into(), "c".into()],
            }],
        )
        .unwrap_err();
        assert!(error.message.contains("not applicable"));
    }
}
