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
    if algorithm == crate::model::SearchAlgorithm::AStar {
        if let Some(outcome) = solve_product_navigation(task, limits)? {
            return Ok(outcome);
        }
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

const MAX_PRODUCT_TARGETS: usize = 3;

#[derive(Clone)]
struct NavigationAction {
    schema_index: usize,
    axis: usize,
}

struct ProductNavigation {
    start: Vec<String>,
    goals: Vec<Vec<String>>,
    initially_visited: BTreeSet<Vec<String>>,
    neighbors: Vec<(String, String)>,
    actions: Vec<NavigationAction>,
}

fn solve_product_navigation(
    task: &Task,
    limits: SearchLimits,
) -> Result<Option<SearchOutcome>, Error> {
    let Some(model) = recognize_product_navigation(task) else {
        return Ok(None);
    };
    let targets = model
        .goals
        .iter()
        .filter(|goal| !model.initially_visited.contains(*goal))
        .cloned()
        .collect::<Vec<_>>();
    if targets.len() > MAX_PRODUCT_TARGETS {
        return Ok(None);
    }

    let mut order = (0..targets.len()).collect::<Vec<_>>();
    let mut best: Option<(usize, Vec<usize>)> = None;
    visit_orders(
        &model,
        &targets,
        &mut order,
        0,
        0,
        &mut Vec::new(),
        &mut best,
    );
    let Some((_, target_order)) = best else {
        return Ok(Some(SearchOutcome::Unsolvable { explored: 1 }));
    };

    let mut current = model.start.clone();
    let mut steps = Vec::new();
    for target_index in target_order {
        let target = &targets[target_index];
        for axis in 0..current.len() {
            let path = coordinate_path(&model.neighbors, &current[axis], &target[axis]);
            let Some(path) = path else {
                return Err(Error::new("product-navigation route reconstruction failed"));
            };
            for next in path.into_iter().skip(1) {
                let action = model
                    .actions
                    .iter()
                    .filter(|action| action.axis == axis)
                    .filter_map(|action| {
                        let ground =
                            instantiate_navigation_action(task, action, &current, &next, axis)
                                .ok()?;
                        checked_action_environment(
                            task,
                            &task.actions[action.schema_index],
                            &ground,
                        )
                        .ok()?;
                        Some(ground)
                    })
                    .min();
                let Some(action) = action else {
                    return Ok(None);
                };
                current[axis] = next;
                steps.push(action);
            }
        }
    }

    let distinct_actions = steps.iter().collect::<BTreeSet<_>>().len();
    if distinct_actions > limits.max_ground_actions {
        return Err(Error::grounding_limit(format!(
            "ground action limit ({}) exceeded",
            limits.max_ground_actions
        )));
    }
    let explored = steps.len().saturating_add(1);
    if explored > limits.max_states {
        return Ok(Some(SearchOutcome::LimitReached {
            explored: limits.max_states,
        }));
    }
    let final_state = replay(task, &steps)?;
    if !eval_with_bindings(task, &final_state, &task.goal, &mut HashMap::new())? {
        return Err(Error::new(
            "product-navigation plan replay did not satisfy the goal",
        ));
    }
    Ok(Some(SearchOutcome::Solved(Plan {
        steps,
        final_state,
        explored,
    })))
}

fn visit_orders(
    model: &ProductNavigation,
    targets: &[Vec<String>],
    permutation: &mut [usize],
    index: usize,
    cost: usize,
    route: &mut Vec<usize>,
    best: &mut Option<(usize, Vec<usize>)>,
) {
    if index == permutation.len() {
        let candidate = (cost, route.clone());
        if best.as_ref().is_none_or(|old| candidate < *old) {
            *best = Some(candidate);
        }
        return;
    }
    for next in index..permutation.len() {
        permutation.swap(index, next);
        let from = route
            .last()
            .map(|last| &targets[*last])
            .unwrap_or(&model.start);
        let target_index = permutation[index];
        if let Some(distance) = product_distance(from, &targets[target_index], &model.neighbors) {
            route.push(target_index);
            visit_orders(
                model,
                targets,
                permutation,
                index + 1,
                cost.saturating_add(distance),
                route,
                best,
            );
            route.pop();
        }
        permutation.swap(index, next);
    }
}

fn product_distance(
    from: &[String],
    to: &[String],
    neighbors: &[(String, String)],
) -> Option<usize> {
    from.iter().zip(to).try_fold(0usize, |sum, (from, to)| {
        coordinate_distance(neighbors, from, to).map(|distance| sum + distance)
    })
}

fn coordinate_distance(neighbors: &[(String, String)], from: &str, to: &str) -> Option<usize> {
    if from == to {
        return Some(0);
    }
    let mut distances = BTreeMap::from([(from.to_owned(), 0usize)]);
    let mut queue = std::collections::VecDeque::from([from.to_owned()]);
    while let Some(current) = queue.pop_front() {
        let next_distance = distances[&current] + 1;
        for (_, target) in neighbors.iter().filter(|(source, _)| source == &current) {
            if target == to {
                return Some(next_distance);
            }
            if !distances.contains_key(target) {
                distances.insert(target.clone(), next_distance);
                queue.push_back(target.clone());
            }
        }
    }
    None
}

fn coordinate_path(neighbors: &[(String, String)], from: &str, to: &str) -> Option<Vec<String>> {
    let mut parents = BTreeMap::from([(from.to_owned(), None::<String>)]);
    let mut queue = std::collections::VecDeque::from([from.to_owned()]);
    while let Some(current) = queue.pop_front() {
        if current == to {
            let mut path = vec![to.to_owned()];
            let mut node = to.to_owned();
            while let Some(Some(parent)) = parents.get(&node) {
                path.push(parent.clone());
                node = parent.clone();
            }
            path.reverse();
            return Some(path);
        }
        for (_, target) in neighbors.iter().filter(|(source, _)| source == &current) {
            if !parents.contains_key(target) {
                parents.insert(target.clone(), Some(current.clone()));
                queue.push_back(target.clone());
            }
        }
    }
    None
}

fn recognize_product_navigation(task: &Task) -> Option<ProductNavigation> {
    let goals = positive_ground_goal(&task.goal)?;
    let mut location_role: Option<String> = None;
    let mut visited_role: Option<String> = None;
    let mut neighbor_role: Option<String> = None;
    let mut actions = Vec::new();
    let mut dimensions = None;

    for (schema_index, action) in task.actions.iter().enumerate() {
        let preconditions = match &action.precondition {
            Formula::And(parts) => parts.as_slice(),
            Formula::Atom(_) => std::slice::from_ref(&action.precondition),
            _ => return None,
        };
        let atoms = preconditions
            .iter()
            .map(|formula| match formula {
                Formula::Atom(atom) => Some(atom),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        if atoms.len() != 2 || action.add.len() != 2 || action.delete.len() != 1 {
            return None;
        }
        let deleted = &action.delete[0];
        let location_pre = atoms
            .iter()
            .find(|atom| atom.predicate == deleted.predicate && atom.terms == deleted.terms)?;
        let location_adds = action
            .add
            .iter()
            .filter(|atom| atom.predicate == deleted.predicate)
            .collect::<Vec<_>>();
        if location_adds.len() != 1 || location_pre.terms.is_empty() {
            return None;
        }
        let location_add = location_adds[0];
        let size = location_pre.terms.len();
        if location_pre
            .terms
            .iter()
            .chain(&location_add.terms)
            .any(|term| !matches!(term, Term::Variable(_)))
            || dimensions.is_some_and(|old| old != size)
        {
            return None;
        }
        let old_variables = location_pre
            .terms
            .iter()
            .filter_map(|term| match term {
                Term::Variable(name) => Some(name),
                Term::Constant(_) => None,
            })
            .collect::<BTreeSet<_>>();
        if old_variables.len() != size {
            return None;
        }
        dimensions = Some(size);
        let visited_adds = action
            .add
            .iter()
            .filter(|atom| atom.predicate != deleted.predicate)
            .collect::<Vec<_>>();
        if visited_adds.len() != 1 || visited_adds[0].terms != location_add.terms {
            return None;
        }
        if location_role
            .as_ref()
            .is_some_and(|role| role != &deleted.predicate)
            || visited_role
                .as_ref()
                .is_some_and(|role| role != &visited_adds[0].predicate)
        {
            return None;
        }
        location_role = Some(deleted.predicate.clone());
        visited_role = Some(visited_adds[0].predicate.clone());
        let neighbor = atoms
            .iter()
            .find(|atom| atom.predicate != deleted.predicate)?;
        if neighbor.terms.len() != 2
            || neighbor_role
                .as_ref()
                .is_some_and(|role| role != &neighbor.predicate)
        {
            return None;
        }
        neighbor_role = Some(neighbor.predicate.clone());

        let axis = (0..size).find(|&axis| {
            location_pre
                .terms
                .iter()
                .zip(&location_add.terms)
                .enumerate()
                .all(|(index, (old, new))| {
                    if index == axis {
                        old != new
                    } else {
                        old == new
                    }
                })
        })?;
        let (from, to) = match (&neighbor.terms[0], &neighbor.terms[1]) {
            (Term::Variable(from), Term::Variable(to))
                if location_pre.terms[axis] == Term::Variable(from.clone())
                    && location_add.terms[axis] == Term::Variable(to.clone()) =>
            {
                (from.clone(), to.clone())
            }
            _ => return None,
        };
        let mut used = BTreeSet::new();
        collect_variables(location_pre, &mut used);
        collect_variables(location_add, &mut used);
        collect_variables(neighbor, &mut used);
        if action
            .parameters
            .iter()
            .any(|parameter| !used.contains(&parameter.name))
            || used.len() != action.parameters.len()
        {
            return None;
        }
        if from == to {
            return None;
        }
        actions.push(NavigationAction { schema_index, axis });
    }

    let location_predicate = location_role?;
    let visited_predicate = visited_role?;
    let neighbor_predicate = neighbor_role?;
    if location_predicate == visited_predicate
        || location_predicate == neighbor_predicate
        || visited_predicate == neighbor_predicate
    {
        return None;
    }
    let dimensions = dimensions?;
    if goals
        .iter()
        .any(|goal| goal.predicate != visited_predicate || goal.arguments.len() != dimensions)
    {
        return None;
    }
    let location_facts = task
        .initial
        .iter()
        .filter(|fact| fact.predicate == location_predicate)
        .collect::<Vec<_>>();
    if location_facts.len() != 1 || location_facts[0].arguments.len() != dimensions {
        return None;
    }
    let start = location_facts[0].arguments.clone();
    let initially_visited: BTreeSet<Vec<String>> = task
        .initial
        .iter()
        .filter(|fact| fact.predicate == visited_predicate)
        .map(|fact| fact.arguments.clone())
        .collect();
    if initially_visited
        .iter()
        .any(|tuple| tuple.len() != dimensions)
    {
        return None;
    }
    let neighbors: Vec<(String, String)> = task
        .initial
        .iter()
        .filter(|fact| fact.predicate == neighbor_predicate && fact.arguments.len() == 2)
        .map(|fact| (fact.arguments[0].clone(), fact.arguments[1].clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if neighbors.is_empty() {
        return None;
    }
    if neighbors.iter().any(|(from, to)| from == to) {
        return None;
    }
    if (0..dimensions).any(|axis| !actions.iter().any(|action| action.axis == axis)) {
        return None;
    }
    let location_types = task
        .predicates
        .iter()
        .find(|predicate| predicate.name == location_predicate)?
        .parameters
        .clone();
    let visited_types = task
        .predicates
        .iter()
        .find(|predicate| predicate.name == visited_predicate)?
        .parameters
        .clone();
    let neighbor_types = task
        .predicates
        .iter()
        .find(|predicate| predicate.name == neighbor_predicate)?
        .parameters
        .clone();
    if location_types.len() != dimensions
        || visited_types != location_types
        || neighbor_types.len() != 2
        || (0..dimensions).any(|axis| {
            location_types[axis] != neighbor_types[0] || location_types[axis] != neighbor_types[1]
        })
    {
        return None;
    }
    if goals
        .iter()
        .any(|goal| goal.arguments == start && !initially_visited.contains(&start))
    {
        return None;
    }
    Some(ProductNavigation {
        start,
        goals: goals.into_iter().map(|goal| goal.arguments).collect(),
        initially_visited,
        neighbors,
        actions,
    })
}

fn collect_variables(atom: &Atom, names: &mut BTreeSet<String>) {
    names.extend(atom.terms.iter().filter_map(|term| match term {
        Term::Variable(name) => Some(name.clone()),
        Term::Constant(_) => None,
    }));
}

fn instantiate_navigation_action(
    task: &Task,
    action: &NavigationAction,
    current: &[String],
    next: &str,
    axis: usize,
) -> Result<GroundAction, Error> {
    let schema = &task.actions[action.schema_index];
    let mut binding = HashMap::<String, String>::new();
    let old_location = &schema.delete[0];
    let new_location = schema
        .add
        .iter()
        .find(|atom| atom.predicate == old_location.predicate)
        .ok_or_else(|| Error::new("missing product-navigation location effect"))?;
    let neighbor = match &schema.precondition {
        Formula::Atom(atom) => vec![atom],
        Formula::And(parts) => parts
            .iter()
            .filter_map(|part| match part {
                Formula::Atom(atom) => Some(atom),
                _ => None,
            })
            .collect(),
        _ => return Err(Error::new("invalid product-navigation precondition")),
    }
    .into_iter()
    .find(|atom| atom.predicate != old_location.predicate)
    .ok_or_else(|| Error::new("missing product-navigation neighbor condition"))?;
    let mut bind_atom = |atom: &Atom, values: &[String]| -> Result<(), Error> {
        if atom.terms.len() != values.len() {
            return Err(Error::new("product-navigation tuple arity mismatch"));
        }
        for (term, value) in atom.terms.iter().zip(values) {
            if let Term::Variable(name) = term {
                if binding.get(name).is_some_and(|old| old != value) {
                    return Err(Error::new("inconsistent product-navigation binding"));
                }
                binding.insert(name.clone(), value.clone());
            }
        }
        Ok(())
    };
    bind_atom(old_location, current)?;
    let mut destination = current.to_vec();
    destination[axis] = next.to_owned();
    bind_atom(new_location, &destination)?;
    bind_atom(neighbor, &[current[axis].clone(), next.to_owned()])?;
    for parameter in &schema.parameters {
        if !binding.contains_key(&parameter.name) {
            return Err(Error::new("unbound product-navigation parameter"));
        }
    }
    Ok(GroundAction {
        name: schema.name.clone(),
        arguments: schema
            .parameters
            .iter()
            .map(|parameter| binding[&parameter.name].clone())
            .collect(),
    })
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

    fn product_visit_task(goal_tuples: &[(&str, &str)], directed: bool) -> Task {
        let positions = ["p0", "p1", "p2"];
        let mut actions = Vec::new();
        for axis in 0..2 {
            let from_names = ["x0_from", "x1_from"];
            let to_name = if axis == 0 { "x0_to" } else { "x1_to" };
            let old_terms = from_names
                .iter()
                .map(|name| Term::Variable((*name).into()))
                .collect::<Vec<_>>();
            let mut new_terms = old_terms.clone();
            new_terms[axis] = Term::Variable(to_name.into());
            let neighbor = Atom {
                predicate: "neighbor".into(),
                terms: vec![
                    Term::Variable(from_names[axis].into()),
                    Term::Variable(to_name.into()),
                ],
            };
            actions.push(Action {
                name: format!("move-{axis}"),
                parameters: from_names
                    .iter()
                    .map(|name| Binding {
                        name: (*name).into(),
                        ty: "pos".into(),
                    })
                    .chain(std::iter::once(Binding {
                        name: to_name.into(),
                        ty: "pos".into(),
                    }))
                    .collect(),
                precondition: Formula::And(vec![
                    Formula::Atom(Atom {
                        predicate: "at".into(),
                        terms: old_terms.clone(),
                    }),
                    Formula::Atom(neighbor),
                ]),
                add: vec![
                    Atom {
                        predicate: "at".into(),
                        terms: new_terms.clone(),
                    },
                    Atom {
                        predicate: "visited".into(),
                        terms: new_terms,
                    },
                ],
                delete: vec![Atom {
                    predicate: "at".into(),
                    terms: old_terms,
                }],
            });
        }
        let mut initial = BTreeSet::from([
            GroundAtom {
                predicate: "at".into(),
                arguments: vec!["p0".into(), "p0".into()],
            },
            GroundAtom {
                predicate: "visited".into(),
                arguments: vec!["p0".into(), "p0".into()],
            },
        ]);
        for (left, right) in positions.iter().zip(positions.iter().skip(1)) {
            initial.insert(GroundAtom {
                predicate: "neighbor".into(),
                arguments: vec![(*left).into(), (*right).into()],
            });
            if !directed {
                initial.insert(GroundAtom {
                    predicate: "neighbor".into(),
                    arguments: vec![(*right).into(), (*left).into()],
                });
            }
        }
        Task {
            name: "product-visit".into(),
            types: vec!["pos".into()],
            objects: positions
                .iter()
                .map(|name| Binding {
                    name: (*name).into(),
                    ty: "pos".into(),
                })
                .collect(),
            predicates: vec![
                Predicate {
                    name: "at".into(),
                    parameters: vec!["pos".into(); 2],
                },
                Predicate {
                    name: "visited".into(),
                    parameters: vec!["pos".into(); 2],
                },
                Predicate {
                    name: "neighbor".into(),
                    parameters: vec!["pos".into(); 2],
                },
            ],
            actions,
            initial,
            goal: Formula::And(
                goal_tuples
                    .iter()
                    .map(|(x, y)| {
                        Formula::Atom(Atom {
                            predicate: "visited".into(),
                            terms: vec![Term::Constant((*x).into()), Term::Constant((*y).into())],
                        })
                    })
                    .collect(),
            ),
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
    fn product_navigation_matches_bfs_for_one_to_three_targets() {
        let targets = [
            vec![("p2", "p0")],
            vec![("p2", "p0"), ("p0", "p2")],
            vec![("p2", "p0"), ("p0", "p2"), ("p2", "p2")],
        ];
        for goals in targets {
            let task = product_visit_task(&goals, false);
            assert!(recognize_product_navigation(&task).is_some());
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
            let SearchOutcome::Solved(astar) = astar else {
                panic!("expected optimized A* plan");
            };
            let SearchOutcome::Solved(bfs) = bfs else {
                panic!("expected BFS plan");
            };
            assert_eq!(astar.steps.len(), bfs.steps.len());
            assert_eq!(astar.explored, astar.steps.len() + 1);
            assert_eq!(replay(&task, &astar.steps).unwrap(), astar.final_state);
        }
    }

    #[test]
    fn product_navigation_falls_back_above_three_remaining_targets() {
        let task = product_visit_task(
            &[("p2", "p0"), ("p0", "p2"), ("p2", "p2"), ("p1", "p1")],
            false,
        );
        assert!(recognize_product_navigation(&task).is_some());
        assert!(
            solve_product_navigation(&task, SearchLimits::default())
                .unwrap()
                .is_none()
        );
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
        assert_eq!(
            match astar {
                SearchOutcome::Solved(plan) => plan.steps.len(),
                other => panic!("expected generic A* plan, got {other:?}"),
            },
            match bfs {
                SearchOutcome::Solved(plan) => plan.steps.len(),
                other => panic!("expected BFS plan, got {other:?}"),
            }
        );
    }

    #[test]
    fn product_navigation_respects_directed_unreachability() {
        let mut task = product_visit_task(&[("p2", "p0")], true);
        task.initial.remove(&GroundAtom {
            predicate: "neighbor".into(),
            arguments: vec!["p1".into(), "p2".into()],
        });
        let SearchOutcome::Unsolvable { explored } = solve_with_algorithm(
            &task,
            SearchLimits::default(),
            crate::model::SearchAlgorithm::AStar,
        )
        .unwrap() else {
            panic!("directed unreachable target should be unsolvable");
        };
        assert_eq!(explored, 1);
    }

    #[test]
    fn product_navigation_falls_back_when_action_has_extra_precondition() {
        let mut task = product_visit_task(&[("p1", "p0")], false);
        task.predicates.push(Predicate {
            name: "permit".into(),
            parameters: vec![],
        });
        task.initial.insert(GroundAtom {
            predicate: "permit".into(),
            arguments: vec![],
        });
        task.actions[0].precondition = Formula::And(vec![
            task.actions[0].precondition.clone(),
            Formula::Atom(Atom {
                predicate: "permit".into(),
                terms: vec![],
            }),
        ]);
        assert!(recognize_product_navigation(&task).is_none());
        let SearchOutcome::Solved(plan) = solve_with_algorithm(
            &task,
            SearchLimits::default(),
            crate::model::SearchAlgorithm::AStar,
        )
        .unwrap() else {
            panic!("generic A* should solve the structurally unsupported task");
        };
        assert_eq!(plan.steps.len(), 1);
    }

    #[test]
    fn product_navigation_enforces_trajectory_and_action_limits() {
        let task = product_visit_task(&[("p1", "p0")], false);
        assert_eq!(
            solve_with_algorithm(
                &task,
                SearchLimits {
                    max_states: 1,
                    max_ground_actions: 10,
                },
                crate::model::SearchAlgorithm::AStar,
            )
            .unwrap(),
            SearchOutcome::LimitReached { explored: 1 }
        );
        let error = solve_with_algorithm(
            &task,
            SearchLimits {
                max_states: 10,
                max_ground_actions: 0,
            },
            crate::model::SearchAlgorithm::AStar,
        )
        .unwrap_err();
        assert_eq!(error.kind(), crate::model::ErrorKind::GroundingLimit);
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
