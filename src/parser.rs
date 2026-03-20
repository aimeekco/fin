use std::error::Error;
use std::fmt;

use nom::IResult;
use nom::Parser;
use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::bytes::complete::{take_till1, take_while1};
use nom::character::complete::{char, digit1, multispace0, multispace1, one_of, space0};
use nom::combinator::{all_consuming, map, map_res, opt, recognize};
use nom::multi::{many0, separated_list1};
use nom::number::complete::recognize_float;
use nom::sequence::{delimited, preceded, separated_pair};

use crate::model::{
    Layer, Modifier, NoteValue, PatternAtom, PatternSource, Program, SoundTarget, Symbol,
};

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
    let (_, (default_target, pattern, modifiers)) = all_consuming((
        layer_header,
        opt(preceded(multispace0, pattern_source)),
        many0(preceded(multispace0, modifier)),
    ))
    .parse(line)
    .map_err(|_| ParseError {
        line: line_no,
        message: "invalid layer statement".to_string(),
    })?;

    let name = Symbol(default_target.display_name());

    Ok(Statement::Layer(Layer {
        name,
        default_target,
        pattern: pattern.unwrap_or(PatternSource::ImplicitSelf),
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

fn layer_header(input: &str) -> IResult<&str, SoundTarget> {
    delimited(char('['), sound_target, char(']')).parse(input)
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
        map(effect_modifier("gain"), Modifier::Gain),
        map(effect_modifier("pan"), Modifier::Pan),
        map(effect_modifier("speed"), Modifier::Speed),
        map(effect_modifier("sustain"), Modifier::Sustain),
    ))
    .parse(input)
}

fn effect_modifier<'a>(
    name: &'static str,
) -> impl Parser<&'a str, Output = f32, Error = nom::error::Error<&'a str>> {
    preceded(
        tag("."),
        preceded(tag(name), preceded(multispace0, float_value)),
    )
}

fn pattern_source(input: &str) -> IResult<&str, PatternSource> {
    alt((cycle_body, note_sequence_body, group_body, atom_body)).parse(input)
}

fn cycle_body(input: &str) -> IResult<&str, PatternSource> {
    map(
        delimited(
            char('<'),
            separated_list1(multispace1, pattern_atom),
            preceded(multispace0, char('>')),
        ),
        PatternSource::Cycle,
    )
    .parse(input)
}

fn group_body(input: &str) -> IResult<&str, PatternSource> {
    map(
        delimited(
            char('['),
            separated_list1(multispace1, pattern_atom),
            preceded(multispace0, char(']')),
        ),
        PatternSource::Group,
    )
    .parse(input)
}

fn note_sequence_body(input: &str) -> IResult<&str, PatternSource> {
    map(
        delimited(
            char('['),
            separated_list1(multispace1, note_value),
            preceded(multispace0, char(']')),
        ),
        PatternSource::NoteSequence,
    )
    .parse(input)
}

fn atom_body(input: &str) -> IResult<&str, PatternSource> {
    map(pattern_atom, PatternSource::Atom).parse(input)
}

fn pattern_atom(input: &str) -> IResult<&str, PatternAtom> {
    let (remaining, token) = token_value(input)?;

    if token.chars().all(|ch| ch.is_ascii_digit()) {
        let index = token.parse::<i32>().map_err(|_| {
            nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
        })?;
        return Ok((remaining, PatternAtom::SampleIndex(index)));
    }

    Ok((
        remaining,
        PatternAtom::Sound(parse_sound_target_token(token)?),
    ))
}

fn note_value(input: &str) -> IResult<&str, NoteValue> {
    let (remaining, token) = token_value(input)?;
    let note = parse_note_token(token)?;
    Ok((remaining, note))
}

fn sound_target(input: &str) -> IResult<&str, SoundTarget> {
    let (remaining, token) = token_value(input)?;
    Ok((remaining, parse_sound_target_token(token)?))
}

fn token_value(input: &str) -> IResult<&str, &str> {
    take_till1(|c: char| c.is_whitespace() || matches!(c, '[' | ']' | '<' | '>')).parse(input)
}

