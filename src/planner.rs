use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::logic::{eval_with_bindings, validate};
use crate::model::{
    Action, Atom, Error, GroundAction, GroundAtom, Plan, SearchLimits, SearchOutcome, State, Task,
    Term,
};

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
    let grounded = ground_actions(task, limits.max_ground_actions)?;

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

        for (schema_index, ground) in &grounded {
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

fn ground_actions(task: &Task, limit: usize) -> Result<Vec<(usize, GroundAction)>, Error> {
    let mut result = Vec::new();
    for (schema_index, action) in task.actions.iter().enumerate() {
        if action.parameters.iter().any(|parameter| {
            !task
                .objects
                .iter()
                .any(|object| parameter.ty == "object" || object.ty == parameter.ty)
        }) {
            continue;
        }
        let mut args = Vec::with_capacity(action.parameters.len());
        enumerate_parameters(task, schema_index, action, 0, &mut args, limit, &mut result)?;
    }
    Ok(result)
}

fn enumerate_parameters(
    task: &Task,
    schema_index: usize,
    schema: &Action,
    index: usize,
    args: &mut Vec<String>,
    limit: usize,
    result: &mut Vec<(usize, GroundAction)>,
) -> Result<(), Error> {
    if index == schema.parameters.len() {
        if result.len() >= limit {
            return Err(Error::grounding_limit(format!(
                "ground action limit ({limit}) exceeded"
            )));
        }
        let ground = GroundAction {
            name: schema.name.clone(),
            arguments: args.clone(),
        };
        result.push((schema_index, ground));
        return Ok(());
    }
    let parameter = &schema.parameters[index];
    for object in task
        .objects
        .iter()
        .filter(|object| parameter.ty == "object" || object.ty == parameter.ty)
    {
        args.push(object.name.clone());
        enumerate_parameters(task, schema_index, schema, index + 1, args, limit, result)?;
        args.pop();
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
