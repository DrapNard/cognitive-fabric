//! Bounded expression parser. No eval, subprocesses or arbitrary code execution.
use anyhow::{anyhow, bail, Result};

pub fn extract(query: &str) -> Option<&str> {
    let (prefix, expression) = query.split_once(':')?;
    if !["calc", "calculate", "math"].contains(&prefix.trim().to_ascii_lowercase().as_str()) {
        return None;
    }
    let expression = expression.trim();
    if expression.is_empty() || expression.len() > 120 {
        return None;
    }
    Some(expression)
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }
    fn consume(&mut self, op: u8) -> bool {
        self.whitespace();
        if self.input.get(self.pos) == Some(&op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn number(&mut self) -> Result<f64> {
        self.whitespace();
        let start = self.pos;
        while self.pos < self.input.len()
            && (self.input[self.pos].is_ascii_digit() || self.input[self.pos] == b'.')
        {
            self.pos += 1;
        }
        if start == self.pos {
            bail!("Expected number")
        }
        let n: f64 = std::str::from_utf8(&self.input[start..self.pos])?.parse()?;
        if !n.is_finite() || n.abs() > 1e12 {
            bail!("Number exceeds limit")
        }
        Ok(n)
    }
    fn primary(&mut self, depth: usize) -> Result<f64> {
        if depth > 12 || self.nodes >= 35 {
            bail!("Expression complexity limit exceeded")
        }
        self.nodes += 1;
        if self.consume(b'+') {
            return self.primary(depth + 1);
        }
        if self.consume(b'-') {
            return Ok(-self.primary(depth + 1)?);
        }
        if self.consume(b'(') {
            let v = self.expression(0, depth + 1)?;
            if !self.consume(b')') {
                bail!("Missing closing parenthesis")
            }
            return Ok(v);
        }
        self.number()
    }
    fn expression(&mut self, min_power: u8, depth: usize) -> Result<f64> {
        let mut lhs = self.primary(depth)?;
        loop {
            self.whitespace();
            let Some(&op) = self.input.get(self.pos) else {
                break;
            };
            let (left, right) = match op {
                b'+' | b'-' => (1, 2),
                b'*' | b'/' | b'%' => (3, 4),
                b'^' => (7, 7),
                _ => break,
            };
            if left < min_power {
                break;
            }
            self.pos += 1;
            let floor_division = op == b'/' && self.input.get(self.pos) == Some(&b'/');
            let exponent = op == b'*' && self.input.get(self.pos) == Some(&b'*');
            if floor_division || exponent {
                self.pos += 1;
            }
            let rhs = self.expression(if exponent { 7 } else { right }, depth + 1)?;
            if (op == b'/' || op == b'%') && rhs == 0.0 {
                bail!("Division by zero")
            }
            lhs = match op {
                b'+' => lhs + rhs,
                b'-' => lhs - rhs,
                b'*' if exponent => {
                    if rhs.abs() > 8.0 || lhs.abs() > 1e6 {
                        bail!("Exponentiation limit exceeded")
                    }
                    lhs.powf(rhs)
                }
                b'*' => lhs * rhs,
                b'/' if floor_division => (lhs / rhs).floor(),
                b'/' => lhs / rhs,
                b'%' => lhs % rhs,
                b'^' => {
                    if rhs.abs() > 8.0 || lhs.abs() > 1e6 {
                        bail!("Exponentiation limit exceeded")
                    }
                    lhs.powf(rhs)
                }
                _ => return Err(anyhow!("Unsupported operator")),
            };
            if !lhs.is_finite() || lhs.abs() > 1e15 {
                bail!("Result exceeds limit")
            }
        }
        Ok(lhs)
    }
}

pub fn calculate(input: &str) -> Result<String> {
    if input.is_empty() || input.len() > 120 {
        bail!("Expression length limit exceeded")
    }
    let mut parser = Parser {
        input: input.as_bytes(),
        pos: 0,
        nodes: 0,
    };
    let n = parser.expression(0, 0)?;
    parser.whitespace();
    if parser.pos != parser.input.len() {
        bail!("Unexpected trailing characters")
    }
    if n.fract() == 0.0 {
        Ok(format!("{n:.0}"))
    } else {
        Ok(format!("{n}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expression() {
        assert_eq!(calculate("(7 + 5) * 3").unwrap(), "36");
    }
    #[test]
    fn malicious_input() {
        assert!(calculate("__import__('os')").is_err());
    }
    #[test]
    fn divide_by_zero() {
        assert!(calculate("1/0").is_err());
    }
    #[test]
    fn exponent_bounded() {
        assert!(calculate("2**500").is_err());
    }
}