fn parse_sound_target_token(input: &str) -> Result<SoundTarget, nom::Err<nom::error::Error<&str>>> {
    let (name, index) = match input.split_once(':') {
        Some((name, index)) => (name, Some(index)),
        None => (input, None),
    };

    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Fail,
        )));
    }

    let index = match index {
        Some(index) => {
            if index.is_empty() || !index.chars().all(|ch| ch.is_ascii_digit()) {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Digit,
                )));
            }
            Some(index.parse::<i32>().map_err(|_| {
                nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
            })?)
        }
        None => None,
    };

    Ok(SoundTarget {
        name: name.to_string(),
        index,
    })
}

fn parse_note_token(input: &str) -> Result<NoteValue, nom::Err<nom::error::Error<&str>>> {
    let chars: Vec<char> = input.chars().collect();
    if chars.len() < 2 {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Fail,
        )));
    }

    let pitch_class = match chars[0].to_ascii_lowercase() {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Fail,
            )));
        }
    };

    let mut index = 1usize;
    let accidental = if let Some(accidental) = chars.get(index) {
        match accidental {
            '#' | 's' | 'S' => {
                index += 1;
                1
            }
            'b' | 'f' | 'F' => {
                index += 1;
                -1
            }
            _ => 0,
        }
    } else {
        0
    };

    let octave = input[index..].parse::<i32>().map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
    })?;
    let semitone = (pitch_class + accidental) as f32 + ((octave - 5) * 12) as f32;

    Ok(NoteValue {
        label: input.to_ascii_lowercase(),
        semitone,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Modifier, NoteValue, PatternAtom, PatternSource, SoundTarget, Symbol};

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
        assert_eq!(
            layer.default_target,
            SoundTarget {
                name: "bd".to_string(),
                index: None
            }
        );
        assert_eq!(layer.pattern, PatternSource::ImplicitSelf);
        assert_eq!(layer.modifiers, vec![Modifier::Divide(4)]);
    }

    #[test]
    fn parses_numeric_sample_name_in_header() {
        let program = parse_program("[808bd] /4").expect("parse should succeed");
        assert_eq!(program.layers[0].name, Symbol("808bd".to_string()));
    }

    #[test]
    fn parses_header_sample_index() {
        let program = parse_program("[bd:3] /4").expect("parse should succeed");
        assert_eq!(
            program.layers[0].default_target,
            SoundTarget {
                name: "bd".to_string(),
                index: Some(3)
            }
        );
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
    fn parses_effect_chain_modifiers() {
        let program = parse_program("[hh] *8 .gain 0.6 .pan -0.3 .speed 1.25 .sustain 0.4")
            .expect("parse should succeed");
        assert_eq!(
            program.layers[0].modifiers,
            vec![
                Modifier::Multiply(8),
                Modifier::Gain(0.6),
                Modifier::Pan(-0.3),
                Modifier::Speed(1.25),
                Modifier::Sustain(0.4),
            ]
        );
    }

    #[test]
    fn parses_cycle_pattern_body() {
        let program = parse_program("[bd] <0 3 5 7> /1").expect("parse should succeed");
        assert_eq!(
            program.layers[0].pattern,
            PatternSource::Cycle(vec![
                PatternAtom::SampleIndex(0),
                PatternAtom::SampleIndex(3),
                PatternAtom::SampleIndex(5),
                PatternAtom::SampleIndex(7),
            ])
        );
    }

    #[test]
    fn parses_group_pattern_body() {
        let program = parse_program("[drum] [bd sd:2] /1").expect("parse should succeed");
        assert_eq!(
            program.layers[0].pattern,
            PatternSource::Group(vec![
                PatternAtom::Sound(SoundTarget {
                    name: "bd".to_string(),
                    index: None,
                }),
                PatternAtom::Sound(SoundTarget {
                    name: "sd".to_string(),
                    index: Some(2),
                }),
            ])
        );
    }

    #[test]
    fn parses_note_sequence_body() {
        let program = parse_program("[bass] [g4 a4 a3 c3]").expect("parse should succeed");
        assert_eq!(
            program.layers[0].pattern,
            PatternSource::NoteSequence(vec![
                NoteValue {
                    label: "g4".to_string(),
                    semitone: -5.0,
                },
                NoteValue {
                    label: "a4".to_string(),
                    semitone: -3.0,
                },
                NoteValue {
                    label: "a3".to_string(),
                    semitone: -15.0,
                },
                NoteValue {
                    label: "c3".to_string(),
                    semitone: -24.0,
                },
            ])
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
        let error = parse_program("[hh] .room 0.6").expect_err("parse should fail");
        assert_eq!(error.line, 1);
    }
}
