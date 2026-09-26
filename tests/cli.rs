use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_folplan"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("CLI must start")
}

fn assert_exit(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn every_dsl_example_validates_and_runs() {
    for (name, exit) in [
        ("travel", 0),
        ("blocks", 0),
        ("blocks-quantified", 0),
        ("logistics", 0),
        ("quantified-inspection", 0),
        ("impossible", 2),
        ("satisfied", 0),
    ] {
        let file = format!("examples/{name}.fol");
        assert_exit(&cli(&["check", &file]), 0);
        let output = cli(&["solve", &file]);
        assert_exit(&output, exit);
        if exit == 0 {
            assert!(String::from_utf8_lossy(&output.stdout).contains("Situation:"));
        }
    }
}

#[test]
fn search_flag_selects_algorithm_and_defaults_to_astar() {
    let default = cli(&["solve", "examples/travel.fol"]);
    assert_exit(&default, 0);
    assert!(String::from_utf8_lossy(&default.stdout).contains("Search: A*"));

    let astar = cli(&["solve", "examples/travel.fol", "--search", "astar"]);
    assert_exit(&astar, 0);
    assert!(String::from_utf8_lossy(&astar.stdout).contains("Search: A*"));

    let bfs = cli(&["solve", "examples/travel.fol", "--search", "bfs"]);
    assert_exit(&bfs, 0);
    assert!(String::from_utf8_lossy(&bfs.stdout).contains("Search: BFS"));
    assert_eq!(plan_steps(&default), plan_steps(&bfs));

    let pddl = cli(&[
        "pddl",
        "examples/pddl/travel-domain.pddl",
        "examples/pddl/travel-problem.pddl",
        "--search",
        "bfs",
    ]);
    assert_exit(&pddl, 0);
    assert!(String::from_utf8_lossy(&pddl.stdout).contains("Search: BFS"));
}

fn plan_steps(output: &Output) -> Option<usize> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().find(|line| line.starts_with("Plan ("))?;
    line.strip_prefix("Plan (")?
        .split_once(" step")?
        .0
        .parse()
        .ok()
}

#[test]
fn every_pddl_example_validates_and_runs() {
    for (name, exit) in [
        ("travel", 0),
        ("blocks", 0),
        ("logistics", 0),
        ("inspection", 0),
        ("impossible", 2),
        ("satisfied", 0),
    ] {
        let domain = format!("examples/pddl/{name}-domain.pddl");
        let problem = format!("examples/pddl/{name}-problem.pddl");
        assert_exit(&cli(&["check-pddl", &domain, &problem]), 0);
        assert_exit(&cli(&["pddl", &domain, &problem]), exit);
    }
}

#[test]
fn cli_reports_errors_and_resource_limits_with_distinct_exit_codes() {
    assert_exit(&cli(&["--help"]), 0);
    for args in [
        vec![],
        vec!["unknown"],
        vec!["solve", "missing-file.fol"],
        vec!["solve", "examples/travel.fol", "--max-states", "0"],
        vec!["solve", "examples/travel.fol", "--max-states", "-1"],
        vec!["solve", "examples/travel.fol", "--max-states"],
        vec!["solve", "examples/travel.fol", "--unknown"],
        vec!["solve", "examples/travel.fol", "--search", "dfs"],
        vec!["solve", "examples/travel.fol", "--search"],
        vec![
            "solve",
            "examples/travel.fol",
            "--search",
            "bfs",
            "--search",
            "astar",
        ],
        vec!["check", "examples/travel.fol", "extra"],
        vec!["pddl", "examples/pddl/travel-domain.pddl"],
        vec![
            "solve",
            "examples/travel.fol",
            "--max-states",
            "1",
            "--max-states",
            "2",
        ],
    ] {
        let output = cli(&args);
        assert_exit(&output, 1);
        assert!(!output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
    assert_exit(
        &cli(&["solve", "examples/travel.fol", "--max-states", "1"]),
        3,
    );
    assert_exit(
        &cli(&[
            "solve",
            "examples/blocks-quantified.fol",
            "--max-ground-actions",
            "1",
        ]),
        3,
    );
}
