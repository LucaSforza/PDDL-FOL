use pddl_fol::{
    ErrorKind, SearchAlgorithm, SearchLimits, SearchOutcome, Task, parse_dsl, parse_pddl,
    solve_with_algorithm, validate,
};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

const USAGE: &str = "Usage:\n  folplan solve file.fol [--search astar|bfs] [--max-states N] [--max-ground-actions N]\n  folplan pddl domain.pddl problem.pddl [--search astar|bfs] [--max-states N] [--max-ground-actions N]\n  folplan check file.fol\n  folplan check-pddl domain.pddl problem.pddl\n  folplan --help";

fn main() {
    process::exit(run(std::env::args_os().skip(1).collect()));
}

fn run(args: Vec<OsString>) -> i32 {
    if args.is_empty() {
        return argument_error("missing command");
    }
    let command = match args[0].to_str() {
        Some(command) => command,
        None => return argument_error("command must be valid UTF-8"),
    };
    if command == "--help" || command == "-h" || command == "help" {
        if args.len() != 1 {
            return argument_error("help does not take arguments");
        }
        println!("{USAGE}");
        return 0;
    }

    match command {
        "solve" => run_solve(&args[1..]),
        "pddl" => run_pddl(&args[1..]),
        "check" => run_check(&args[1..]),
        "check-pddl" => run_check_pddl(&args[1..]),
        _ => argument_error(format!("unknown command '{command}'")),
    }
}

fn run_solve(args: &[OsString]) -> i32 {
    let (paths, limits, algorithm) = match parse_search_args(args, 1) {
        Ok(parsed) => parsed,
        Err(message) => return argument_error(message),
    };
    let source = match read_source(&paths[0]) {
        Ok(source) => source,
        Err(message) => return report_error(message, 1),
    };
    let task = match parse_dsl(&source) {
        Ok(task) => task,
        Err(error) => return report_error(error.to_string(), 1),
    };
    run_task(&task, limits, algorithm)
}

fn run_pddl(args: &[OsString]) -> i32 {
    let (paths, limits, algorithm) = match parse_search_args(args, 2) {
        Ok(parsed) => parsed,
        Err(message) => return argument_error(message),
    };
    let domain = match read_source(&paths[0]) {
        Ok(source) => source,
        Err(message) => return report_error(message, 1),
    };
    let problem = match read_source(&paths[1]) {
        Ok(source) => source,
        Err(message) => return report_error(message, 1),
    };
    let task = match parse_pddl(&domain, &problem) {
        Ok(task) => task,
        Err(error) => return report_error(error.to_string(), 1),
    };
    run_task(&task, limits, algorithm)
}

fn run_check(args: &[OsString]) -> i32 {
    if args.len() != 1 {
        return argument_error("check requires exactly one .fol file");
    }
    let source = match read_source(&args[0]) {
        Ok(source) => source,
        Err(message) => return report_error(message, 1),
    };
    match parse_dsl(&source).and_then(|task| validate(&task).map(|()| task)) {
        Ok(task) => {
            println!("valid: {}", task.name);
            0
        }
        Err(error) => report_error(error.to_string(), 1),
    }
}

fn run_check_pddl(args: &[OsString]) -> i32 {
    if args.len() != 2 {
        return argument_error("check-pddl requires a domain and a problem file");
    }
    let domain = match read_source(&args[0]) {
        Ok(source) => source,
        Err(message) => return report_error(message, 1),
    };
    let problem = match read_source(&args[1]) {
        Ok(source) => source,
        Err(message) => return report_error(message, 1),
    };
    match parse_pddl(&domain, &problem).and_then(|task| validate(&task).map(|()| task)) {
        Ok(task) => {
            println!("valid: {}", task.name);
            0
        }
        Err(error) => report_error(error.to_string(), 1),
    }
}

