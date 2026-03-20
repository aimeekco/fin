use std::error::Error;
use std::fmt;

use nom::IResult;
use nom::Parser;
use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::bytes::complete::take_while1;
use nom::character::complete::{char, digit1, multispace0, one_of, space0};
use nom::combinator::{all_consuming, map, map_res, recognize};
use nom::multi::many0;
use nom::number::complete::recognize_float;
use nom::sequence::{delimited, preceded, separated_pair};

use crate::model::{Layer, Modifier, PatternSource, Program, Symbol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl Error for ParseError {}

enum Statement {
    Bpm(f32),
    Layer(Layer),
}

pub fn parse_program(input: &str) -> Result<Program, ParseError> {
    let mut bpm = None;
    let mut layers = Vec::new();

    for (index, raw_line) in input.lines().enumerate() {
        let line_no = index + 1;
        let line = strip_comment(raw_line).trim();

        if line.is_empty() {
            continue;
        }

        let statement = parse_statement(line, line_no)?;
        match statement {
            Statement::Bpm(value) => bpm = Some(value),
            Statement::Layer(layer) => layers.push(layer),
        }
    }

    Ok(Program { bpm, layers })
}

fn strip_comment(line: &str) -> &str {
    match line.split_once('#') {
        Some((before, _)) => before,
        None => line,
    }
}

fn parse_statement(line: &str, line_no: usize) -> Result<Statement, ParseError> {
    if line.starts_with('[') {
        parse_layer_statement(line, line_no)
    } else {
        parse_assignment_statement(line, line_no)
    }
}

fn parse_assignment_statement(line: &str, line_no: usize) -> Result<Statement, ParseError> {
    let (_, (name, value)) = all_consuming(separated_pair(
        identifier,
        delimited(space0, char('='), space0),
        float_value,
    ))
    .parse(line)
    .map_err(|_| ParseError {
        line: line_no,
        message: "invalid assignment".to_string(),
    })?;

    if name != "bpm" {
        return Err(ParseError {
            line: line_no,
            message: format!("unsupported assignment `{name}`"),
        });
    }

    Ok(Statement::Bpm(value))
}

fn parse_layer_statement(line: &str, line_no: usize) -> Result<Statement, ParseError> {
    let (_, (name, modifiers)) =
        all_consuming((layer_header, many0(preceded(multispace0, modifier))))
            .parse(line)
            .map_err(|_| ParseError {
                line: line_no,
                message: "invalid layer statement".to_string(),
            })?;

    Ok(Statement::Layer(Layer {
        name: Symbol(name.to_string()),
        pattern: PatternSource::ImplicitSelf,
        modifiers,
        source_line: line_no,
    }))
}

fn identifier(input: &str) -> IResult<&str, &str> {
    recognize((
        take_while1(|c: char| c.is_ascii_alphabetic() || c == '_'),
        many0(one_of(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-",
        )),
    ))
    .parse(input)
}

fn layer_header(input: &str) -> IResult<&str, &str> {
    delimited(char('['), identifier, char(']')).parse(input)
}

fn float_value(input: &str) -> IResult<&str, f32> {
    map_res(recognize_float, str::parse::<f32>).parse(input)
}

fn divide_modifier(input: &str) -> IResult<&str, u32> {
    map_res(preceded(char('/'), digit1), |value: &str| {
        let parsed = value.parse::<u32>().map_err(|_| "invalid divide value")?;
        if parsed == 0 {
            return Err("divide must be greater than zero");
        }
        Ok::<u32, &'static str>(parsed)
    })
    .parse(input)
}

fn multiply_modifier(input: &str) -> IResult<&str, u32> {
    map_res(preceded(char('*'), digit1), |value: &str| {
        let parsed = value.parse::<u32>().map_err(|_| "invalid multiply value")?;
        if parsed == 0 {
            return Err("multiply must be greater than zero");
        }
        Ok::<u32, &'static str>(parsed)
    })
    .parse(input)
}

fn shift_right_modifier(input: &str) -> IResult<&str, f32> {
    preceded(tag(">>"), preceded(multispace0, float_value)).parse(input)
}

fn shift_left_modifier(input: &str) -> IResult<&str, f32> {
    map(
        preceded(tag("<<"), preceded(multispace0, float_value)),
        |value| -value,
    )
    .parse(input)
}

fn modifier(input: &str) -> IResult<&str, Modifier> {
    alt((
        map(divide_modifier, Modifier::Divide),
        map(multiply_modifier, Modifier::Multiply),
        map(shift_right_modifier, Modifier::Shift),
        map(shift_left_modifier, Modifier::Shift),
    ))
    .parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Modifier, PatternSource, Symbol};

    #[test]
    fn parses_bpm_assignment() {
        let program = parse_program("bpm = 128").expect("parse should succeed");
        assert_eq!(program.bpm, Some(128.0));
        assert!(program.layers.is_empty());
    }

    #[test]
    fn parses_single_layer() {
        let program = parse_program("[bd] /4").expect("parse should succeed");
        assert_eq!(program.layers.len(), 1);
        let layer = &program.layers[0];
        assert_eq!(layer.name, Symbol("bd".to_string()));
        assert_eq!(layer.pattern, PatternSource::ImplicitSelf);
        assert_eq!(layer.modifiers, vec![Modifier::Divide(4)]);
    }

    #[test]
    fn parses_density_and_shift_modifiers() {
        let program = parse_program("[hh] *8 >> 0.25").expect("parse should succeed");
        assert_eq!(
            program.layers[0].modifiers,
            vec![Modifier::Multiply(8), Modifier::Shift(0.25)]
        );
    }

    #[test]
    fn parses_left_shift_modifier() {
        let program = parse_program("[sd] /2 << 0.5").expect("parse should succeed");
        assert_eq!(
            program.layers[0].modifiers,
            vec![Modifier::Divide(2), Modifier::Shift(-0.5)]
        );
    }

    #[test]
    fn parses_multiple_lines_and_comments() {
        let source = "bpm = 128\n# comment\n[bd] /4 # kick\n[sd] /2";
        let program = parse_program(source).expect("parse should succeed");
        assert_eq!(program.bpm, Some(128.0));
        assert_eq!(program.layers.len(), 2);
    }

    #[test]
    fn rejects_unsupported_assignment() {
        let error = parse_program("swing = 0.1").expect_err("parse should fail");
        assert_eq!(error.line, 1);
        assert!(error.message.contains("unsupported assignment"));
    }

    #[test]
    fn rejects_divide_by_zero() {
        let error = parse_program("[bd] /0").expect_err("parse should fail");
        assert_eq!(error.line, 1);
    }

    #[test]
    fn rejects_multiply_by_zero() {
        let error = parse_program("[hh] *0").expect_err("parse should fail");
        assert_eq!(error.line, 1);
    }

    #[test]
    fn rejects_unknown_modifier_syntax() {
        let error = parse_program("[hh] .gain 0.6").expect_err("parse should fail");
        assert_eq!(error.line, 1);
    }
}
