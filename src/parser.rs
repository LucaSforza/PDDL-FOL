//! Parser for the supported PDDL subset.

use crate::model::GroundAtom;
use crate::model::{Action, Atom, Binding, Error, Formula, Predicate, State, Task, Term};
use std::collections::{BTreeMap, BTreeSet};

const MAX_DEPTH: usize = 256;

#[derive(Clone, Debug)]
struct Node {
    kind: Kind,
    line: usize,
    column: usize,
}

#[derive(Clone, Debug)]
enum Kind {
    Atom(String),
    List(Vec<Node>),
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    line: usize,
    column: usize,
}

/// Parse PDDL domain and problem sources into one task.
pub fn parse_pddl(domain: &str, problem: &str) -> Result<Task, Error> {
    let droot = parse_one(domain)?;
    let proot = parse_one(problem)?;
    let dform = list(&droot, "domain definition")?;
    let pform = list(&proot, "problem definition")?;
    if pform
        .iter()
        .skip(2)
        .any(|n| head(n).as_deref() == Some(":action"))
    {
        return Err(at(&proot, "problem definitions cannot contain :action"));
    }
    expect_head(dform, "define", &droot)?;
    expect_head(pform, "define", &proot)?;
    let (domain_name, dsections) = parse_pddl_header(dform, &droot, "domain")?;
    let (problem_name, psections) = parse_pddl_header(pform, &proot, "problem")?;
    let p_domain = list(psections[":domain"], "problem :domain section")?;
    if p_domain.len() != 2 || atom(&p_domain[0])? != ":domain" {
        return Err(at(psections[":domain"], "expected '(:domain name)'"));
    }
    if name(&p_domain[1], false)? != domain_name {
        return Err(at(
            psections[":domain"],
            "problem refers to a different domain",
        ));
    }

    let mut types = Vec::new();
    if let Some(section) = dsections.get(":types") {
        types = parse_pddl_types(section)?;
    }
    let mut constants = Vec::new();
    if let Some(section) = dsections.get(":constants") {
        constants = parse_typed_list(section_items(section, ":constants")?, false)?;
    }
    let mut objects = constants;
    if let Some(section) = psections.get(":objects") {
        objects.extend(parse_typed_list(
            section_items(section, ":objects")?,
            false,
        )?);
    }
    let predicates = parse_pddl_predicates(dsections[":predicates"])?;
    let mut actions = Vec::new();
    for section in dform.iter().skip(2) {
        if head(section).as_deref() == Some(":action") {
            actions.push(parse_pddl_action(section)?);
        }
    }
    let initial = parse_pddl_initial(psections[":init"])?;
    let goal = parse_section_formula(psections[":goal"], ":goal")?;

    Ok(Task {
        name: problem_name,
        types,
        objects,
        predicates,
        actions,
        initial,
        goal,
    })
}

fn parse_one(source: &str) -> Result<Node, Error> {
    let tokens = lex(source)?;
    let mut pos = 0;
    let node = parse_node(&tokens, &mut pos, 0)?;
    if pos != tokens.len() {
        return Err(token_error(&tokens[pos], "unexpected trailing input"));
    }
    Ok(node)
}

fn lex(source: &str) -> Result<Vec<Token>, Error> {
    let mut result = Vec::new();
    let mut chars = source.char_indices().peekable();
    let (mut line, mut column) = (1usize, 1usize);
    while let Some((_, ch)) = chars.next() {
        if ch == '\n' {
            line += 1;
            column = 1;
            continue;
        }
        if ch == ';' {
            for (_, c) in chars.by_ref() {
                if c == '\n' {
                    line += 1;
                    column = 1;
                    break;
                }
                column += 1;
            }
            continue;
        }
        if ch.is_whitespace() {
            column += 1;
            continue;
        }
        let (start_line, start_column) = (line, column);
        if ch == '(' || ch == ')' {
            result.push(Token {
                text: ch.to_string(),
                line,
                column,
            });
            column += 1;
            continue;
        }
        let mut text = String::from(ch);
        column += 1;
        while let Some((_, next)) = chars.peek().copied() {
            if next.is_whitespace() || next == '(' || next == ')' || next == ';' {
                break;
            }
            chars.next();
            text.push(next);
            column += 1;
        }
        if !text.is_ascii() {
            return Err(Error::new(format!(
                "{start_line}:{start_column}: identifiers must be ASCII"
            )));
        }
        result.push(Token {
            text: text.to_ascii_lowercase(),
            line: start_line,
            column: start_column,
        });
    }
    Ok(result)
}

