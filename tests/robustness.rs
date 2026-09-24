use pddl_fol::{Binding, Formula, SearchLimits, SearchOutcome, parse_dsl, solve, validate};

fn empty_task() -> pddl_fol::Task {
    parse_dsl("problem empty { types; objects {} predicates {} init: true; goal: false; }").unwrap()
}

#[test]
fn wide_binder_lists_are_rejected_before_recursive_evaluation() {
    let mut task = empty_task();
    let bindings = (0..300)
        .map(|n| Binding {
            name: format!("x{n}"),
            ty: "object".into(),
        })
        .collect();
    task.goal = Formula::Forall(bindings, Box::new(Formula::And(vec![])));
    assert!(validate(&task).unwrap_err().message.contains("limit"));
}

#[test]
fn pathological_formula_depth_is_a_parser_error() {
    let source = format!(
        "problem deep {{ types; objects {{}} predicates {{}} init: true; goal: {}true; }}",
        "!".repeat(400)
    );
    assert!(parse_dsl(&source).unwrap_err().message.contains("depth"));
}

#[test]
fn empty_parameter_type_prevents_dead_cartesian_products() {
    let mut task = empty_task();
    task.types.push("empty".into());
    task.objects.push(Binding {
        name: "a".into(),
        ty: "object".into(),
    });
    task.actions.push(pddl_fol::Action {
        name: "unavailable".into(),
        parameters: vec![Binding {
            name: "x".into(),
            ty: "empty".into(),
        }],
        precondition: Formula::And(vec![]),
        add: vec![],
        delete: vec![],
    });
    assert!(matches!(
        solve(&task, SearchLimits::default()).unwrap(),
        SearchOutcome::Unsolvable { .. }
    ));
}

#[test]
fn visited_states_terminate_a_cycle_without_a_plan() {
    let task = parse_dsl(
        "problem cycle {
        types place;
        objects { a, b, unreachable: place; }
        predicates { At(place); Road(place,place); }
        init: At(a) & Road(a,b) & Road(b,a);
        goal: At(unreachable);
        action Move(from:place,to:place) {
            pre: At(from) & Road(from,to);
            effect: !At(from) & At(to);
        }
    }",
    )
    .unwrap();
    assert!(matches!(
        solve(&task, SearchLimits::default()).unwrap(),
        SearchOutcome::Unsolvable { explored: 2 }
    ));
}
