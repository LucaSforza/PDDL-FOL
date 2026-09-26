use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

use crate::logic::{eval_with_bindings, validate};
use crate::model::{
    Action, Atom, Error, Formula, GroundAction, GroundAtom, Plan, SearchLimits, SearchOutcome,
    State, Task, Term,
};
use agent::problem::{CostructSolution, Problem, SuitableState, Utility};
use agent::statexplorer::resolver::{GraphSearchAlgorithm, GraphSearchOutcome, bounded_search};

const MAX_JOIN_ATOMS: usize = 256;
const MAX_HMAX_GROUND_ACTIONS: usize = 10_000;

/// Finds a shortest plan with A* and admissible heuristics.
pub fn solve(task: &Task, limits: SearchLimits) -> Result<SearchOutcome, Error> {
    solve_with_algorithm(task, limits, crate::model::SearchAlgorithm::AStar)
}

/// Finds a shortest plan using the selected bounded graph-search algorithm.
pub fn solve_with_algorithm(
    task: &Task,
    limits: SearchLimits,
    algorithm: crate::model::SearchAlgorithm,
) -> Result<SearchOutcome, Error> {
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
    let goal_atoms = if algorithm == crate::model::SearchAlgorithm::AStar {
        positive_ground_goal(&task.goal)
    } else {
        None
    };
    let hmax_ground_count = goal_atoms
        .as_ref()
        .and_then(|_| typed_ground_action_count(task, MAX_HMAX_GROUND_ACTIONS));
    let relaxed = if let Some(count) = hmax_ground_count {
        let grounding = ground_actions(task, count.max(1))?;
        Some(build_relaxed_model(task, &grounding)?)
    } else {
        None
    };
    let max_goal_adds = goal_atoms
        .as_ref()
        .map(|goals| goal_add_upper_bound(task, goals));
    let object_order = task
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| (object.name.as_str(), index))
        .collect();
    let problem = PlannerProblem {
        task,
        object_order,
        candidates: RefCell::new(CandidatePool::default()),
        candidate_limit: limits.max_ground_actions,
        relaxed,
        goal_atoms,
        max_goal_adds,
        error: RefCell::new(None),
        hmax_cache: RefCell::new(HashMap::new()),
    };
    let agent_algorithm = match algorithm {
        crate::model::SearchAlgorithm::AStar => GraphSearchAlgorithm::AStar,
        crate::model::SearchAlgorithm::Bfs => GraphSearchAlgorithm::BreadthFirst,
    };
    let result = bounded_search(
        &problem,
        task.initial.clone(),
        agent_algorithm,
        limits.max_states,
    );
    if let Some(error) = problem.error.borrow_mut().take() {
        return Err(error);
    }
    match result {
        GraphSearchOutcome::Solved {
            state,
            actions,
            expanded,
        } => {
            let pool = problem.candidates.borrow();
            let steps = actions
                .iter()
                .map(|index| {
                    pool.actions
                        .get(*index)
                        .map(|(_, action)| action.clone())
                        .ok_or_else(|| Error::new("search returned an invalid action index"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let replayed = replay(task, &steps)?;
            if replayed != state {
                return Err(Error::new(
                    "search returned a plan whose replayed state differs",
                ));
            }
            Ok(SearchOutcome::Solved(Plan {
                steps,
                final_state: state,
                explored: expanded,
            }))
        }
        GraphSearchOutcome::Exhausted { expanded } => {
            Ok(SearchOutcome::Unsolvable { explored: expanded })
        }
        GraphSearchOutcome::LimitReached { expanded } => {
            Ok(SearchOutcome::LimitReached { explored: expanded })
        }
    }
}

#[derive(Clone)]
struct RelaxedAction {
    preconditions: Vec<GroundAtom>,
    add: Vec<GroundAtom>,
}

struct RelaxedModel {
    actions: Vec<RelaxedAction>,
    users: HashMap<GroundAtom, Vec<usize>>,
    empty_precondition_actions: Vec<usize>,
}

#[derive(Default)]
struct CandidatePool {
    actions: Vec<(usize, GroundAction)>,
    indices: BTreeMap<(usize, Vec<String>), usize>,
}

impl CandidatePool {
    fn intern(
        &mut self,
        schema_index: usize,
        action: GroundAction,
        limit: usize,
    ) -> Result<usize, Error> {
        let key = (schema_index, action.arguments.clone());
        if let Some(index) = self.indices.get(&key) {
            return Ok(*index);
        }
        if self.actions.len() >= limit {
            return Err(Error::grounding_limit(format!(
                "ground action limit ({limit}) exceeded"
            )));
        }
        let index = self.actions.len();
        self.actions.push((schema_index, action));
        self.indices.insert(key, index);
        Ok(index)
    }
}

fn build_relaxed_model(
    task: &Task,
    grounded: &[(usize, GroundAction)],
) -> Result<RelaxedModel, Error> {
    let mut actions = Vec::with_capacity(grounded.len());
    for (schema_index, ground) in grounded {
        let schema = &task.actions[*schema_index];
        let env = action_environment(schema, ground);
        let mut preconditions = relaxed_preconditions(&schema.precondition)
            .iter()
            .map(|atom| instantiate(atom, &env))
            .collect::<Result<Vec<_>, _>>()?;
        preconditions.sort_unstable();
        preconditions.dedup();
        let mut add = schema
            .add
            .iter()
            .map(|atom| instantiate(atom, &env))
            .collect::<Result<Vec<_>, _>>()?;
        add.sort_unstable();
        add.dedup();
        actions.push(RelaxedAction { preconditions, add });
    }
    let mut users: HashMap<GroundAtom, Vec<usize>> = HashMap::new();
    let mut empty_precondition_actions = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        if action.preconditions.is_empty() {
            empty_precondition_actions.push(index);
        }
        for precondition in &action.preconditions {
            users.entry(precondition.clone()).or_default().push(index);
        }
    }
    Ok(RelaxedModel {
        actions,
        users,
        empty_precondition_actions,
    })
}

struct PlannerProblem<'a> {
    task: &'a Task,
    object_order: HashMap<&'a str, usize>,
    candidates: RefCell<CandidatePool>,
    candidate_limit: usize,
    relaxed: Option<RelaxedModel>,
    goal_atoms: Option<Vec<GroundAtom>>,
    max_goal_adds: Option<usize>,
    error: RefCell<Option<Error>>,
    hmax_cache: RefCell<HashMap<State, Option<u32>>>,
}

impl PlannerProblem<'_> {
    fn remember_error(&self, error: Error) {
        let mut slot = self.error.borrow_mut();
        if slot.is_none() {
            *slot = Some(error);
        }
    }

    fn relaxed_hmax(&self, state: &State) -> Option<u32> {
        let goals = self.goal_atoms.as_ref()?;
        let model = self.relaxed.as_ref()?;
        if goals.is_empty() {
            return Some(0);
        }
        let mut costs: HashMap<GroundAtom, u32> =
            state.iter().cloned().map(|atom| (atom, 0)).collect();
        let mut agenda = BinaryHeap::new();
        let mut finalized = BTreeSet::new();
        for atom in state {
            agenda.push(Reverse((0, atom.clone())));
        }
        let mut remaining: Vec<usize> = model
            .actions
            .iter()
            .map(|action| action.preconditions.len())
            .collect();
        let mut max_precondition_cost = vec![0; model.actions.len()];
        let mut unresolved_goals = goals.len();
        let mut max_goal_cost = 0;
        for &index in &model.empty_precondition_actions {
            for atom in &model.actions[index].add {
                agenda.push(Reverse((1, atom.clone())));
                costs
                    .entry(atom.clone())
                    .and_modify(|old| *old = (*old).min(1))
                    .or_insert(1);
            }
        }
        while let Some(Reverse((cost, atom))) = agenda.pop() {
            if costs.get(&atom).copied() != Some(cost) || !finalized.insert(atom.clone()) {
                continue;
            }
            if goals.binary_search(&atom).is_ok() {
                unresolved_goals -= 1;
                max_goal_cost = max_goal_cost.max(cost);
                if unresolved_goals == 0 {
                    return Some(max_goal_cost);
                }
            }
            let Some(users) = model.users.get(&atom) else {
                continue;
            };
            for &index in users {
                remaining[index] -= 1;
                max_precondition_cost[index] = max_precondition_cost[index].max(cost);
                if remaining[index] != 0 {
                    continue;
                }
                let effect_cost = max_precondition_cost[index].saturating_add(1);
                for effect in &model.actions[index].add {
                    if costs.get(effect).is_none_or(|old| effect_cost < *old) {
                        costs.insert(effect.clone(), effect_cost);
                        agenda.push(Reverse((effect_cost, effect.clone())));
                    }
                }
            }
        }
        goals
            .iter()
            .map(|goal| costs.get(goal).copied())
            .try_fold(0, |max, cost| cost.map(|cost| max.max(cost)))
    }

    fn goal_cover(&self, state: &State) -> Option<u32> {
        let goals = self.goal_atoms.as_ref()?;
        let missing = goals.iter().filter(|goal| !state.contains(*goal)).count();
        if missing == 0 {
            return Some(0);
        }
        let max_added = self.max_goal_adds?;
        if max_added == 0 {
            return None;
        }
        Some(missing.div_ceil(max_added) as u32)
    }

    fn cached_hmax(&self, state: &State) -> Option<u32> {
        if let Some(cost) = self.hmax_cache.borrow().get(state).copied() {
            return cost;
        }
        let cost = self.relaxed_hmax(state);
        self.hmax_cache.borrow_mut().insert(state.clone(), cost);
        cost
    }

    fn heuristic(&self, state: &State) -> u32 {
        let Some(goals) = self.goal_atoms.as_ref() else {
            return 0;
        };
        let cover = self.goal_cover(state).unwrap_or(0);
        let h_max = if self.relaxed.is_some() {
            self.cached_hmax(state).unwrap_or(0)
        } else {
            0
        };
        if goals.is_empty() {
            0
        } else {
            h_max.max(cover)
        }
    }
}

