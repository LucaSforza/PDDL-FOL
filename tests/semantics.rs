use pddl_fol::{
    ErrorKind, SearchLimits, SearchOutcome, evaluate, parse_dsl, parse_pddl, replay, solve,
    validate,
};

const TRAVEL: &str = r#"
problem travel {
  types place;
  objects { home, office, park: place; }
  predicates { At(place); Road(place, place); }
  init: At(home) and Road(home,park) and Road(park,office) and Road(home,office);
  goal: At(office);
  action Move(from:place,to:place) {
    pre: At(from) and Road(from,to) and from != to;
    effect: not At(from) and At(to);
  }
}
"#;

const DOMAIN: &str = r#"
(define (domain travel)
  (:requirements :strips :typing :negative-preconditions :equality)
  (:types place - object)
  (:predicates (at ?p - place) (road ?from ?to - place))
  (:action move
    :parameters (?from ?to - place)
    :precondition (and (at ?from) (road ?from ?to) (not (= ?from ?to)))
    :effect (and (not (at ?from)) (at ?to))))
"#;

const PROBLEM: &str = r#"
(define (problem trip)
  (:domain travel)
  (:objects home office park - place)
  (:init (at home) (road home park) (road park office) (road home office))
  (:goal (at office)))
"#;

#[test]
fn both_languages_find_the_same_shortest_plan_and_preserve_static_facts() {
    let dsl = parse_dsl(TRAVEL).unwrap();
    let pddl = parse_pddl(DOMAIN, PROBLEM).unwrap();
    let mut results = Vec::new();
    for task in [dsl, pddl] {
        let SearchOutcome::Solved(plan) = solve(&task, SearchLimits::default()).unwrap() else {
            panic!("a direct road must yield a plan");
        };
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].name, "move");
        assert_eq!(plan.steps[0].arguments, ["home", "office"]);
        let final_state = replay(&task, &plan.steps).unwrap();
        assert_eq!(final_state, plan.final_state);
        assert!(evaluate(&task, &final_state, &task.goal).unwrap());
        for fact in task.initial.iter().filter(|a| a.predicate == "road") {
            assert!(final_state.contains(fact), "frame rule must preserve roads");
        }
        assert!(!final_state.iter().any(|a| a.arguments == ["home"]));
        results.push(final_state);
    }
    assert_eq!(results[0], results[1]);
}

fn logic_task(goal: &str) -> String {
    format!(
        "problem logic {{
          types item, empty;
          objects {{ a, b: item; }}
          predicates {{ Marked(item); }}
          init: Marked(a);
          goal: {goal};
        }}"
    )
}

#[test]
fn quantified_logic_uses_finite_models_and_lexical_scope() {
    let cases = [
        ("!Marked(b)", true),
        ("a = b", false),
        ("true", true),
        ("false", false),
        ("forall z:empty . false", true),
        ("exists z:empty . true", false),
        ("exists x:item . Marked(x)", true),
        ("forall x:item . Marked(x)", false),
        ("Marked(b) -> Marked(a)", true),
        ("Marked(a) -> Marked(b)", false),
        (
            "exists x:item . (x = a & (exists x:item . x = b) & x = a)",
            true,
        ),
        ("forall x:item . exists y:item . x != y", true),
    ];
    for (formula, expected) in cases {
        let task = parse_dsl(&logic_task(formula)).unwrap();
        validate(&task).unwrap();
        assert_eq!(
            evaluate(&task, &task.initial, &task.goal).unwrap(),
            expected,
            "formula: {formula}"
        );
    }
}

#[test]
fn ascii_word_and_symbol_operator_spellings_parse_to_the_same_model() {
    let words = parse_dsl(&logic_task(
        "FORALL x:item . (not Marked(x) or (x != a and Marked(a)))",
    ))
    .unwrap();
    let symbols = parse_dsl(&logic_task(
        "forall x:item . (!Marked(x) | (x != a & Marked(a)))",
    ))
    .unwrap();
    assert_eq!(words, symbols);

    let words = parse_dsl(&logic_task(
        "Marked(a) and false or true implies false implies true",
    ))
    .unwrap();
    let symbols = parse_dsl(&logic_task("Marked(a) && false || true -> false -> true")).unwrap();
    assert_eq!(words, symbols);
}

#[test]
fn dsl_rejects_non_ascii_with_source_location_even_in_comments() {
    assert_eq!(
        parse_dsl("∀").unwrap_err().to_string(),
        "1:1: non-ASCII character '∀'"
    );
    assert_eq!(
        parse_dsl("// ok\n// ∧").unwrap_err().to_string(),
        "2:4: non-ASCII character '∧'"
    );
}