fn parse_node(tokens: &[Token], pos: &mut usize, depth: usize) -> Result<Node, Error> {
    if depth > MAX_DEPTH {
        let token = tokens.get(*pos).cloned().unwrap_or(Token {
            text: "".into(),
            line: 1,
            column: 1,
        });
        return Err(token_error(&token, "maximum nesting depth exceeded"));
    }
    let token = tokens
        .get(*pos)
        .ok_or_else(|| Error::new("unexpected end of input"))?
        .clone();
    *pos += 1;
    if token.text == ")" {
        return Err(token_error(&token, "unexpected ')'"));
    }
    if token.text != "(" {
        return Ok(Node {
            kind: Kind::Atom(token.text),
            line: token.line,
            column: token.column,
        });
    }
    let mut items = Vec::new();
    loop {
        let next = tokens
            .get(*pos)
            .ok_or_else(|| token_error(&token, "unclosed list"))?;
        if next.text == ")" {
            *pos += 1;
            return Ok(Node {
                kind: Kind::List(items),
                line: token.line,
                column: token.column,
            });
        }
        items.push(parse_node(tokens, pos, depth + 1)?);
    }
}

fn list<'a>(node: &'a Node, what: &str) -> Result<&'a [Node], Error> {
    match &node.kind {
        Kind::List(items) => Ok(items),
        Kind::Atom(_) => Err(at(node, format!("expected {what}"))),
    }
}

fn atom(node: &Node) -> Result<&str, Error> {
    match &node.kind {
        Kind::Atom(value) => Ok(value),
        Kind::List(_) => Err(at(node, "expected an identifier")),
    }
}

fn head(node: &Node) -> Option<String> {
    match &node.kind {
        Kind::List(items) => items.first().and_then(|n| atom(n).ok()).map(str::to_owned),
        Kind::Atom(_) => None,
    }
}

fn expect_head(items: &[Node], expected: &str, node: &Node) -> Result<(), Error> {
    if items.first().and_then(|n| atom(n).ok()) != Some(expected) {
        return Err(at(node, format!("expected '{expected}' form")));
    }
    Ok(())
}

fn name(node: &Node, variable: bool) -> Result<String, Error> {
    let value = atom(node)?;
    let raw = if variable {
        value
            .strip_prefix('?')
            .ok_or_else(|| at(node, "expected variable beginning with '?'"))?
    } else {
        value
    };
    if raw.is_empty()
        || !raw.as_bytes()[0].is_ascii_alphabetic()
        || !raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(at(
            node,
            format!(
                "invalid {} name '{value}'",
                if variable { "variable" } else { "identifier" }
            ),
        ));
    }
    if !variable && value.starts_with('?') {
        return Err(at(node, "unexpected variable where a name was required"));
    }
    Ok(if variable {
        raw.to_owned()
    } else {
        value.to_owned()
    })
}

fn unique_sections(nodes: &[Node], allow_actions: bool) -> Result<BTreeMap<String, &Node>, Error> {
    let mut sections = BTreeMap::new();
    for node in nodes {
        let items = list(node, "section")?;
        let key = items.first().ok_or_else(|| at(node, "empty section"))?;
        let key = atom(key)?.to_owned();
        if allow_actions && key == ":action" {
            continue;
        }
        if sections.insert(key.clone(), node).is_some() {
            return Err(at(node, format!("duplicate section '{key}'")));
        }
    }
    Ok(sections)
}

fn section_items<'a>(node: &'a Node, expected: &str) -> Result<&'a [Node], Error> {
    let items = list(node, "section")?;
    expect_head(items, expected, node)?;
    Ok(&items[1..])
}

fn parse_section_formula(node: &Node, section: &str) -> Result<Formula, Error> {
    let items = section_items(node, section)?;
    if items.len() != 1 {
        return Err(at(
            node,
            format!("{section} section requires exactly one formula"),
        ));
    }
    parse_formula(&items[0], 0)
}

