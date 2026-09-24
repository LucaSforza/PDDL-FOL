//! Parser for the ASCII infix FOLPlan language.

use crate::model::{
    Action, Atom, Binding, Error, Formula, GroundAtom, Predicate, State, Task, Term,
};
use std::collections::BTreeSet;

const MAX_DEPTH: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    line: usize,
    column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TokenKind {
    Name(String),
    Symbol(Symbol),
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    LBrace,
    RBrace,
    LParen,
    RParen,
    Comma,
    Colon,
    Semi,
    Dot,
    And,
    Or,
    Not,
    Implies,
    Equal,
    NotEqual,
}

/// Parse a FOLPlan task written in the brace-based infix syntax.
pub fn parse_dsl(source: &str) -> Result<Task, Error> {
    let tokens = lex(source)?;
    Parser {
        tokens,
        pos: 0,
        depth: 0,
    }
    .parse_task()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn parse_task(mut self) -> Result<Task, Error> {
        self.expect_name("problem")?;
        let name = self.read_name()?;
        self.expect_symbol(Symbol::LBrace)?;

        let mut types = None;
        let mut objects = None;
        let mut predicates = None;
        let mut initial = None;
        let mut goal = None;
        let mut actions = Vec::new();

        while !self.symbol(Symbol::RBrace) {
            if self.is_end() {
                return self.error_here("unclosed problem block");
            }
            match self.peek_name() {
                Some("types") => {
                    self.advance();
                    if types.is_some() {
                        return self.error_previous("duplicate types section");
                    }
                    types = Some(self.parse_types()?);
                }
                Some("objects") => {
                    self.advance();
                    if objects.is_some() {
                        return self.error_previous("duplicate objects section");
                    }
                    objects = Some(self.parse_objects()?);
                }
                Some("predicates") => {
                    self.advance();
                    if predicates.is_some() {
                        return self.error_previous("duplicate predicates section");
                    }
                    predicates = Some(self.parse_predicates()?);
                }
                Some("init") => {
                    self.advance();
                    if initial.is_some() {
                        return self.error_previous("duplicate init section");
                    }
                    self.expect_symbol(Symbol::Colon)?;
                    initial = Some(self.parse_initial()?);
                    self.expect_symbol(Symbol::Semi)?;
                }
                Some("goal") => {
                    self.advance();
                    if goal.is_some() {
                        return self.error_previous("duplicate goal section");
                    }
                    self.expect_symbol(Symbol::Colon)?;
                    goal = Some(self.formula(0, &mut Vec::new())?);
                    self.expect_symbol(Symbol::Semi)?;
                }
                Some("action") => {
                    self.advance();
                    actions.push(self.parse_action()?);
                }
                Some(other) => return self.error_here(format!("unknown section '{other}'")),
                None => return self.error_here("expected a section"),
            }
        }
        self.advance();
        if !self.is_end() {
            return self.error_here("unexpected trailing input");
        }

        Ok(Task {
            name,
            types: types.ok_or_else(|| self.err_last("missing required types section"))?,
            objects: objects.ok_or_else(|| self.err_last("missing required objects section"))?,
            predicates: predicates
                .ok_or_else(|| self.err_last("missing required predicates section"))?,
            actions,
            initial: initial.ok_or_else(|| self.err_last("missing required init section"))?,
            goal: goal.ok_or_else(|| self.err_last("missing required goal section"))?,
        })
    }

    fn parse_types(&mut self) -> Result<Vec<String>, Error> {
        let mut types = Vec::new();
        if self.symbol(Symbol::Semi) {
            self.advance();
            return Ok(types);
        }
        loop {
            types.push(self.read_name()?);
            if self.symbol(Symbol::Comma) {
                self.advance();
                continue;
            }
            self.expect_symbol(Symbol::Semi)?;
            return Ok(types);
        }
    }

    fn parse_objects(&mut self) -> Result<Vec<Binding>, Error> {
        self.expect_symbol(Symbol::LBrace)?;
        let mut result = Vec::new();
        while !self.symbol(Symbol::RBrace) {
            if self.is_end() {
                return self.error_here("unclosed objects block");
            }
            let mut names = vec![self.read_name()?];
            while self.symbol(Symbol::Comma) {
                self.advance();
                names.push(self.read_name()?);
            }
            self.expect_symbol(Symbol::Colon)?;
            let ty = self.read_name()?;
            result.extend(names.into_iter().map(|name| Binding {
                name,
                ty: ty.clone(),
            }));
            self.expect_symbol(Symbol::Semi)?;
        }
        self.advance();
        Ok(result)
    }

    fn parse_predicates(&mut self) -> Result<Vec<Predicate>, Error> {
        self.expect_symbol(Symbol::LBrace)?;
        let mut result = Vec::new();
        while !self.symbol(Symbol::RBrace) {
            if self.is_end() {
                return self.error_here("unclosed predicates block");
            }
            let name = self.read_name()?;
            self.expect_symbol(Symbol::LParen)?;
            let mut parameters = Vec::new();
            if !self.symbol(Symbol::RParen) {
                loop {
                    parameters.push(self.read_name()?);
                    if self.symbol(Symbol::Comma) {
                        self.advance();
                        continue;
                    }
                    break;
                }
            }
            self.expect_symbol(Symbol::RParen)?;
            self.expect_symbol(Symbol::Semi)?;
            result.push(Predicate { name, parameters });
        }
        self.advance();
        Ok(result)
    }

    fn parse_initial(&mut self) -> Result<State, Error> {
        let mut facts = BTreeSet::new();
        let formula = self.formula(0, &mut Vec::new())?;
        self.collect_initial(&formula, &mut facts)?;
        Ok(facts)
    }

    fn collect_initial(&self, formula: &Formula, state: &mut State) -> Result<(), Error> {
        match formula {
            Formula::And(parts) => {
                for part in parts {
                    self.collect_initial(part, state)?;
                }
            }
            Formula::Atom(atom) => {
                let arguments = atom
                    .terms
                    .iter()
                    .map(|term| match term {
                        Term::Constant(name) => Ok(name.clone()),
                        Term::Variable(_) => {
                            Err(self.err_last("initial facts cannot contain variables"))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                state.insert(GroundAtom {
                    predicate: atom.predicate.clone(),
                    arguments,
                });
            }
            _ => {
                return Err(
                    self.err_last("init supports only positive atoms joined by conjunction")
                );
            }
        }
        Ok(())
    }

    fn parse_action(&mut self) -> Result<Action, Error> {
        let name = self.read_name()?;
        self.expect_symbol(Symbol::LParen)?;
        let mut parameters = Vec::new();
        let mut scope = Vec::new();
        if !self.symbol(Symbol::RParen) {
            loop {
                let variable = self.read_name()?;
                self.expect_symbol(Symbol::Colon)?;
                let ty = self.read_name()?;
                scope.push(variable.clone());
                parameters.push(Binding { name: variable, ty });
                if self.symbol(Symbol::Comma) {
                    self.advance();
                    continue;
                }
                break;
            }
        }
        self.expect_symbol(Symbol::RParen)?;
        self.expect_symbol(Symbol::LBrace)?;
        let mut precondition = None;
        let mut effect = None;
        while !self.symbol(Symbol::RBrace) {
            if self.is_end() {
                return self.error_here("unclosed action block");
            }
            let field = self.read_name()?;
            self.expect_symbol(Symbol::Colon)?;
            match field.as_str() {
                "pre" => {
                    if precondition.is_some() {
                        return self.error_previous("duplicate action pre section");
                    }
                    precondition = Some(self.formula(0, &mut scope)?);
                }
                "effect" => {
                    if effect.is_some() {
                        return self.error_previous("duplicate action effect section");
                    }
                    let parsed = self.formula(0, &mut scope)?;
                    let mut additions = Vec::new();
                    let mut deletions = Vec::new();
                    self.collect_effect(&parsed, &mut additions, &mut deletions)?;
                    effect = Some((additions, deletions));
                }
                _ => return self.error_previous(format!("unknown action field '{field}'")),
            }
            self.expect_symbol(Symbol::Semi)?;
        }
        self.advance();
        Ok(Action {
            name,
            parameters,
            precondition: precondition.ok_or_else(|| self.err_last("action is missing pre"))?,
            add: effect
                .as_ref()
                .ok_or_else(|| self.err_last("action is missing effect"))?
                .0
                .clone(),
            delete: effect
                .ok_or_else(|| self.err_last("action is missing effect"))?
                .1,
        })
    }

    fn collect_effect(
        &self,
        formula: &Formula,
        add: &mut Vec<Atom>,
        delete: &mut Vec<Atom>,
    ) -> Result<(), Error> {
        match formula {
            Formula::And(parts) => {
                for part in parts {
                    self.collect_effect(part, add, delete)?;
                }
            }
            Formula::Atom(atom) => add.push(atom.clone()),
            Formula::Not(inner) => match inner.as_ref() {
                Formula::Atom(atom) => delete.push(atom.clone()),
                _ => return Err(self.err_last("effects may negate atoms only")),
            },
            _ => {
                return Err(
                    self.err_last("effects support only atoms, negated atoms, and conjunctions")
                );
            }
        }
        Ok(())
    }

    fn formula(&mut self, min_precedence: u8, scope: &mut Vec<String>) -> Result<Formula, Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.depth -= 1;
            return self.error_here("maximum formula nesting depth exceeded");
        }
        let mut left = self.prefix(scope)?;
        loop {
            let Some((precedence, symbol, right_assoc)) = self.infix() else {
                break;
            };
            if precedence < min_precedence {
                break;
            }
            self.advance();
            let rhs = self.formula(precedence + if right_assoc { 0 } else { 1 }, scope)?;
            left = match symbol {
                Symbol::And => merge_and(left, rhs),
                Symbol::Or => merge_or(left, rhs),
                Symbol::Implies => Formula::Implies(Box::new(left), Box::new(rhs)),
                _ => unreachable!(),
            };
        }
        self.depth -= 1;
        Ok(left)
    }

    fn prefix(&mut self, scope: &mut Vec<String>) -> Result<Formula, Error> {
        if self.symbol(Symbol::Not) {
            self.advance();
            return Ok(Formula::Not(Box::new(self.formula(4, scope)?)));
        }
        if self.symbol(Symbol::LParen) {
            self.advance();
            let formula = self.formula(0, scope)?;
            self.expect_symbol(Symbol::RParen)?;
            return Ok(formula);
        }
        match self.peek_name() {
            Some("true") => {
                self.advance();
                Ok(Formula::And(Vec::new()))
            }
            Some("false") => {
                self.advance();
                Ok(Formula::Or(Vec::new()))
            }
            Some("forall") | Some("exists") => self.quantified(scope),
            Some(_) => self.application_or_equality(scope),
            None => self.error_here("expected a formula"),
        }
    }

    fn quantified(&mut self, scope: &mut Vec<String>) -> Result<Formula, Error> {
        let is_forall = self.peek_name() == Some("forall");
        self.advance();
        let old_len = scope.len();
        let mut bindings = Vec::new();
        loop {
            let name = self.read_name()?;
            self.expect_symbol(Symbol::Colon)?;
            let ty = self.read_name()?;
            bindings.push(Binding {
                name: name.clone(),
                ty,
            });
            scope.push(name);
            if self.symbol(Symbol::Comma) {
                self.advance();
                continue;
            }
            break;
        }
        self.expect_symbol(Symbol::Dot)?;
        let body = self.formula(0, scope)?;
        scope.truncate(old_len);
        Ok(if is_forall {
            Formula::Forall(bindings, Box::new(body))
        } else {
            Formula::Exists(bindings, Box::new(body))
        })
    }

    fn application_or_equality(&mut self, scope: &[String]) -> Result<Formula, Error> {
        let first = self.read_name()?;
        let left = self.term(first.clone(), scope);
        if self.symbol(Symbol::Equal) || self.symbol(Symbol::NotEqual) {
            let unequal = self.symbol(Symbol::NotEqual);
            self.advance();
            let right_name = self.read_name()?;
            let right = self.term(right_name, scope);
            let equality = Formula::Equal(left, right);
            return Ok(if unequal {
                Formula::Not(Box::new(equality))
            } else {
                equality
            });
        }
        self.expect_symbol(Symbol::LParen)?;
        let mut terms = Vec::new();
        if !self.symbol(Symbol::RParen) {
            loop {
                let name = self.read_name()?;
                terms.push(self.term(name, scope));
                if self.symbol(Symbol::Comma) {
                    self.advance();
                    continue;
                }
                break;
            }
        }
        self.expect_symbol(Symbol::RParen)?;
        Ok(Formula::Atom(Atom {
            predicate: first,
            terms,
        }))
    }

    fn term(&self, name: String, scope: &[String]) -> Term {
        if scope.iter().rev().any(|bound| bound == &name) {
            Term::Variable(name)
        } else {
            Term::Constant(name)
        }
    }

    fn infix(&self) -> Option<(u8, Symbol, bool)> {
        let TokenKind::Symbol(symbol) = self.current().kind else {
            return None;
        };
        Some(match symbol {
            Symbol::Implies => (1, symbol, true),
            Symbol::Or => (2, symbol, false),
            Symbol::And => (3, symbol, false),
            _ => return None,
        })
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }
    fn is_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::End)
    }
    fn peek_name(&self) -> Option<&str> {
        if let TokenKind::Name(ref s) = self.current().kind {
            Some(s)
        } else {
            None
        }
    }
    fn symbol(&self, symbol: Symbol) -> bool {
        self.current().kind == TokenKind::Symbol(symbol)
    }
    fn advance(&mut self) {
        if !self.is_end() {
            self.pos += 1;
        }
    }

    fn read_name(&mut self) -> Result<String, Error> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Name(name) if !is_reserved(&name) => {
                self.advance();
                Ok(name)
            }
            TokenKind::Name(ref name) => Err(error(
                &token,
                format!("reserved word '{name}' cannot be used as a name"),
            )),
            _ => Err(error(&token, "expected an identifier")),
        }
    }

    fn expect_name(&mut self, expected: &str) -> Result<(), Error> {
        if self.peek_name() == Some(expected) {
            self.advance();
            Ok(())
        } else {
            self.error_here(format!("expected '{expected}'"))
        }
    }

    fn expect_symbol(&mut self, expected: Symbol) -> Result<(), Error> {
        if self.symbol(expected) {
            self.advance();
            Ok(())
        } else {
            self.error_here(format!("expected {}", symbol_name(expected)))
        }
    }

    fn error_here<T>(&self, message: impl AsRef<str>) -> Result<T, Error> {
        Err(error(self.current(), message))
    }
    fn error_previous<T>(&self, message: impl AsRef<str>) -> Result<T, Error> {
        Err(error(&self.tokens[self.pos.saturating_sub(1)], message))
    }
    fn err_last(&self, message: impl AsRef<str>) -> Error {
        error(&self.tokens[self.pos.saturating_sub(1)], message)
    }
}

