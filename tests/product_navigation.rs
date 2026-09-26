use pddl_fol::{
    ErrorKind, SearchAlgorithm, SearchLimits, SearchOutcome, evaluate, parse_pddl, replay,
    solve_with_algorithm,
};

fn navigation_domain(dimensions: usize, renamed: bool, reverse_parameters: bool) -> String {
    let location = if renamed { "whereabouts" } else { "at" };
    let neighbor = if renamed { "arc" } else { "neighbor" };
    let visited = if renamed { "marked" } else { "visited" };
    let variables = (0..dimensions)
        .map(|axis| format!("?x{axis}"))
        .collect::<Vec<_>>();
    let mut predicates = format!(
        "({location} {}) ({visited} {}) ({neighbor} ?from - place ?to - place)",
        variables
            .iter()
            .map(|var| format!("{var} - place"))
            .collect::<Vec<_>>()
            .join(" "),
        variables
            .iter()
            .map(|var| format!("{var} - place"))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let mut actions = String::new();
    for axis in 0..dimensions {
        let next = "?next";
        let mut parameters = variables.clone();
        parameters.push(next.into());
        if reverse_parameters {
            parameters.reverse();
        }
        let old_tuple = variables.join(" ");
        let mut new_tuple = variables.clone();
        new_tuple[axis] = next.into();
        let new_tuple = new_tuple.join(" ");
        let from = &variables[axis];
        let action_name = if renamed {
            format!("traverse_{axis}")
        } else {
            format!("move_{axis}")
        };
        actions.push_str(&format!(
            "(:action {action_name}\n  :parameters ({})\n  :precondition (and ({location} {old_tuple}) ({neighbor} {from} {next}))\n  :effect (and (not ({location} {old_tuple})) ({location} {new_tuple}) ({visited} {new_tuple})))\n",
            parameters
                .iter()
                .map(|parameter| format!("{parameter} - place"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    predicates.push(')');
    format!(
        "(define (domain navigation)\n (:requirements :strips :typing)\n (:types place - object)\n (:predicates {predicates}\n {actions})\n"
    )
}

fn navigation_problem(
    dimensions: usize,
    places: &[&str],
    edges: &[(&str, &str)],
    start: &[&str],
    initially_visited: &[Vec<&str>],
    goals: &[Vec<&str>],
    renamed: bool,
) -> String {
    let location = if renamed { "whereabouts" } else { "at" };
    let neighbor = if renamed { "arc" } else { "neighbor" };
    let visited = if renamed { "marked" } else { "visited" };
    let mut facts = vec![format!("({location} {})", start.join(" "))];
    facts.extend(
        initially_visited
            .iter()
            .map(|tuple| format!("({visited} {})", tuple.join(" "))),
    );
    facts.extend(
        edges
            .iter()
            .map(|(from, to)| format!("({neighbor} {from} {to})")),
    );
    let goal_atoms = goals
        .iter()
        .map(|tuple| format!("({visited} {})", tuple.join(" ")))
        .collect::<Vec<_>>();
    let goal = match goal_atoms.as_slice() {
        [only] => only.clone(),
        _ => format!("(and {})", goal_atoms.join(" ")),
    };
    assert_eq!(start.len(), dimensions);
    assert!(goals.iter().all(|tuple| tuple.len() == dimensions));
    format!(
        "(define (problem navigation-instance)\n (:domain navigation)\n (:objects {} - place)\n (:init {})\n (:goal {goal}))\n",
        places.join(" "),
        facts.join(" "),
    )
}

fn task(
    dimensions: usize,
    places: &[&str],
    edges: &[(&str, &str)],
    start: &[&str],
    initially_visited: &[Vec<&str>],
    goals: &[Vec<&str>],
) -> pddl_fol::Task {
    let domain = navigation_domain(dimensions, false, false);
    let problem = navigation_problem(
        dimensions,
        places,
        edges,
        start,
        initially_visited,
        goals,
        false,
    );
    parse_pddl(&domain, &problem).unwrap()
}

fn plan(task: &pddl_fol::Task, algorithm: SearchAlgorithm, limits: SearchLimits) -> pddl_fol::Plan {
    let SearchOutcome::Solved(plan) = solve_with_algorithm(task, limits, algorithm).unwrap() else {
        panic!("expected a plan from {algorithm:?}");
    };
    assert_eq!(replay(task, &plan.steps).unwrap(), plan.final_state);
    assert!(evaluate(task, &plan.final_state, &task.goal).unwrap());
    plan
}

#[test]
fn directed_one_dimensional_three_goal_route_matches_bfs() {
    let places = ["s", "a", "b", "c"];
    let edges = [("s", "a"), ("a", "b"), ("b", "c"), ("c", "s")];
    let start = ["s"];
    let initial_visits = [vec!["s"]];
    for (goals, expected_length) in [
        (vec![vec!["b"]], 2),
        (vec![vec!["b"], vec!["c"]], 3),
        (vec![vec!["a"], vec!["b"], vec!["c"]], 3),
    ] {
        let task = task(1, &places, &edges, &start, &initial_visits, &goals);
        let astar = plan(&task, SearchAlgorithm::AStar, SearchLimits::default());
        let bfs = plan(&task, SearchAlgorithm::Bfs, SearchLimits::default());
        assert_eq!(astar.steps.len(), expected_length);
        assert_eq!(astar.steps.len(), bfs.steps.len());
    }
}

#[test]
fn two_dimensional_product_optimizer_solves_at_the_exact_state_limit() {
    let places = ["a", "b"];
    let edges = [("a", "b"), ("b", "a")];
    let start = ["a", "a"];
    let initial_visits = [vec!["a", "a"]];
    let goals = [vec!["b", "b"]];
    let task = task(2, &places, &edges, &start, &initial_visits, &goals);
    let limits = SearchLimits {
        max_states: 3,
        max_ground_actions: 2,
    };

    let astar = plan(&task, SearchAlgorithm::AStar, limits);
    assert_eq!(astar.steps.len(), 2);
    assert_eq!(astar.explored, 3);
    let bfs = plan(
        &task,
        SearchAlgorithm::Bfs,
        SearchLimits {
            max_states: 100,
            max_ground_actions: 100,
        },
    );
    assert_eq!(astar.steps.len(), bfs.steps.len());
    assert!(matches!(
        solve_with_algorithm(
            &task,
            SearchLimits {
                max_ground_actions: 100,
                ..limits
            },
            SearchAlgorithm::Bfs,
        )
        .unwrap(),
        SearchOutcome::LimitReached { .. }
    ));
    assert!(matches!(
        solve_with_algorithm(
            &task,
            SearchLimits {
                max_states: 2,
                ..limits
            },
            SearchAlgorithm::AStar,
        )
        .unwrap(),
        SearchOutcome::LimitReached { explored: 2 }
    ));
    assert_eq!(
        solve_with_algorithm(
            &task,
            SearchLimits {
                max_ground_actions: 1,
                ..limits
            },
            SearchAlgorithm::AStar,
        )
        .unwrap_err()
        .kind(),
        ErrorKind::GroundingLimit
    );
}

#[test]
fn renamed_predicates_and_reordered_parameters_are_recognized() {
    let domain = navigation_domain(2, true, true);
    let problem = navigation_problem(
        2,
        &["a", "b"],
        &[("a", "b"), ("b", "a")],
        &["a", "a"],
        &[vec!["a", "a"]],
        &[vec!["b", "a"]],
        true,
    );
    let task = parse_pddl(&domain, &problem).unwrap();
    let result = plan(
        &task,
        SearchAlgorithm::AStar,
        SearchLimits {
            max_states: 2,
            max_ground_actions: 1,
        },
    );
    assert_eq!(result.steps.len(), 1);
    assert_eq!(result.steps[0].arguments, ["b", "a", "a"]);
    assert!(matches!(
        solve_with_algorithm(&task, SearchLimits::default(), SearchAlgorithm::Bfs).unwrap(),
        SearchOutcome::Solved(_)
    ));
}

#[test]
fn extra_move_condition_and_effect_force_generic_search() {
    let base = navigation_domain(2, false, false);
    let predicates = "(:predicates (at ?x0 - place ?x1 - place) (visited ?x0 - place ?x1 - place) (neighbor ?from - place ?to - place)";
    let domain_with_enabled = base
        .replace(predicates, &format!("{predicates} (enabled)"))
        .replace(
            ":precondition (and (at ?x0 ?x1) (neighbor ?x0 ?next))",
            ":precondition (and (at ?x0 ?x1) (neighbor ?x0 ?next) (enabled))",
        );
    let problem_with_enabled = navigation_problem(
        2,
        &["a", "b"],
        &[("a", "b"), ("b", "a")],
        &["a", "a"],
        &[vec!["a", "a"]],
        &[vec!["b", "b"]],
        false,
    )
    .replace("(:init", "(:init (enabled)");

    let predicates = "(:predicates (at ?x0 - place ?x1 - place) (visited ?x0 - place ?x1 - place) (neighbor ?from - place ?to - place)";
    let domain_with_extra_effect = base
        .replace(predicates, &format!("{predicates} (touched ?x - place)"))
        .replace(
            "(visited ?next ?x1)))",
            "(visited ?next ?x1) (touched ?next)))",
        )
        .replace(
            "(visited ?x0 ?next)))",
            "(visited ?x0 ?next) (touched ?next)))",
        );
    let cases = [
        (domain_with_enabled, problem_with_enabled),
        (
            domain_with_extra_effect,
            navigation_problem(
                2,
                &["a", "b"],
                &[("a", "b"), ("b", "a")],
                &["a", "a"],
                &[vec!["a", "a"]],
                &[vec!["b", "b"]],
                false,
            ),
        ),
    ];
    for (domain, problem) in cases {
        let task = parse_pddl(&domain, &problem).unwrap();
        let tight = SearchLimits {
            max_states: 3,
            max_ground_actions: 100,
        };
        assert!(matches!(
            solve_with_algorithm(&task, tight, SearchAlgorithm::AStar).unwrap(),
            SearchOutcome::LimitReached { .. }
        ));
        assert!(matches!(
            solve_with_algorithm(&task, tight, SearchAlgorithm::Bfs).unwrap(),
            SearchOutcome::LimitReached { .. }
        ));
    }
}

#[test]
fn deleting_visited_facts_forces_generic_unsolvability_result() {
    let domain = navigation_domain(2, false, false).replace(
        ":effect (and (not (at ?x0 ?x1))",
        ":effect (and (not (visited ?x0 ?x1)) (not (at ?x0 ?x1))",
    );
    let problem = navigation_problem(
        2,
        &["a", "b"],
        &[("a", "b"), ("b", "a")],
        &["a", "a"],
        &[vec!["a", "a"]],
        &[vec!["b", "a"], vec!["b", "b"]],
        false,
    );
    let task = parse_pddl(&domain, &problem).unwrap();
    for algorithm in [SearchAlgorithm::AStar, SearchAlgorithm::Bfs] {
        assert!(matches!(
            solve_with_algorithm(&task, SearchLimits::default(), algorithm).unwrap(),
            SearchOutcome::Unsolvable { .. }
        ));
    }
}

#[test]
fn diagonal_move_disqualifies_product_assumption_and_keeps_shortest_plan() {
    let mut domain = navigation_domain(2, false, false);
    let diagonal = "(:action diagonal\n  :parameters (?x0 - place ?x1 - place ?next0 - place ?next1 - place)\n  :precondition (and (at ?x0 ?x1) (neighbor ?x0 ?next0))\n  :effect (and (not (at ?x0 ?x1)) (at ?next0 ?next1) (visited ?next0 ?next1)))\n";
    domain.insert_str(domain.len() - 2, diagonal);
    let problem = navigation_problem(
        2,
        &["a", "b"],
        &[("a", "b"), ("b", "a")],
        &["a", "a"],
        &[vec!["a", "a"]],
        &[vec!["b", "b"]],
        false,
    );
    let task = parse_pddl(&domain, &problem).unwrap();
    let astar = plan(&task, SearchAlgorithm::AStar, SearchLimits::default());
    let bfs = plan(&task, SearchAlgorithm::Bfs, SearchLimits::default());
    assert_eq!(astar.steps.len(), 1);
    assert_eq!(astar.steps.len(), bfs.steps.len());
    assert_eq!(astar.steps[0].name, "diagonal");
}