fn parse_formula(node: &Node, depth: usize) -> Result<Formula, Error> {
    if depth > MAX_DEPTH {
        return Err(at(node, "maximum formula nesting depth exceeded"));
    }
    let items = list(node, "formula")?;
    if items.is_empty() {
        return Err(at(node, "empty formula"));
    }
    let op = atom(&items[0])?;
    match op {
        "and" | "or" => {
            let children = items[1..]
                .iter()
                .map(|n| parse_formula(n, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(if op == "and" {
                Formula::And(children)
            } else {
                Formula::Or(children)
            })
        }
        "not" => {
            if items.len() != 2 {
                return Err(at(node, "not requires exactly one formula"));
            }
            Ok(Formula::Not(Box::new(parse_formula(&items[1], depth + 1)?)))
        }
        "imply" => {
            if items.len() != 3 {
                return Err(at(node, "imply requires exactly two formulas"));
            }
            Ok(Formula::Implies(
                Box::new(parse_formula(&items[1], depth + 1)?),
                Box::new(parse_formula(&items[2], depth + 1)?),
            ))
        }
        "exists" | "forall" => {
            if items.len() != 3 {
                return Err(at(node, "quantifier requires bindings and one formula"));
            }
            let bindings = parse_quantified_bindings(&items[1])?;
            let body = parse_formula(&items[2], depth + 1)?;
            Ok(if op == "exists" {
                Formula::Exists(bindings, Box::new(body))
            } else {
                Formula::Forall(bindings, Box::new(body))
            })
        }
        "=" => {
            if items.len() != 3 {
                return Err(at(node, "equality requires exactly two terms"));
            }
            Ok(Formula::Equal(
                parse_term(&items[1])?,
                parse_term(&items[2])?,
            ))
        }
        ":when" | "when" | "increase" | "decrease" | "assign" => {
            Err(at(node, format!("unsupported construct '{op}'")))
        }
        _ => Ok(Formula::Atom(parse_atom(node)?)),
    }
}

fn parse_term(node: &Node) -> Result<Term, Error> {
    let value = atom(node)?;
    if value.starts_with('?') {
        Ok(Term::Variable(name(node, true)?))
    } else {
        Ok(Term::Constant(name(node, false)?))
    }
}

fn parse_atom(node: &Node) -> Result<Atom, Error> {
    let items = list(node, "atom")?;
    if items.is_empty() {
        return Err(at(node, "empty atom"));
    }
    let predicate = predicate_name(&items[0])?;
    let terms = items[1..]
        .iter()
        .map(parse_term)
        .collect::<Result<_, _>>()?;
    Ok(Atom { predicate, terms })
}

fn parse_pddl_header<'a>(
    items: &'a [Node],
    root: &Node,
    kind: &str,
) -> Result<(String, BTreeMap<String, &'a Node>), Error> {
    if items.len() < 2 {
        return Err(at(root, "incomplete define form"));
    }
    let header = list(&items[1], "domain/problem header")?;
    if header.len() != 2
        || atom(&header[0])?
            != if kind == "domain" {
                "domain"
            } else {
                "problem"
            }
    {
        return Err(at(&items[1], format!("expected '({kind} name)' header")));
    }
    let entity_name = name(&header[1], false)?;
    let sections = unique_sections(&items[2..], kind == "domain")?;
    if kind == "domain" {
        for key in sections.keys() {
            if ![":requirements", ":types", ":constants", ":predicates"].contains(&key.as_str()) {
                return Err(at(sections[key], format!("unknown domain section '{key}'")));
            }
        }
        if !sections.contains_key(":predicates") {
            return Err(at(root, "domain is missing :predicates"));
        }
        if let Some(req) = sections.get(":requirements") {
            validate_requirements(req)?;
        }
    } else {
        for key in sections.keys() {
            if ![":domain", ":objects", ":init", ":goal"].contains(&key.as_str()) {
                return Err(at(
                    sections[key],
                    format!("unknown problem section '{key}'"),
                ));
            }
        }
        for key in [":domain", ":init", ":goal"] {
            if !sections.contains_key(key) {
                return Err(at(root, format!("problem is missing {key}")));
            }
        }
    }
    Ok((entity_name, sections))
}

fn validate_requirements(node: &Node) -> Result<(), Error> {
    let accepted = [
        ":strips",
        ":typing",
        ":negative-preconditions",
        ":disjunctive-preconditions",
        ":equality",
        ":existential-preconditions",
        ":universal-preconditions",
        ":quantified-preconditions",
    ];
    for req in section_items(node, ":requirements")? {
        let value = atom(req)?;
        if !accepted.contains(&value) {
            return Err(at(req, format!("unsupported requirement '{value}'")));
        }
    }
    Ok(())
}

fn parse_pddl_types(node: &Node) -> Result<Vec<String>, Error> {
    let items = section_items(node, ":types")?;
    let mut names = Vec::new();
    let mut group = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let word = atom(&items[i])?;
        if word == "-" {
            if group.is_empty() || i + 1 >= items.len() {
                return Err(at(&items[i], "malformed typed type list"));
            }
            let parent = name(&items[i + 1], false)?;
            if parent != "object" {
                return Err(at(&items[i + 1], "hierarchical types are not supported"));
            }
            names.append(&mut group);
            i += 2;
        } else {
            group.push(name(&items[i], false)?);
            i += 1;
        }
    }
    names.append(&mut group);
    Ok(names)
}

