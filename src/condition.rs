use crate::interpolate::interpolate;
use crate::safety::safe_path;
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::env;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Call { name: String, args: Vec<String> },
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

pub struct ConditionContext<'a> {
    pub project_root: &'a Path,
    pub values: &'a HashMap<String, String>,
    pub allow_outside_cwd: bool,
}

pub fn validate_syntax(condition: &str) -> Result<()> {
    let expr = parse(condition)?;
    validate_expr(&expr)
}

pub fn evaluate(condition: &str, context: &ConditionContext<'_>) -> Result<bool> {
    let expr = parse(condition)?;
    eval(&expr, context)
}

fn eval(expr: &Expr, context: &ConditionContext<'_>) -> Result<bool> {
    match expr {
        Expr::Not(inner) => Ok(!eval(inner, context)?),
        Expr::And(left, right) => Ok(eval(left, context)? && eval(right, context)?),
        Expr::Or(left, right) => Ok(eval(left, context)? || eval(right, context)?),
        Expr::Call { name, args } => eval_call(name, args, context),
    }
}

fn eval_call(name: &str, args: &[String], context: &ConditionContext<'_>) -> Result<bool> {
    match name {
        "os_is" => {
            expect_arity(name, args, 1)?;
            Ok(env::consts::OS == args[0])
        }
        "file_exists" => {
            expect_arity(name, args, 1)?;
            let path = condition_path(&args[0], context)?;
            Ok(path.is_file())
        }
        "dir_exists" => {
            expect_arity(name, args, 1)?;
            let path = condition_path(&args[0], context)?;
            Ok(path.is_dir())
        }
        "env_exists" => {
            expect_arity(name, args, 1)?;
            Ok(env::var_os(&args[0]).is_some())
        }
        "input_exists" => {
            expect_arity(name, args, 1)?;
            Ok(context
                .values
                .get(&args[0])
                .map(|value| !value.is_empty())
                .unwrap_or(false))
        }
        "input_equals" => {
            expect_arity(name, args, 2)?;
            Ok(context.values.get(&args[0]) == Some(&args[1]))
        }
        _ => bail!("unknown condition function '{name}'"),
    }
}

fn validate_expr(expr: &Expr) -> Result<()> {
    match expr {
        Expr::Not(inner) => validate_expr(inner),
        Expr::And(left, right) | Expr::Or(left, right) => {
            validate_expr(left)?;
            validate_expr(right)
        }
        Expr::Call { name, args } => validate_call(name, args),
    }
}

fn validate_call(name: &str, args: &[String]) -> Result<()> {
    match name {
        "os_is" | "file_exists" | "dir_exists" | "env_exists" | "input_exists" => {
            expect_arity(name, args, 1)
        }
        "input_equals" => expect_arity(name, args, 2),
        _ => bail!("unknown condition function '{name}'"),
    }
}

fn condition_path(raw: &str, context: &ConditionContext<'_>) -> Result<std::path::PathBuf> {
    let interpolated = interpolate(raw, context.values)?;
    safe_path(
        context.project_root,
        &interpolated,
        context.allow_outside_cwd,
    )
}

fn expect_arity(name: &str, args: &[String], expected: usize) -> Result<()> {
    if args.len() != expected {
        bail!(
            "condition function '{name}' expects {expected} argument(s), got {}",
            args.len()
        );
    }
    Ok(())
}