impl Problem for PlannerProblem<'_> {
    type State = State;
}

impl CostructSolution for PlannerProblem<'_> {
    type Action = usize;
    type Cost = u32;

    fn executable_actions(&self, state: &Self::State) -> impl Iterator<Item = Self::Action> {
        if self.goal_atoms.is_some()
            && (self.goal_cover(state).is_none()
                || (self.relaxed.is_some() && self.cached_hmax(state).is_none()))
        {
            return Vec::new().into_iter();
        }
        let candidates = {
            let mut pool = self.candidates.borrow_mut();
            match applicable_actions(
                self.task,
                state,
                self.candidate_limit,
                &self.object_order,
                &mut pool,
            ) {
                Ok(candidates) => candidates,
                Err(error) => {
                    self.remember_error(error);
                    Vec::new()
                }
            }
        };
        let pool = self.candidates.borrow();
        candidates
            .into_iter()
            .filter(|&index| {
                let (schema_index, ground) = &pool.actions[index];
                let schema = &self.task.actions[*schema_index];
                let mut env = action_environment(schema, ground);
                match eval_with_bindings(self.task, state, &schema.precondition, &mut env) {
                    Ok(applicable) => applicable,
                    Err(error) => {
                        self.remember_error(error);
                        false
                    }
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn result(&self, state: &Self::State, action: &Self::Action) -> (Self::State, Self::Cost) {
        let pool = self.candidates.borrow();
        let Some((schema_index, ground)) = pool.actions.get(*action) else {
            self.remember_error(Error::new("unknown grounded action index"));
            return (state.clone(), 1);
        };
        let schema = &self.task.actions[*schema_index];
        match apply(schema, ground, state) {
            Ok(next) => (next, 1),
            Err(error) => {
                self.remember_error(error);
                (state.clone(), 1)
            }
        }
    }
}

impl Utility for PlannerProblem<'_> {
    fn heuristic(&self, state: &Self::State) -> Self::Cost {
        PlannerProblem::heuristic(self, state)
    }
}

impl SuitableState for PlannerProblem<'_> {
    fn is_suitable(&self, state: &Self::State) -> bool {
        match eval_with_bindings(self.task, state, &self.task.goal, &mut HashMap::new()) {
            Ok(suitable) => suitable,
            Err(error) => {
                self.remember_error(error);
                false
            }
        }
    }
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

fn relaxed_preconditions(formula: &Formula) -> Vec<&Atom> {
    match formula {
        Formula::Atom(atom) => vec![atom],
        Formula::And(parts) => parts.iter().flat_map(relaxed_preconditions).collect(),
        _ => Vec::new(),
    }
}

fn positive_ground_goal(formula: &Formula) -> Option<Vec<GroundAtom>> {
    fn collect(formula: &Formula, atoms: &mut Vec<GroundAtom>) -> Option<()> {
        match formula {
            Formula::Atom(atom) => {
                let arguments = atom
                    .terms
                    .iter()
                    .map(|term| match term {
                        Term::Constant(name) => Some(name.clone()),
                        Term::Variable(_) => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                atoms.push(GroundAtom {
                    predicate: atom.predicate.clone(),
                    arguments,
                });
                Some(())
            }
            Formula::And(parts) => {
                for part in parts {
                    collect(part, atoms)?;
                }
                Some(())
            }
            _ => None,
        }
    }

    let mut atoms = Vec::new();
    collect(formula, &mut atoms)?;
    atoms.sort_unstable();
    atoms.dedup();
    Some(atoms)
}

fn typed_ground_action_count(task: &Task, limit: usize) -> Option<usize> {
    let mut total = 0usize;
    for action in &task.actions {
        let mut count = 1usize;
        for parameter in &action.parameters {
            let domain = task
                .objects
                .iter()
                .filter(|object| parameter.ty == "object" || object.ty == parameter.ty)
                .count();
            count = count.checked_mul(domain)?;
        }
        total = total.checked_add(count)?;
        if total > limit {
            return None;
        }
    }
    Some(total)
}

fn ground_actions(task: &Task, limit: usize) -> Result<Vec<(usize, GroundAction)>, Error> {
    let mut result = Vec::new();
    for (schema_index, action) in task.actions.iter().enumerate() {
        complete_binding(task, action, 0, &mut HashMap::new(), &mut |binding| {
            if result.len() >= limit {
                return Err(Error::new("internal h_max grounding estimate was exceeded"));
            }
            result.push((
                schema_index,
                GroundAction {
                    name: action.name.clone(),
                    arguments: action
                        .parameters
                        .iter()
                        .map(|parameter| binding[&parameter.name].clone())
                        .collect(),
                },
            ));
            Ok(())
        })?;
    }
    Ok(result)
}

fn goal_add_upper_bound(task: &Task, goals: &[GroundAtom]) -> usize {
    task.actions
        .iter()
        .map(|action| {
            action
                .add
                .iter()
                .filter(|effect| {
                    goals.iter().any(|goal| {
                        effect.predicate == goal.predicate
                            && effect.terms.len() == goal.arguments.len()
                    })
                })
                .cloned()
                .collect::<BTreeSet<_>>()
                .len()
        })
        .max()
        .unwrap_or(0)
}

fn applicable_actions(
    task: &Task,
    state: &State,
    limit: usize,
    object_order: &HashMap<&str, usize>,
    pool: &mut CandidatePool,
) -> Result<Vec<usize>, Error> {
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
        {
            let mut emit_binding = |binding: &HashMap<String, String>| {
                let mut binding = binding.clone();
                complete_binding(task, schema, 0, &mut binding, &mut |binding| {
                    let arguments = schema
                        .parameters
                        .iter()
                        .map(|parameter| binding[&parameter.name].clone())
                        .collect::<Vec<_>>();
                    let key = (schema_index, arguments.clone());
                    let candidate = GroundAction {
                        name: schema.name.clone(),
                        arguments,
                    };
                    let index = pool.intern(schema_index, candidate, limit)?;
                    candidates.entry(key).or_insert(index);
                    Ok(())
                })
            };
            if atoms.is_empty() {
                emit_binding(&HashMap::new())?;
            } else {
                join_atoms(state, &atoms, 0, &mut HashMap::new(), &mut emit_binding)?;
            }
        }
    }

    let mut result = candidates.into_iter().collect::<Vec<_>>();
    result.sort_by_key(|((schema_index, arguments), _)| {
        (
            *schema_index,
            arguments
                .iter()
                .map(|argument| object_order[argument.as_str()])
                .collect::<Vec<_>>(),
        )
    });
    Ok(result.into_iter().map(|(_, index)| index).collect())
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

    #[test]
    fn a_star_uses_goal_cover_to_reduce_independent_goal_expansions() {
        let names = ["g0", "g1", "g2", "g3", "g4"];
        let task = Task {
            name: "independent-goals".into(),
            types: vec![],
            objects: vec![],
            predicates: names
                .iter()
                .map(|name| Predicate {
                    name: (*name).into(),
                    parameters: vec![],
                })
                .collect(),
            actions: names
                .iter()
                .map(|name| Action {
                    name: format!("make-{name}"),
                    parameters: vec![],
                    precondition: Formula::And(vec![]),
                    add: vec![Atom {
                        predicate: (*name).into(),
                        terms: vec![],
                    }],
                    delete: vec![],
                })
                .collect(),
            initial: BTreeSet::new(),
            goal: Formula::And(
                names
                    .iter()
                    .map(|name| {
                        Formula::Atom(Atom {
                            predicate: (*name).into(),
                            terms: vec![],
                        })
                    })
                    .collect(),
            ),
        };
        let limits = SearchLimits::default();
        let astar = match solve_with_algorithm(&task, limits, crate::model::SearchAlgorithm::AStar)
            .unwrap()
        {
            SearchOutcome::Solved(plan) => plan,
            other => panic!("expected A* plan, got {other:?}"),
        };
        let bfs = match solve_with_algorithm(&task, limits, crate::model::SearchAlgorithm::Bfs)
            .unwrap()
        {
            SearchOutcome::Solved(plan) => plan,
            other => panic!("expected BFS plan, got {other:?}"),
        };
        assert_eq!(astar.steps.len(), 5);
        assert_eq!(astar.steps.len(), bfs.steps.len());
        assert!(astar.explored < bfs.explored, "A*={astar:?}, BFS={bfs:?}");
        assert_eq!(replay(&task, &astar.steps).unwrap(), astar.final_state);
    }

    #[test]
    fn unsupported_goal_structure_uses_zero_heuristic_and_is_not_pruned() {
        let mut task = travel_task();
        task.goal = Formula::Not(Box::new(Formula::Atom(Atom {
            predicate: "at".into(),
            terms: vec![Term::Constant("a".into())],
        })));
        let astar = solve_with_algorithm(
            &task,
            SearchLimits::default(),
            crate::model::SearchAlgorithm::AStar,
        )
        .unwrap();
        let bfs = solve_with_algorithm(
            &task,
            SearchLimits::default(),
            crate::model::SearchAlgorithm::Bfs,
        )
        .unwrap();
        assert_eq!(astar, bfs);
    }

    #[test]
    fn composite_heuristic_is_admissible_and_consistent_on_a_finite_graph() {
        use std::collections::{BTreeMap, VecDeque};

        let atom = |predicate: &str| Atom {
            predicate: predicate.into(),
            terms: vec![],
        };
        let ground = |predicate: &str| GroundAtom {
            predicate: predicate.into(),
            arguments: vec![],
        };
        let task = Task {
            name: "heuristic-check".into(),
            types: vec![],
            objects: vec![],
            predicates: ["ready", "a", "b", "c"]
                .iter()
                .map(|name| Predicate {
                    name: (*name).into(),
                    parameters: vec![],
                })
                .collect(),
            actions: vec![
                Action {
                    name: "prepare".into(),
                    parameters: vec![],
                    precondition: Formula::And(vec![]),
                    add: vec![atom("ready")],
                    delete: vec![],
                },
                Action {
                    name: "get-a".into(),
                    parameters: vec![],
                    precondition: Formula::Atom(atom("ready")),
                    add: vec![atom("a")],
                    delete: vec![],
                },
                Action {
                    name: "get-b".into(),
                    parameters: vec![],
                    precondition: Formula::Atom(atom("ready")),
                    add: vec![atom("b")],
                    delete: vec![],
                },
                Action {
                    name: "get-c".into(),
                    parameters: vec![],
                    precondition: Formula::Atom(atom("ready")),
                    add: vec![atom("c")],
                    delete: vec![],
                },
                Action {
                    name: "consume-ready".into(),
                    parameters: vec![],
                    precondition: Formula::And(vec![
                        Formula::Atom(atom("b")),
                        Formula::Atom(atom("c")),
                    ]),
                    add: vec![],
                    delete: vec![atom("ready")],
                },
                Action {
                    name: "erase-a".into(),
                    parameters: vec![],
                    precondition: Formula::Atom(atom("b")),
                    add: vec![],
                    delete: vec![atom("a")],
                },
            ],
            initial: BTreeSet::new(),
            goal: Formula::And(vec![
                Formula::Atom(atom("a")),
                Formula::Atom(atom("b")),
                Formula::Atom(atom("c")),
            ]),
        };
        let grounded = ground_actions(&task, 100).unwrap();
        let goals = positive_ground_goal(&task.goal).unwrap();
        let relaxed = build_relaxed_model(&task, &grounded).unwrap();
        let max_goal_adds = goal_add_upper_bound(&task, &goals);
        let object_order = task
            .objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.name.as_str(), index))
            .collect();
        let problem = PlannerProblem {
            task: &task,
            object_order,
            candidates: RefCell::new(CandidatePool::default()),
            candidate_limit: SearchLimits::default().max_ground_actions,
            relaxed: Some(relaxed),
            goal_atoms: Some(goals),
            max_goal_adds: Some(max_goal_adds),
            error: RefCell::new(None),
            hmax_cache: RefCell::new(HashMap::new()),
        };

        let exact_distance = |start: &State| {
            let mut queue = VecDeque::from([(start.clone(), 0usize)]);
            let mut seen = BTreeSet::from([start.clone()]);
            while let Some((state, distance)) = queue.pop_front() {
                if eval_with_bindings(&task, &state, &task.goal, &mut HashMap::new()).unwrap() {
                    return Some(distance);
                }
                for (schema_index, action) in &grounded {
                    let schema = &task.actions[*schema_index];
                    let mut env = action_environment(schema, action);
                    if !eval_with_bindings(&task, &state, &schema.precondition, &mut env).unwrap() {
                        continue;
                    }
                    let next = apply(schema, action, &state).unwrap();
                    if seen.insert(next.clone()) {
                        queue.push_back((next, distance + 1));
                    }
                }
            }
            None
        };

        let mut queue = VecDeque::from([task.initial.clone()]);
        let mut reachable = BTreeMap::from([(task.initial.clone(), ())]);
        let mut saw_hmax_dominate_cover = false;
        let mut saw_cover_dominate_hmax = false;
        while let Some(state) = queue.pop_front() {
            let distance = exact_distance(&state).expect("all reachable states can reach the goal");
            let estimate = problem.heuristic(&state) as usize;
            let h_max = problem.cached_hmax(&state).unwrap() as usize;
            let h_cover = problem.goal_cover(&state).unwrap() as usize;
            saw_hmax_dominate_cover |= h_max > h_cover;
            saw_cover_dominate_hmax |= h_cover > h_max;
            assert!(
                estimate <= distance,
                "heuristic {estimate} overestimates distance {distance} from {state:?}"
            );
            for (schema_index, action) in &grounded {
                let schema = &task.actions[*schema_index];
                let mut env = action_environment(schema, action);
                if !eval_with_bindings(&task, &state, &schema.precondition, &mut env).unwrap() {
                    continue;
                }
                let next = apply(schema, action, &state).unwrap();
                let cost = 1usize;
                let next_estimate = problem.heuristic(&next) as usize;
                assert!(
                    estimate <= cost + next_estimate,
                    "inconsistent edge h({state:?})={estimate} -> h({next:?})={next_estimate}"
                );
                if !reachable.contains_key(&next) {
                    reachable.insert(next.clone(), ());
                    queue.push_back(next);
                }
            }
        }
        assert!(saw_hmax_dominate_cover);
        assert!(saw_cover_dominate_hmax);
        assert!(reachable.contains_key(&BTreeSet::from([
            ground("ready"),
            ground("a"),
            ground("b"),
            ground("c")
        ])));
    }
}