fn merge_and(left: Formula, right: Formula) -> Formula {
    let mut parts = match left {
        Formula::And(parts) => parts,
        other => vec![other],
    };
    match right {
        Formula::And(mut parts2) => parts.append(&mut parts2),
        other => parts.push(other),
    }
    Formula::And(parts)
}

fn merge_or(left: Formula, right: Formula) -> Formula {
    let mut parts = match left {
        Formula::Or(parts) => parts,
        other => vec![other],
    };
    match right {
        Formula::Or(mut parts2) => parts.append(&mut parts2),
        other => parts.push(other),
    }
    Formula::Or(parts)
}

fn lex(source: &str) -> Result<Vec<Token>, Error> {
    let mut result = Vec::new();
    let mut chars = source.chars().peekable();
    let (mut line, mut column) = (1usize, 1usize);
    while let Some(ch) = chars.next() {
        if !ch.is_ascii() {
            return Err(Error::new(format!(
                "{line}:{column}: non-ASCII character '{ch}'"
            )));
        }
        if ch == '\n' {
            line += 1;
            column = 1;
            continue;
        }
        if ch.is_whitespace() {
            column += 1;
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            column += 2;
            for c in chars.by_ref() {
                if !c.is_ascii() {
                    return Err(Error::new(format!(
                        "{line}:{column}: non-ASCII character '{c}'"
                    )));
                }
                if c == '\n' {
                    line += 1;
                    column = 1;
                    break;
                }
                column += 1;
            }
            continue;
        }
        let start = (line, column);
        if ch.is_ascii_alphabetic() {
            let mut word = String::from(ch.to_ascii_lowercase());
            while let Some(next) = chars.peek().copied() {
                if next == '-' && chars.clone().nth(1) == Some('>') {
                    break;
                }
                if !next.is_ascii_alphanumeric() && next != '_' && next != '-' {
                    break;
                }
                chars.next();
                word.push(next.to_ascii_lowercase());
                column += 1;
            }
            let kind = match word.as_str() {
                "and" => TokenKind::Symbol(Symbol::And),
                "or" => TokenKind::Symbol(Symbol::Or),
                "not" => TokenKind::Symbol(Symbol::Not),
                "implies" => TokenKind::Symbol(Symbol::Implies),
                _ => TokenKind::Name(word),
            };
            result.push(Token {
                kind,
                line,
                column: start.1,
            });
            column += 1;
            continue;
        }
        let kind = match ch {
            '{' => Some(TokenKind::Symbol(Symbol::LBrace)),
            '}' => Some(TokenKind::Symbol(Symbol::RBrace)),
            '(' => Some(TokenKind::Symbol(Symbol::LParen)),
            ')' => Some(TokenKind::Symbol(Symbol::RParen)),
            ',' => Some(TokenKind::Symbol(Symbol::Comma)),
            ':' => Some(TokenKind::Symbol(Symbol::Colon)),
            ';' => Some(TokenKind::Symbol(Symbol::Semi)),
            '.' => Some(TokenKind::Symbol(Symbol::Dot)),
            '&' => {
                if chars.peek() == Some(&'&') {
                    chars.next();
                    column += 1;
                }
                Some(TokenKind::Symbol(Symbol::And))
            }
            '|' => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                    column += 1;
                }
                Some(TokenKind::Symbol(Symbol::Or))
            }
            '!' => {
                if chars.peek() == Some(&'=') {
                    chars.next();
                    column += 1;
                    Some(TokenKind::Symbol(Symbol::NotEqual))
                } else {
                    Some(TokenKind::Symbol(Symbol::Not))
                }
            }
            '=' => Some(TokenKind::Symbol(Symbol::Equal)),
            '-' if chars.peek() == Some(&'>') => {
                chars.next();
                column += 1;
                Some(TokenKind::Symbol(Symbol::Implies))
            }
            _ => None,
        };
        if let Some(kind) = kind {
            result.push(Token {
                kind,
                line,
                column: start.1,
            });
            column += 1;
        } else {
            return Err(Error::new(format!(
                "{}:{}: unexpected character '{ch}'",
                line, column
            )));
        }
    }
    result.push(Token {
        kind: TokenKind::End,
        line,
        column,
    });
    Ok(result)
}