fn run_task(task: &Task, limits: SearchLimits, algorithm: SearchAlgorithm) -> i32 {
    match solve_with_algorithm(task, limits, algorithm) {
        Ok(SearchOutcome::Solved(plan)) => {
            println!("Search: {}", algorithm_name(algorithm));
            println!(
                "Plan ({} step{}):",
                plan.steps.len(),
                if plan.steps.len() == 1 { "" } else { "s" }
            );
            if plan.steps.is_empty() {
                println!("  (empty)");
            } else {
                for (index, step) in plan.steps.iter().enumerate() {
                    println!("  {}. {step}", index + 1);
                }
            }
            println!("Situation: {}", plan.situation());
            println!("States explored: {}", plan.explored);
            0
        }
        Ok(SearchOutcome::Unsolvable { explored }) => {
            println!(
                "Search: {}\nImpossible: no plan exists.\nStates explored: {explored}",
                algorithm_name(algorithm)
            );
            2
        }
        Ok(SearchOutcome::LimitReached { explored }) => {
            println!(
                "Search: {}\nSearch limit reached.\nStates explored: {explored}",
                algorithm_name(algorithm)
            );
            3
        }
        Err(error) => report_error(
            error.to_string(),
            if error.kind() == ErrorKind::GroundingLimit {
                3
            } else {
                1
            },
        ),
    }
}

fn parse_search_args(
    args: &[OsString],
    path_count: usize,
) -> Result<(Vec<PathBuf>, SearchLimits, SearchAlgorithm), String> {
    let mut paths = Vec::with_capacity(path_count);
    let mut limits = SearchLimits::default();
    let mut algorithm = SearchAlgorithm::AStar;
    let mut seen_search = false;
    let mut seen_states = false;
    let mut seen_ground = false;
    let mut index = 0;
    while index < args.len() {
        let item = &args[index];
        let option = item
            .to_str()
            .ok_or_else(|| "arguments must be valid UTF-8".to_owned())?;
        if option == "--search" {
            if seen_search {
                return Err("--search may be specified only once".into());
            }
            let value = args
                .get(index + 1)
                .and_then(|value| value.to_str())
                .ok_or_else(|| "--search requires astar or bfs".to_owned())?;
            algorithm = match value {
                "astar" => SearchAlgorithm::AStar,
                "bfs" => SearchAlgorithm::Bfs,
                _ => return Err("--search must be astar or bfs".into()),
            };
            seen_search = true;
            index += 2;
        } else if option == "--max-states" || option == "--max-ground-actions" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("{option} requires a positive integer"))?;
            let value = value
                .to_str()
                .ok_or_else(|| format!("{option} value must be valid UTF-8"))?;
            let parsed = value
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("{option} must be a positive integer"))?;
            match option {
                "--max-states" => {
                    if seen_states {
                        return Err("--max-states may be specified only once".into());
                    }
                    limits.max_states = parsed;
                    seen_states = true;
                }
                _ => {
                    if seen_ground {
                        return Err("--max-ground-actions may be specified only once".into());
                    }
                    limits.max_ground_actions = parsed;
                    seen_ground = true;
                }
            }
            index += 2;
        } else if option.starts_with('-') {
            return Err(format!("unknown option '{option}'"));
        } else {
            paths.push(PathBuf::from(item));
            index += 1;
        }
    }
    if paths.len() != path_count {
        return Err(format!(
            "expected {path_count} input file{}",
            if path_count == 1 { "" } else { "s" }
        ));
    }
    Ok((paths, limits, algorithm))
}

fn algorithm_name(algorithm: SearchAlgorithm) -> &'static str {
    match algorithm {
        SearchAlgorithm::AStar => "A*",
        SearchAlgorithm::Bfs => "BFS",
    }
}

fn read_source(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn argument_error(message: impl AsRef<str>) -> i32 {
    eprintln!("error: {}\n{USAGE}", message.as_ref());
    1
}

fn report_error(message: impl AsRef<str>, code: i32) -> i32 {
    eprintln!("error: {}", message.as_ref());
    code
}
