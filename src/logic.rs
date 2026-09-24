use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::model::{Atom, Binding, Error, Formula, GroundAtom, State, Task, Term};

const MAX_RECURSION: usize = 256;

/// Checks every declaration, formula, effect, and initial-state atom in a task.
pub fn validate(task: &Task) -> Result<(), Error> {
    let types: BTreeSet<&str> = task.types.iter().map(String::as_str).collect();
    if types.len() != task.types.len() {
        return Err(Error::new("duplicate type declaration"));
    }
    if types.contains("object") {
        return Err(Error::new(
            "type `object` is implicit and cannot be redeclared",
        ));
    }
    for ty in &task.types {
        if ty.is_empty() {
            return Err(Error::new("type names must not be empty"));
        }
    }
    let known_type = |ty: &str| ty == "object" || types.contains(ty);

    let mut objects = BTreeMap::new();
    for object in &task.objects {
        if object.name.is_empty() {
            return Err(Error::new("object names must not be empty"));
        }
        if !known_type(&object.ty) {
            return Err(Error::new(format!(
                "unknown type `{}` for object `{}`",
                object.ty, object.name
            )));
        }
        if objects
            .insert(object.name.as_str(), object.ty.as_str())
            .is_some()
        {
            return Err(Error::new(format!("duplicate object `{}`", object.name)));
        }
    }

    let mut predicates = BTreeMap::new();
    for predicate in &task.predicates {
        if predicate.name.is_empty() {
            return Err(Error::new("predicate names must not be empty"));
        }
        for ty in &predicate.parameters {
            if !known_type(ty) {
                return Err(Error::new(format!(
                    "unknown type `{ty}` in predicate `{}`",
                    predicate.name
                )));
            }
        }
        if predicates
            .insert(predicate.name.as_str(), predicate)
            .is_some()
        {
            return Err(Error::new(format!(
                "duplicate predicate `{}`",
                predicate.name
            )));
        }
    }

    let mut action_names = BTreeSet::new();
    for action in &task.actions {
        if action.parameters.len() > MAX_RECURSION {
            return Err(Error::new("action exceeds the 256-parameter limit"));
        }
        if action.name.is_empty() {
            return Err(Error::new("action names must not be empty"));
        }
        if !action_names.insert(action.name.as_str()) {
            return Err(Error::new(format!("duplicate action `{}`", action.name)));
        }
        let mut variables = HashMap::new();
        for parameter in &action.parameters {
            if parameter.name.is_empty() {
                return Err(Error::new(format!(
                    "empty parameter name in action `{}`",
                    action.name
                )));
            }
            if !known_type(&parameter.ty) {
                return Err(Error::new(format!(
                    "unknown type `{}` in action `{}`",
                    parameter.ty, action.name
                )));
            }
            if variables
                .insert(parameter.name.as_str(), parameter.ty.as_str())
                .is_some()
            {
                return Err(Error::new(format!(
                    "duplicate parameter `{}` in action `{}`",
                    parameter.name, action.name
                )));
            }
        }
        check_formula_depth(&action.precondition)?;
        check_formula(task, &action.precondition, &variables)?;
        for atom in action.add.iter().chain(&action.delete) {
            check_atom(task, atom, &variables)?;
        }
    }

    for atom in &task.initial {
        check_ground_atom(task, atom)?;
    }
    check_formula_depth(&task.goal)?;
    check_formula(task, &task.goal, &HashMap::new())
}

/// Evaluates a closed formula in a validated finite task and state.
pub fn evaluate(task: &Task, state: &State, formula: &Formula) -> Result<bool, Error> {
    validate(task)?;
    validate_state(task, state)?;
    check_formula_depth(formula)?;
    check_formula(task, formula, &HashMap::new())?;
    eval(task, state, formula, &mut HashMap::new())
}

fn check_formula_depth(formula: &Formula) -> Result<(), Error> {
    // Flat binder lists also consume evaluator stack, even without nested syntax.
    // Check iteratively before entering recursive validation or evaluation.
    let mut pending = vec![(formula, 0usize)];
    while let Some((formula, depth)) = pending.pop() {
        if depth > MAX_RECURSION {
            return Err(Error::new(
                "formula exceeds the 256-level nesting/binding limit",
            ));
        }
        match formula {
            Formula::Not(body) => pending.push((body, depth + 1)),
            Formula::And(parts) | Formula::Or(parts) => {
                pending.extend(parts.iter().map(|part| (part, depth + 1)));
            }
            Formula::Implies(left, right) => {
                pending.push((left, depth + 1));
                pending.push((right, depth + 1));
            }
            Formula::Exists(bindings, body) | Formula::Forall(bindings, body) => {
                pending.push((body, depth.saturating_add(bindings.len()).saturating_add(1)));
            }
            Formula::Atom(_) | Formula::Equal(_, _) => {}
        }
    }
    Ok(())
}