fn parse(input: &str) -> Result<Expr> {
    let mut parser = Parser::new(input);
    let expr = parser.parse_or()?;
    parser.skip_ws();
    if !parser.is_eof() {
        bail!("unexpected condition text near '{}'", parser.remaining());
    }
    Ok(expr)
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut expr = self.parse_and()?;
        loop {
            self.skip_ws();
            if !self.consume("||") {
                break;
            }
            let right = self.parse_and()?;
            expr = Expr::Or(Box::new(expr), Box::new(right));
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut expr = self.parse_not()?;
        loop {
            self.skip_ws();
            if !self.consume("&&") {
                break;
            }
            let right = self.parse_not()?;
            expr = Expr::And(Box::new(expr), Box::new(right));
        }
        Ok(expr)
    }

    fn parse_not(&mut self) -> Result<Expr> {
        self.skip_ws();
        if self.consume_keyword("not") {
            return Ok(Expr::Not(Box::new(self.parse_not()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        self.skip_ws();
        if self.consume("(") {
            let expr = self.parse_or()?;
            self.skip_ws();
            if !self.consume(")") {
                bail!("expected ')' in condition");
            }
            return Ok(expr);
        }
        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<Expr> {
        let name = self.parse_identifier()?;
        self.skip_ws();
        if !self.consume("(") {
            bail!("expected '(' after condition function '{name}'");
        }

        let mut args = Vec::new();
        loop {
            self.skip_ws();
            if self.consume(")") {
                break;
            }
            args.push(self.parse_string()?);
            self.skip_ws();
            if self.consume(")") {
                break;
            }
            if !self.consume(",") {
                bail!("expected ',' or ')' in condition function '{name}'");
            }
        }

        Ok(Expr::Call { name, args })
    }

    fn parse_identifier(&mut self) -> Result<String> {
        self.skip_ws();
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        if self.pos == start {
            bail!("expected condition function");
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_string(&mut self) -> Result<String> {
        self.skip_ws();
        let quote = match self.peek_char() {
            Some('"') => '"',
            Some('\'') => '\'',
            _ => bail!("expected quoted string condition argument"),
        };
        self.pos += quote.len_utf8();
        let mut output = String::new();
        while let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
            if ch == quote {
                return Ok(output);
            }
            if ch == '\\' {
                let Some(escaped) = self.peek_char() else {
                    bail!("unterminated escape in condition string");
                };
                self.pos += escaped.len_utf8();
                output.push(escaped);
            } else {
                output.push(ch);
            }
        }
        bail!("unterminated condition string")
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        self.skip_ws();
        if !self.input[self.pos..].starts_with(keyword) {
            return false;
        }
        let end = self.pos + keyword.len();
        if self.input[end..]
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return false;
        }
        self.pos = end;
        true
    }

    fn consume(&mut self, text: &str) -> bool {
        self.skip_ws();
        if self.input[self.pos..].starts_with(text) {
            self.pos += text.len();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn evaluates_file_and_dir_conditions() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("config")).unwrap();
        fs::write(temp.path().join("config/app.toml"), "ok").unwrap();
        let values = HashMap::new();
        let context = ConditionContext {
            project_root: temp.path(),
            values: &values,
            allow_outside_cwd: false,
        };

        assert!(evaluate("file_exists('config/app.toml')", &context).unwrap());
        assert!(evaluate("dir_exists(\"config\")", &context).unwrap());
        assert!(!evaluate("file_exists('missing')", &context).unwrap());
    }

    #[test]
    fn evaluates_inputs_and_boolean_operators() {
        let temp = tempdir().unwrap();
        let values = HashMap::from([("target".to_string(), "preview".to_string())]);
        let context = ConditionContext {
            project_root: temp.path(),
            values: &values,
            allow_outside_cwd: false,
        };

        assert!(evaluate("input_exists('target')", &context).unwrap());
        assert!(evaluate("input_equals('target', 'preview')", &context).unwrap());
        assert!(evaluate(
            "input_equals('target', 'preview') && not input_equals('target', 'prod')",
            &context
        )
        .unwrap());
        assert!(!evaluate(
            "input_equals('target', 'prod') || input_equals('missing', 'x')",
            &context
        )
        .unwrap());
    }

    #[test]
    fn rejects_path_traversal_conditions() {
        let temp = tempdir().unwrap();
        let values = HashMap::new();
        let context = ConditionContext {
            project_root: temp.path(),
            values: &values,
            allow_outside_cwd: false,
        };

        assert!(evaluate("file_exists('../secret')", &context).is_err());
    }

    #[test]
    fn validates_syntax() {
        assert!(validate_syntax("os_is('linux') || os_is('macos')").is_ok());
        assert!(validate_syntax("unknown").is_err());
        assert!(validate_syntax("file_exists(config)").is_err());
    }
}