#[test]
fn pddl_rejects_non_ascii_in_comments() {
    assert_eq!(
        parse_pddl("; ∧", PROBLEM).unwrap_err().to_string(),
        "1:3: non-ASCII character '∧'"
    );
}

#[test]
fn formula_precedence_is_unary_then_and_then_or_then_right_associative_imply() {
    for (formula, expected) in [
        ("Marked(a) | Marked(b) & false", true),
        ("(Marked(a) | Marked(b)) & false", false),
        ("false -> true -> false", true),
        ("(true -> false) -> false", true),
    ] {
        let task = parse_dsl(&logic_task(formula)).unwrap();
        assert_eq!(
            evaluate(&task, &task.initial, &task.goal).unwrap(),
            expected,
            "{formula}"
        );
    }
}

#[test]
fn quantifier_scope_reaches_the_delimiter_and_parentheses_close_it() {
    let scoped = parse_dsl(&logic_task("exists x:item . Marked(x) & x = a")).unwrap();
    let pddl_fol::Formula::Exists(_, body) = &scoped.goal else {
        panic!("the quantifier should cover the rest of the formula");
    };
    assert!(matches!(body.as_ref(), pddl_fol::Formula::And(_)));
    assert!(evaluate(&scoped, &scoped.initial, &scoped.goal).unwrap());

    let grouped = parse_dsl(&logic_task("(exists x:item . Marked(x)) & true")).unwrap();
    let pddl_fol::Formula::And(parts) = &grouped.goal else {
        panic!("parentheses should let the quantifier end before the conjunction");
    };
    assert!(matches!(parts[0], pddl_fol::Formula::Exists(_, _)));
}

#[test]
fn empty_sections_and_zero_arity_predicates_are_supported() {
    let task = parse_dsl(
        "problem ready {
          // Empty type and object sections are valid.
          types;
          objects {}
          predicates { Ready(); }
          init: Ready();
          goal: Ready();
        }",
    )
    .unwrap();
    assert_eq!(task.predicates[0].parameters.len(), 0);
    assert!(evaluate(&task, &task.initial, &task.goal).unwrap());

    let empty =
        parse_dsl("problem empty { types; objects {} predicates {} init: true; goal: true; }")
            .unwrap();
    assert!(empty.initial.is_empty());
    assert!(evaluate(&empty, &empty.initial, &empty.goal).unwrap());
}