pub(crate) fn validate_state(task: &Task, state: &State) -> Result<(), Error> {
    for atom in state {
        check_ground_atom(task, atom)?;
    }
    Ok(())
}

pub(crate) fn eval_with_bindings(
    task: &Task,
    state: &State,
    formula: &Formula,
    bindings: &mut HashMap<String, String>,
) -> Result<bool, Error> {
    eval(task, state, formula, bindings)
}

fn check_formula(task: &Task, formula: &Formula, scope: &HashMap<&str, &str>) -> Result<(), Error> {
    let known_type = |ty: &str| ty == "object" || task.types.iter().any(|declared| declared == ty);
    match formula {
        Formula::Atom(atom) => check_atom(task, atom, scope)?,
        Formula::Equal(left, right) => {
            check_term(task, left, scope, None)?;
            check_term(task, right, scope, None)?;
        }
        Formula::Not(inner) => check_formula(task, inner, scope)?,
        Formula::And(parts) | Formula::Or(parts) => {
            for part in parts {
                check_formula(task, part, scope)?;
            }
        }
        Formula::Implies(left, right) => {
            check_formula(task, left, scope)?;
            check_formula(task, right, scope)?;
        }
        Formula::Exists(bindings, inner) | Formula::Forall(bindings, inner) => {
            let mut nested = scope.clone();
            let mut local_names = BTreeSet::new();
            for binding in bindings {
                if binding.name.is_empty() {
                    return Err(Error::new("quantified variable names must not be empty"));
                }
                if !known_type(&binding.ty) {
                    return Err(Error::new(format!(
                        "unknown quantified type `{}`",
                        binding.ty
                    )));
                }
                if !local_names.insert(binding.name.as_str()) {
                    return Err(Error::new(format!(
                        "duplicate quantified variable `{}`",
                        binding.name
                    )));
                }
                nested.insert(binding.name.as_str(), binding.ty.as_str());
            }
            check_formula(task, inner, &nested)?;
        }
    }
    Ok(())
}

fn check_atom(task: &Task, atom: &Atom, scope: &HashMap<&str, &str>) -> Result<(), Error> {
    let predicate = task
        .predicates
        .iter()
        .find(|p| p.name == atom.predicate)
        .ok_or_else(|| Error::new(format!("unknown predicate `{}`", atom.predicate)))?;
    if predicate.parameters.len() != atom.terms.len() {
        return Err(Error::new(format!(
            "predicate `{}` expects {} arguments, got {}",
            atom.predicate,
            predicate.parameters.len(),
            atom.terms.len()
        )));
    }
    for (term, expected) in atom.terms.iter().zip(&predicate.parameters) {
        check_term(task, term, scope, Some(expected))?;
    }
    Ok(())
}

fn check_term(
    task: &Task,
    term: &Term,
    scope: &HashMap<&str, &str>,
    expected_type: Option<&str>,
) -> Result<(), Error> {
    let (name, actual_type) = match term {
        Term::Variable(name) => match scope.get(name.as_str()) {
            Some(ty) => (name.as_str(), *ty),
            None => return Err(Error::new(format!("unbound variable `{name}`"))),
        },
        Term::Constant(name) => {
            let ty = task
                .objects
                .iter()
                .find(|object| object.name == *name)
                .map(|object| object.ty.as_str())
                .ok_or_else(|| Error::new(format!("unknown object `{name}`")))?;
            (name.as_str(), ty)
        }
    };
    if let Some(expected) =
        expected_type.filter(|expected| *expected != "object" && actual_type != *expected)
    {
        return Err(Error::new(format!(
            "term `{name}` has type `{actual_type}`, expected `{expected}`"
        )));
    }
    Ok(())
}

fn check_ground_atom(task: &Task, atom: &GroundAtom) -> Result<(), Error> {
    let predicate = task
        .predicates
        .iter()
        .find(|p| p.name == atom.predicate)
        .ok_or_else(|| Error::new(format!("unknown predicate `{}`", atom.predicate)))?;
    if predicate.parameters.len() != atom.arguments.len() {
        return Err(Error::new(format!(
            "predicate `{}` expects {} arguments, got {}",
            atom.predicate,
            predicate.parameters.len(),
            atom.arguments.len()
        )));
    }
    for (name, expected) in atom.arguments.iter().zip(&predicate.parameters) {
        let actual = task
            .objects
            .iter()
            .find(|object| object.name == *name)
            .map(|object| object.ty.as_str())
            .ok_or_else(|| Error::new(format!("unknown object `{name}`")))?;
        if expected != "object" && actual != expected {
            return Err(Error::new(format!(
                "object `{name}` has type `{actual}`, expected `{expected}`"
            )));
        }
    }
    Ok(())
}