fn parse_typed_list(nodes: &[Node], variables: bool) -> Result<Vec<Binding>, Error> {
    let mut result = Vec::new();
    let mut group = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        if atom(&nodes[i])? == "-" {
            if group.is_empty() || i + 1 >= nodes.len() {
                return Err(at(&nodes[i], "malformed typed list"));
            }
            let ty = name(&nodes[i + 1], false)?;
            result.extend(group.drain(..).map(|n| Binding {
                name: n,
                ty: ty.clone(),
            }));
            i += 2;
        } else {
            group.push(name(&nodes[i], variables)?);
            i += 1;
        }
    }
    result.extend(group.into_iter().map(|n| Binding {
        name: n,
        ty: "object".into(),
    }));
    Ok(result)
}

fn parse_quantified_bindings(node: &Node) -> Result<Vec<Binding>, Error> {
    parse_typed_list(list(node, "quantifier bindings")?, true)
}

fn predicate_name(node: &Node) -> Result<String, Error> {
    let value = name(node, false)?;
    if ["and", "or", "not", "imply", "exists", "forall", "="].contains(&value.as_str()) {
        return Err(at(
            node,
            format!("reserved operator '{value}' cannot be a predicate name"),
        ));
    }
    Ok(value)
}

fn parse_pddl_predicates(node: &Node) -> Result<Vec<Predicate>, Error> {
    section_items(node, ":predicates")?
        .iter()
        .map(|decl| {
            let fields = list(decl, "predicate declaration")?;
            if fields.is_empty() {
                return Err(at(decl, "empty predicate declaration"));
            }
            let pname = predicate_name(&fields[0])?;
            let bindings = parse_typed_list(&fields[1..], true)?;
            let mut parameter_names = BTreeSet::new();
            for binding in &bindings {
                if !parameter_names.insert(binding.name.clone()) {
                    return Err(at(
                        decl,
                        format!("duplicate predicate parameter '?{}'", binding.name),
                    ));
                }
            }
            let params = bindings.into_iter().map(|b| b.ty).collect();
            Ok(Predicate {
                name: pname,
                parameters: params,
            })
        })
        .collect()
}

fn parse_pddl_action(node: &Node) -> Result<Action, Error> {
    let fields = list(node, "PDDL action")?;
    if fields.len() < 2 {
        return Err(at(node, ":action requires a name"));
    }
    let action_name = name(&fields[1], false)?;
    if fields.len() % 2 != 0 {
        return Err(at(node, "PDDL action fields must be key/value pairs"));
    }
    let mut values = BTreeMap::new();
    for pair in fields[2..].chunks_exact(2) {
        let key = atom(&pair[0])?;
        if ![":parameters", ":precondition", ":effect"].contains(&key) {
            return Err(at(&pair[0], format!("unknown action field '{key}'")));
        }
        if values.insert(key.to_owned(), &pair[1]).is_some() {
            return Err(at(&pair[0], format!("duplicate action field '{key}'")));
        }
    }
    let empty_params = Node {
        kind: Kind::List(vec![]),
        line: node.line,
        column: node.column,
    };
    let params_node = values.get(":parameters").copied().unwrap_or(&empty_params);
    let parameters = parse_typed_list(list(params_node, "action parameters")?, true)?;
    let precondition = match values.get(":precondition") {
        Some(n) => parse_formula(n, 0)?,
        None => Formula::And(Vec::new()),
    };
    let mut add = Vec::new();
    let mut delete = Vec::new();
    if let Some(effect) = values.get(":effect") {
        parse_effect(effect, &mut add, &mut delete, 0)?;
    }
    Ok(Action {
        name: action_name,
        parameters,
        precondition,
        add,
        delete,
    })
}