fn is_reserved(name: &str) -> bool {
    matches!(name, "true" | "false" | "forall" | "exists")
}

fn symbol_name(symbol: Symbol) -> &'static str {
    match symbol {
        Symbol::LBrace => "'{'",
        Symbol::RBrace => "'}'",
        Symbol::LParen => "'('",
        Symbol::RParen => "')'",
        Symbol::Comma => "','",
        Symbol::Colon => "':'",
        Symbol::Semi => "';'",
        Symbol::Dot => "'.'",
        Symbol::And => "conjunction",
        Symbol::Or => "disjunction",
        Symbol::Not => "negation",
        Symbol::Implies => "implication",
        Symbol::Equal => "'='",
        Symbol::NotEqual => "'!='",
    }
}

fn error(token: &Token, message: impl AsRef<str>) -> Error {
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

    const SAMPLE: &str = r#"
        problem demo {
            types block, surface;
            objects { a, b, c: block; table: surface; }
            predicates { On(block, object); Ready(); }
            init: On(b, table) and On(a, table) and On(c, a);
            goal: On(a, b) and On(b, c) and On(c, table);
            action Move(b: block, from: object, to: object) {
                pre: On(b, from) and b != to and from != to and
                    (forall z: block . not On(z, b)) and
                    (to != table implies (forall z: block . not On(z, to)));
                effect: not On(b, from) and On(b, to);
            }
        }
    "#;

    #[test]
    fn parses_infix_sections_bindings_quantifiers_and_effects() {
        let task = parse_dsl(SAMPLE).unwrap();
        assert_eq!(task.name, "demo");
        assert_eq!(task.types, ["block", "surface"]);
        assert_eq!(task.objects.len(), 4);
        assert_eq!(task.initial.len(), 3);
        assert_eq!(task.actions[0].parameters[0].name, "b");
        assert_eq!(task.actions[0].delete.len(), 1);
        assert_eq!(task.actions[0].add.len(), 1);
        let Formula::And(preconditions) = &task.actions[0].precondition else {
            panic!("conjunction expected")
        };
        assert!(
            preconditions
                .iter()
                .any(|f| matches!(f, Formula::Forall(_, _)))
        );
        assert!(
            preconditions
                .iter()
                .any(|f| matches!(f, Formula::Implies(_, _)))
        );
        let Formula::And(goals) = &task.goal else {
            panic!("conjunction expected")
        };
        assert_eq!(goals.len(), 3);
    }

    #[test]
    fn word_operators_and_arrow_without_spaces_work() {
        let source = r#"problem logic {
            types;
            objects {}
            predicates { P(); Q(); }
            init: true;
            goal: forall x: object . (P() and not Q()) implies (P() or Q());
        }"#;
        let task = parse_dsl(source).unwrap();
        assert!(matches!(task.goal, Formula::Forall(_, _)));
        let tokens = lex("a->b").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Name("a".into()));
        assert_eq!(tokens[1].kind, TokenKind::Symbol(Symbol::Implies));
    }

    #[test]
    fn rejects_bad_sections_formula_forms_and_unsupported_effects() {
        assert!(parse_dsl("problem x { types; objects {} predicates {}; init: true; }").is_err());
        assert!(
            parse_dsl(
                "problem x { types; types; objects {} predicates {}; init: true; goal: true; }"
            )
            .is_err()
        );
        assert!(
            parse_dsl(
                "problem x { types; objects {} predicates { true(); } init: true; goal: true; }"
            )
            .is_err()
        );
        let source = SAMPLE.replace(
            "effect: not On(b, from) and On(b, to);",
            "effect: On(b, from) | On(b, to);",
        );
        assert!(parse_dsl(&source).is_err());
        let source = SAMPLE.replace(
            "init: On(b, table) and On(a, table) and On(c, a);",
            "init: !On(b, table);",
        );
        assert!(parse_dsl(&source).is_err());
    }
}