fn eval(
    task: &Task,
    state: &State,
    formula: &Formula,
    env: &mut HashMap<String, String>,
) -> Result<bool, Error> {
    Ok(match formula {
        Formula::Atom(atom) => state.contains(&GroundAtom {
            predicate: atom.predicate.clone(),
            arguments: atom
                .terms
                .iter()
                .map(|term| resolve(term, env))
                .collect::<Result<_, _>>()?,
        }),
        Formula::Equal(left, right) => resolve(left, env)? == resolve(right, env)?,
        Formula::Not(inner) => !eval(task, state, inner, env)?,
        Formula::And(parts) => {
            let mut result = true;
            for part in parts {
                if !eval(task, state, part, env)? {
                    result = false;
                    break;
                }
            }
            result
        }
        Formula::Or(parts) => {
            let mut result = false;
            for part in parts {
                if eval(task, state, part, env)? {
                    result = true;
                    break;
                }
            }
            result
        }
        Formula::Implies(left, right) => {
            !eval(task, state, left, env)? || eval(task, state, right, env)?
        }
        Formula::Exists(bindings, inner) => quantify(task, state, bindings, inner, env, true)?,
        Formula::Forall(bindings, inner) => quantify(task, state, bindings, inner, env, false)?,
    })
}

fn quantify(
    task: &Task,
    state: &State,
    bindings: &[Binding],
    inner: &Formula,
    env: &mut HashMap<String, String>,
    existential: bool,
) -> Result<bool, Error> {
    fn visit(
        task: &Task,
        state: &State,
        bindings: &[Binding],
        index: usize,
        inner: &Formula,
        env: &mut HashMap<String, String>,
        existential: bool,
    ) -> Result<bool, Error> {
        if index == bindings.len() {
            return eval(task, state, inner, env);
        }
        let binding = &bindings[index];
        let prior = env.remove(&binding.name);
        let mut result = !existential;
        for object in task
            .objects
            .iter()
            .filter(|object| binding.ty == "object" || object.ty == binding.ty)
        {
            env.insert(binding.name.clone(), object.name.clone());
            let value = visit(task, state, bindings, index + 1, inner, env, existential)?;
            if value == existential {
                result = existential;
                break;
            }
        }
        env.remove(&binding.name);
        if let Some(value) = prior {
            env.insert(binding.name.clone(), value);
        }
        Ok(result)
    }
    visit(task, state, bindings, 0, inner, env, existential)
}

fn resolve(term: &Term, env: &HashMap<String, String>) -> Result<String, Error> {
    match term {
        Term::Constant(name) => Ok(name.clone()),
        Term::Variable(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| Error::new(format!("unbound variable `?{name}` during evaluation"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Task {
        Task {
            name: "nested".into(),
            types: vec!["place".into()],
            objects: vec![
                Binding {
                    name: "a".into(),
                    ty: "place".into(),
                },
                Binding {
                    name: "b".into(),
                    ty: "place".into(),
                },
            ],
            predicates: vec![crate::model::Predicate {
                name: "at".into(),
                parameters: vec!["place".into()],
            }],
            actions: vec![],
            initial: BTreeSet::from([GroundAtom {
                predicate: "at".into(),
                arguments: vec!["a".into()],
            }]),
            goal: Formula::And(vec![]),
        }
    }

    #[test]
    fn finite_quantifiers_support_shadowing_and_empty_types() {
        let mut t = task();
        t.types.push("empty".into());
        let shadowed = Formula::Exists(
            vec![Binding {
                name: "x".into(),
                ty: "place".into(),
            }],
            Box::new(Formula::Forall(
                vec![Binding {
                    name: "x".into(),
                    ty: "place".into(),
                }],
                Box::new(Formula::Or(vec![
                    Formula::Equal(Term::Variable("x".into()), Term::Constant("a".into())),
                    Formula::Equal(Term::Variable("x".into()), Term::Constant("b".into())),
                ])),
            )),
        );
        assert!(evaluate(&t, &t.initial, &shadowed).unwrap());
        assert!(
            evaluate(
                &t,
                &t.initial,
                &Formula::Forall(
                    vec![Binding {
                        name: "z".into(),
                        ty: "empty".into()
                    }],
                    Box::new(Formula::Atom(Atom {
                        predicate: "missing".into(),
                        terms: vec![]
                    }))
                )
            )
            .is_err()
        );
        let empty_forall = Formula::Forall(
            vec![Binding {
                name: "z".into(),
                ty: "empty".into(),
            }],
            Box::new(Formula::Equal(
                Term::Constant("a".into()),
                Term::Constant("b".into()),
            )),
        );
        assert!(evaluate(&t, &t.initial, &empty_forall).unwrap());
    }

    #[test]
    fn validation_rejects_free_goal_variables_and_bad_state_types() {
        let mut t = task();
        t.goal = Formula::Atom(Atom {
            predicate: "at".into(),
            terms: vec![Term::Variable("x".into())],
        });
        assert!(
            validate(&t)
                .unwrap_err()
                .message
                .contains("unbound variable")
        );
        t.goal = Formula::And(vec![]);
        t.initial.insert(GroundAtom {
            predicate: "at".into(),
            arguments: vec!["ghost".into()],
        });
        assert!(validate(&t).is_err());
    }
}