fn parse_effect(
    node: &Node,
    add: &mut Vec<Atom>,
    delete: &mut Vec<Atom>,
    depth: usize,
) -> Result<(), Error> {
    if depth > MAX_DEPTH {
        return Err(at(node, "maximum effect nesting depth exceeded"));
    }
    let fields = list(node, "effect")?;
    if fields.is_empty() {
        return Err(at(node, "empty effect"));
    }
    match atom(&fields[0])? {
        "and" => {
            for child in &fields[1..] {
                parse_effect(child, add, delete, depth + 1)?;
            }
        }
        "not" => {
            if fields.len() != 2 {
                return Err(at(node, "negative effect requires one atom"));
            }
            let atom = parse_atom(&fields[1])?;
            delete.push(atom);
        }
        "forall" | "when" | "increase" | "decrease" | "assign" => {
            return Err(at(
                node,
                format!("unsupported effect construct '{}'", atom(&fields[0])?),
            ));
        }
        _ => add.push(parse_atom(node)?),
    }
    Ok(())
}

fn parse_pddl_initial(node: &Node) -> Result<State, Error> {
    let mut state = BTreeSet::new();
    for fact in section_items(node, ":init")? {
        if head(fact).as_deref() == Some("not") {
            return Err(at(
                fact,
                "negative initial facts are not supported; use closed-world semantics",
            ));
        }
        let a = parse_atom(fact)?;
        let args = a
            .terms
            .into_iter()
            .map(|t| match t {
                Term::Constant(c) => Ok(c),
                Term::Variable(_) => Err(at(fact, "initial atoms cannot contain variables")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        state.insert(GroundAtom {
            predicate: a.predicate,
            arguments: args,
        });
    }
    Ok(state)
}

fn at(node: &Node, message: impl AsRef<str>) -> Error {
    Error::new(format!(
        "{}:{}: {}",
        node.line,
        node.column,
        message.as_ref()
    ))
}

fn token_error(token: &Token, message: impl AsRef<str>) -> Error {
    Error::new(format!(
        "{}:{}: {}",
        token.line,
        token.column,
        message.as_ref()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pddl_accepts_typed_groups_and_flat_quantifier_bindings() {
        let domain = "(define (domain d) (:types place - object)\
          (:constants depot - place) (:predicates (at ?x - place))\
          (:action mark :parameters (?x ?y - place)\
            :precondition (and) :effect (and (at ?x) (not (at ?y)))))";
        let problem = "(define (problem p) (:domain d) (:objects a b - place)\
          (:init (at a)) (:goal (forall (?x ?y - place) (or (at ?x) (not (at ?y))))))";
        let task = parse_pddl(domain, problem).unwrap();
        assert_eq!(task.objects.len(), 3);
        assert_eq!(task.actions[0].parameters.len(), 2);
        assert_eq!(task.actions[0].delete.len(), 1);
        let Formula::Forall(bindings, _) = task.goal else {
            panic!("expected forall")
        };
        assert_eq!(
            bindings.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["x", "y"]
        );
    }

    #[test]
    fn unsupported_and_ambiguous_syntax_is_rejected() {
        let domain = "(define (domain d) (:predicates) (:action a :effect (when (p) (q))))";
        let problem = "(define (problem p) (:domain d) (:init) (:goal (and)))";
        assert!(parse_pddl(domain, problem).is_err());
        let duplicate_params = "(define (domain d) (:predicates (p ?x ?x)))";
        assert!(parse_pddl(duplicate_params, problem).is_err());
    }
}