#[test]
fn search_distinguishes_empty_plan_unreachable_and_limits() {
    let satisfied = parse_dsl(&TRAVEL.replace("goal: At(office);", "goal: At(home);")).unwrap();
    let SearchOutcome::Solved(plan) = solve(&satisfied, SearchLimits::default()).unwrap() else {
        panic!("initial goal must have an empty plan");
    };
    assert!(plan.steps.is_empty());
    assert_eq!(plan.situation(), "S0");
    assert!(matches!(
        solve(
            &satisfied,
            SearchLimits {
                max_ground_actions: 1,
                ..SearchLimits::default()
            }
        )
        .unwrap(),
        SearchOutcome::Solved(_)
    ));

    let impossible = parse_dsl(&TRAVEL.replace("goal: At(office);", "goal: false;")).unwrap();
    assert!(matches!(
        solve(&impossible, SearchLimits::default()).unwrap(),
        SearchOutcome::Unsolvable { .. }
    ));
    assert!(matches!(
        solve(
            &impossible,
            SearchLimits {
                max_states: 1,
                ..SearchLimits::default()
            }
        )
        .unwrap(),
        SearchOutcome::LimitReached { .. }
    ));
    let error = solve(
        &impossible,
        SearchLimits {
            max_ground_actions: 1,
            ..SearchLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::GroundingLimit);
}

#[test]
fn quantified_blocks_produce_the_lecture_plan() {
    let task = parse_dsl(include_str!("../examples/blocks-quantified.fol")).unwrap();
    let SearchOutcome::Solved(plan) = solve(&task, SearchLimits::default()).unwrap() else {
        panic!("blocks must be rearrangeable");
    };
    assert_eq!(plan.steps.len(), 3);
    let moves: Vec<_> = plan.steps.iter().map(ToString::to_string).collect();
    assert_eq!(
        moves,
        [
            "move(c, a, table)",
            "move(b, table, c)",
            "move(a, table, b)"
        ]
    );
    assert!(evaluate(&task, &replay(&task, &plan.steps).unwrap(), &task.goal).unwrap());
}

#[test]
fn malformed_models_are_rejected_before_search() {
    let cases = [
        TRAVEL.replace("types place;", "types place, place;"),
        TRAVEL.replace("home, office, park: place;", "home, office, park: missing;"),
        TRAVEL.replace("home, office, park: place;", "home, home, park: place;"),
        TRAVEL.replace("goal: At(office);", "goal: At(missing);"),
        TRAVEL.replace("goal: At(office);", "goal: Unknown(office);"),
        TRAVEL.replace("goal: At(office);", "goal: At(home,office);"),
        TRAVEL.replace("goal: At(office);", "goal: At(free);"),
        TRAVEL.replace(
            "effect: not At(from) and At(to);",
            "effect: not At(from) and At(free);",
        ),
        TRAVEL.replace("init: At(home)", "init: At(free)"),
        TRAVEL.replace("from:place,to:place", "from:place,from:place"),
        TRAVEL
            .replace("types place;", "types place, other;")
            .replace("office, park: place;", "office: other; park: place;"),
    ];
    for source in cases {
        let rejected = match parse_dsl(&source) {
            Err(_) => true,
            Ok(task) => solve(&task, SearchLimits::default()).is_err(),
        };
        assert!(rejected, "invalid model accepted: {source}");
    }
}

#[test]
fn parser_rejects_unsupported_or_ambiguous_input() {
    for source in [
        format!("{TRAVEL} trailing"),
        TRAVEL.replace("goal: At(office);", "goal: At(office); goal: At(home);"),
        TRAVEL.replace("types place;", "types place; mystery;"),
        TRAVEL.replace(
            "effect: not At(from) and At(to);",
            "effect: At(from) | At(to);",
        ),
        TRAVEL.replace(
            "pre: At(from) and Road(from,to) and from != to;",
            "pre: At(from); pre: Road(from,to);",
        ),
        TRAVEL.replace("pre: At(from) and Road(from,to) and from != to;", ""),
        TRAVEL.replace("effect: not At(from) and At(to);", ""),
        TRAVEL.replace(
            "effect: not At(from) and At(to);",
            "effect: At(from) | At(to);",
        ),
    ] {
        assert!(parse_dsl(&source).is_err(), "accepted: {source}");
    }
    for domain in [
        DOMAIN.replace(":strips", ":adl"),
        DOMAIN.replace("(at ?to))))", "(when (at ?from) (at ?to)))))"),
        DOMAIN.replace("place - object", "place - location location - object"),
        DOMAIN.replace("(:types place - object)", "(:functions (fuel))"),
    ] {
        assert!(parse_pddl(&domain, PROBLEM).is_err(), "accepted: {domain}");
    }
    assert!(
        parse_pddl(
            DOMAIN,
            &PROBLEM.replace("(:domain travel)", "(:domain other)")
        )
        .is_err()
    );
}

#[test]
fn pddl_constants_untyped_objects_and_case_insensitivity_work() {
    let domain = "(DEFINE (DOMAIN D)
      (:CONSTANTS A)
      (:PREDICATES (P ?X))
      (:ACTION MARK :PARAMETERS (?X)
        :PRECONDITION (NOT (P ?X)) :EFFECT (P ?X)))";
    let problem = "(define (problem p) (:domain d) (:objects b)
      (:init) (:goal (forall (?x) (p ?x))))";
    let task = parse_pddl(domain, problem).unwrap();
    let SearchOutcome::Solved(plan) = solve(&task, SearchLimits::default()).unwrap() else {
        panic!("both constant and problem object can be marked");
    };
    assert_eq!(plan.steps.len(), 2);
    assert!(evaluate(&task, &plan.final_state, &task.goal).unwrap());
}

#[test]
fn replay_rejects_inapplicable_and_unknown_actions() {
    let task = parse_dsl(TRAVEL).unwrap();
    let SearchOutcome::Solved(mut plan) = solve(&task, SearchLimits::default()).unwrap() else {
        panic!("expected plan");
    };
    plan.steps.push(plan.steps[0].clone());
    assert!(replay(&task, &plan.steps).is_err());
    plan.steps.truncate(1);
    plan.steps[0].name = "missing".into();
    assert!(replay(&task, &plan.steps).is_err());
}

#[test]
fn add_delete_overlap_uses_documented_add_wins_rule() {
    let task = parse_dsl(
        "problem overlap {
          types;
          objects {}
          predicates { Done(); }
          init: true;
          goal: Done();
          action mark() { pre: true; effect: !Done() & Done(); }
        }",
    )
    .unwrap();
    let SearchOutcome::Solved(plan) = solve(&task, SearchLimits::default()).unwrap() else {
        panic!("add must win over delete");
    };
    assert_eq!(plan.steps.len(), 1);
    assert!(evaluate(&task, &replay(&task, &plan.steps).unwrap(), &task.goal).unwrap());
}
