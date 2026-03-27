use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use nom::IResult;
use nom::Parser;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_till1, take_until, take_while1};
use nom::character::complete::{char, digit1, multispace0, multispace1, one_of, space0};
use nom::combinator::{all_consuming, map, map_res, opt, recognize, verify};
use nom::multi::{many0, separated_list1};
use nom::number::complete::recognize_float;
use nom::sequence::{delimited, preceded};

use crate::model::{
    BarPattern, BarSelector, Layer, Modifier, NoteValue, PatternAtom, PatternSource, PatternValue,
    Program, SoundTarget, Symbol, TempoChange,
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

pub fn parse_program(input: &str) -> Result<Program, ParseError> {
    let mut bpm = None;
    let mut tempo_changes = BTreeMap::new();
    let mut bars = None;
    let mut layers = Vec::new();
    let mut current_layer: Option<Layer> = None;

    for (index, raw_line) in input.lines().enumerate() {
        let line_no = index + 1;
        let without_comment = strip_comment(raw_line);
        let trimmed = without_comment.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }

        let indent = trimmed.chars().take_while(|ch| ch.is_whitespace()).count();
        let line = trimmed.trim_start();

        if indent == 0 {
            if let Some(layer) = current_layer.take() {
                layers.push(layer);
            }

            if line.starts_with("[bar") {
                return Err(ParseError {
                    line: line_no,
                    message: "bar definition must be indented under a layer".to_string(),
                });
            }

            if line.starts_with('[') {
                current_layer = Some(parse_layer_statement(line, line_no)?);
                continue;
            }

            let assignment = parse_assignment_statement(line, line_no)?;
            match (assignment.name, assignment.selector) {
                ("bpm", None) => {
                    if bpm.is_some() {
                        return Err(ParseError {
                            line: line_no,
                            message: "duplicate `bpm` assignment".to_string(),
                        });
                    }
                    bpm = Some(assignment.value);
                }
                ("bpm", Some(BarSelector::Default)) => {
                    return Err(ParseError {
                        line: line_no,
                        message: "`bpm [default]` is not supported; use `bpm = <number>`"
                            .to_string(),
                    });
                }
                ("bpm", Some(selector)) => {
                    if tempo_changes.contains_key(&selector) {
                        return Err(ParseError {
                            line: line_no,
                            message: format!(
                                "duplicate `bpm {}` assignment",
                                selector.header_label()
                            ),
                        });
                    }
                    tempo_changes.insert(
                        selector,
                        TempoChange {
                            bpm: assignment.value,
                            source_line: line_no,
                        },
                    );
                }
                ("bars", None) => {
                    if bars.is_some() {
                        return Err(ParseError {
                            line: line_no,
                            message: "duplicate `bars` assignment".to_string(),
                        });
                    }
                    if assignment.value.fract() != 0.0 || assignment.value <= 0.0 {
                        return Err(ParseError {
                            line: line_no,
                            message: "`bars` must be a positive integer".to_string(),
                        });
                    }
                    bars = Some(assignment.value as u32);
                }
                ("bars", Some(_)) => {
                    return Err(ParseError {
                        line: line_no,
                        message: "`bars` does not support scoped assignments".to_string(),
                    });
                }
                (other, _) => {
                    return Err(ParseError {
                        line: line_no,
                        message: format!("unsupported assignment `{other}`"),
                    });
                }
            }
        } else {
            let layer = current_layer.as_mut().ok_or_else(|| ParseError {
                line: line_no,
                message: "indented content must follow a layer".to_string(),
            })?;
            let (bar_selector, bar_pattern) = parse_bar_statement(line, line_no)?;
            if layer
                .bars
                .insert(bar_selector.clone(), bar_pattern)
                .is_some()
            {
                return Err(ParseError {
                    line: line_no,
                    message: format!("duplicate `{}` definition", bar_selector.header_label()),
                });
            }
        }
    }

    if let Some(layer) = current_layer.take() {
        layers.push(layer);
    }

    let program = Program {
        bpm,
        tempo_changes,
        bars,
        layers,
    };
    validate_program(&program)?;
    Ok(program)
}

fn validate_program(program: &Program) -> Result<(), ParseError> {
    let max_bars = program.effective_bars();
    let mut expected_intro = 1u32;
    for (selector, change) in &program.tempo_changes {
        match selector {
            BarSelector::Intro(intro_index) if *intro_index != expected_intro => {
                return Err(ParseError {
                    line: change.source_line,
                    message: format!(
                        "missing `{}` before `bpm {}`",
                        BarSelector::Intro(expected_intro).header_label(),
                        selector.header_label()
                    ),
                });
            }
            BarSelector::Intro(_) => {
                expected_intro += 1;
            }
            BarSelector::Exact(bar_index) if *bar_index > max_bars => {
                return Err(ParseError {
                    line: change.source_line,
                    message: format!(
                        "`bpm [bar{bar_index}]` is out of range for bars={max_bars}"
                    ),
                });
            }
            BarSelector::Every(value) if *value < 2 => {
                return Err(ParseError {
                    line: change.source_line,
                    message: "`bpm [bar%N]` requires N >= 2".to_string(),
                });
            }
            BarSelector::Default => unreachable!("default tempo selector is rejected during parse"),
            _ => {}
        }
    }

    for layer in &program.layers {
        let mut expected_intro = 1u32;
        for (bar_selector, bar) in &layer.bars {
            match bar_selector {
                BarSelector::Intro(intro_index) if *intro_index != expected_intro => {
                    return Err(ParseError {
                        line: bar.source_line,
                        message: format!(
                            "missing `{}` before `{}`",
                            BarSelector::Intro(expected_intro).header_label(),
                            bar_selector.header_label()
                        ),
                    });
                }
                BarSelector::Intro(_) => {
                    expected_intro += 1;
                }
                BarSelector::Exact(bar_index) if *bar_index > max_bars => {
                    return Err(ParseError {
                        line: bar.source_line,
                        message: format!("`[bar{bar_index}]` is out of range for bars={max_bars}"),
                    });
                }
                BarSelector::Every(value) if *value < 2 => {
                    return Err(ParseError {
                        line: bar.source_line,
                        message: "`[bar%N]` requires N >= 2".to_string(),
                    });
                }
                _ => {}
            }
        }
    }
    Ok(())
}

struct AssignmentStatement<'a> {
    name: &'a str,
    selector: Option<BarSelector>,
    value: f32,
}

fn strip_comment(line: &str) -> &str {
    match line.split_once('#') {
        Some((before, _)) => before,
        None => line,
    }
}

fn parse_assignment_statement(
    line: &str,
    line_no: usize,
) -> Result<AssignmentStatement<'_>, ParseError> {
    let (_, (name, selector, _, value)) = all_consuming((
        identifier,
        opt(preceded(multispace1, bar_header)),
        delimited(space0, char('='), space0),
        float_value,
    ))
    .parse(line)
    .map_err(|_| ParseError {
        line: line_no,
        message: "invalid assignment".to_string(),
    })?;
    Ok(AssignmentStatement {
        name,
        selector,
        value,
    })
}

fn parse_layer_statement(line: &str, line_no: usize) -> Result<Layer, ParseError> {
    let (_, (default_target, modifiers)) =
        all_consuming((layer_header, many0(preceded(multispace1, layer_modifier))))
            .parse(line)
            .map_err(|_| ParseError {
                line: line_no,
                message: "invalid layer statement".to_string(),
            })?;

    Ok(Layer {
        name: Symbol(default_target.display_name()),
        default_target,
        modifiers,
        bars: BTreeMap::new(),
        source_line: line_no,
    })
}

fn parse_bar_statement(
    line: &str,
    line_no: usize,
) -> Result<(BarSelector, BarPattern), ParseError> {
    let (_, (bar_selector, items)) =
        all_consuming((bar_header, many0(preceded(multispace1, bar_item))))
            .parse(line)
            .map_err(|_| ParseError {
                line: line_no,
                message: "invalid bar statement".to_string(),
            })?;

    let mut pattern = PatternSource::ImplicitSelf;
    let mut pattern_seen = false;
    let mut modifiers = Vec::new();

    for item in items {
        match item {
            BarItem::Pattern(next) => {
                if pattern_seen {
                    return Err(ParseError {
                        line: line_no,
                        message: "bar statement can only contain one pattern body".to_string(),
                    });
                }
                pattern = next;
                pattern_seen = true;
            }
            BarItem::Modifier(modifier) => modifiers.push(modifier),
        }
    }

    Ok((
        bar_selector,
        BarPattern {
            pattern,
            modifiers,
            source_line: line_no,
        },
    ))
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

fn bar_header(input: &str) -> IResult<&str, BarSelector> {
    alt((
        map(tag("[intro]"), |_| BarSelector::Intro(1)),
        map(
            delimited(tag("[intro"), unsigned_value, char(']')),
            BarSelector::Intro,
        ),
        map(tag("[default]"), |_| BarSelector::Default),
        map(
            delimited(tag("[bar%"), every_value, char(']')),
            BarSelector::Every,
        ),
        map(
            delimited(tag("[bar"), unsigned_value, char(']')),
            BarSelector::Exact,
        ),
    ))
    .parse(input)
}

fn unsigned_value(input: &str) -> IResult<&str, u32> {
    map_res(digit1, str::parse::<u32>).parse(input)
}

fn every_value(input: &str) -> IResult<&str, u32> {
    map_res(digit1, |value: &str| {
        let parsed = value.parse::<u32>().map_err(|_| "invalid periodic value")?;
        if parsed < 2 {
            return Err("periodic value must be at least 2");
        }
        Ok::<u32, &'static str>(parsed)
    })
    .parse(input)
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

fn effect_modifier<'a>(
    name: &'static str,
) -> impl Parser<&'a str, Output = f32, Error = nom::error::Error<&'a str>> {
    preceded(
        tag("."),
        preceded(tag(name), preceded(multispace0, float_value)),
    )
}

fn any_modifier(input: &str) -> IResult<&str, Modifier> {
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

fn layer_modifier(input: &str) -> IResult<&str, Modifier> {
    alt((
        map(effect_modifier("gain"), Modifier::Gain),
        map(effect_modifier("pan"), Modifier::Pan),
        map(effect_modifier("speed"), Modifier::Speed),
        map(effect_modifier("sustain"), Modifier::Sustain),
    ))
    .parse(input)
}

enum BarItem {
    Pattern(PatternSource),
    Modifier(Modifier),
}

fn bar_item(input: &str) -> IResult<&str, BarItem> {
    alt((
        map(any_modifier, BarItem::Modifier),
        map(pattern_source, BarItem::Pattern),
    ))
    .parse(input)
}

fn pattern_source(input: &str) -> IResult<&str, PatternSource> {
    alt((sequence_body, group_body, atom_body)).parse(input)
}

fn sequence_body(input: &str) -> IResult<&str, PatternSource> {
    alt((sequence_grid_body, sequence_list_body)).parse(input)
}

fn sequence_grid_body(input: &str) -> IResult<&str, PatternSource> {
    map(
        delimited(
            char('<'),
            verify(take_until(">"), |content: &str| is_compact_grid(content)),
            char('>'),
        ),
        |content: &str| {
            PatternSource::Sequence(
                content
                    .chars()
                    .filter(|ch| !ch.is_whitespace())
                    .map(|ch| match ch {
                        'o' | 'O' => PatternValue::Hit,
                        'x' | 'X' | '_' | '-' => PatternValue::Rest,
                        _ => unreachable!("grid parser validates characters"),
                    })
                    .collect(),
            )
        },
    )
    .parse(input)
}

fn sequence_list_body(input: &str) -> IResult<&str, PatternSource> {
    map(
        delimited(
            char('<'),
            separated_list1(multispace1, pattern_value),
            preceded(multispace0, char('>')),
        ),
        PatternSource::Sequence,
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

fn atom_body(input: &str) -> IResult<&str, PatternSource> {
    map(pattern_atom, PatternSource::Atom).parse(input)
}

fn pattern_value(input: &str) -> IResult<&str, PatternValue> {
    alt((
        map(hit_token, |_| PatternValue::Hit),
        map(rest_token, |_| PatternValue::Rest),
        map(note_value, PatternValue::Note),
        map(pattern_atom, PatternValue::Atom),
    ))
    .parse(input)
}

fn hit_token(input: &str) -> IResult<&str, &str> {
    tag("o").parse(input)
}

fn rest_token(input: &str) -> IResult<&str, &str> {
    alt((tag("x"), tag("_"), tag("-"))).parse(input)
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

fn is_compact_grid(content: &str) -> bool {
    let mut saw_step = false;
    for ch in content.chars().filter(|ch| !ch.is_whitespace()) {
        if !matches!(ch, 'o' | 'O' | 'x' | 'X' | '_' | '-') {
            return false;
        }
        saw_step = true;
    }
    saw_step
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
    use crate::model::{PatternAtom, PatternValue};

    #[test]
    fn parses_top_level_bars_assignment() {
        let program = parse_program("bpm = 120\nbars = 4\n").expect("parse should succeed");
        assert_eq!(program.bpm, Some(120.0));
        assert_eq!(program.bars, Some(4));
    }

    #[test]
    fn parses_scoped_bpm_assignments() {
        let program = parse_program("bpm = 120\nbpm [intro] = 90\nbpm [bar2] = 140\n")
            .expect("parse should succeed");

        assert_eq!(program.bpm, Some(120.0));
        assert_eq!(
            program
                .tempo_changes
                .get(&BarSelector::Intro(1))
                .map(|change| change.bpm),
            Some(90.0)
        );
        assert_eq!(
            program
                .tempo_changes
                .get(&BarSelector::Exact(2))
                .map(|change| change.bpm),
            Some(140.0)
        );
    }

    #[test]
    fn parses_layer_with_effect_modifiers() {
        let program = parse_program("[hh] .gain 0.5 .pan -0.25").expect("parse should succeed");
        assert_eq!(program.layers.len(), 1);
        assert_eq!(
            program.layers[0].modifiers,
            vec![Modifier::Gain(0.5), Modifier::Pan(-0.25)]
        );
    }

    #[test]
    fn rejects_rhythmic_modifier_on_layer_line() {
        let error = parse_program("[bd] /4").expect_err("parse should fail");
        assert!(error.message.contains("invalid layer statement"));
    }

    #[test]
    fn parses_bar_pattern_with_subdivision_and_atom_sequence() {
        let program =
            parse_program("bars = 4\n[bd]\n  [bar1] /4 <0 3 5 7>\n").expect("parse should succeed");
        let bar = program.layers[0]
            .bars
            .get(&BarSelector::Exact(1))
            .expect("bar should exist");
        assert_eq!(bar.modifiers, vec![Modifier::Divide(4)]);
        assert_eq!(
            bar.pattern,
            PatternSource::Sequence(vec![
                PatternValue::Atom(PatternAtom::SampleIndex(0)),
                PatternValue::Atom(PatternAtom::SampleIndex(3)),
                PatternValue::Atom(PatternAtom::SampleIndex(5)),
                PatternValue::Atom(PatternAtom::SampleIndex(7)),
            ])
        );
    }

    #[test]
    fn parses_bar_pattern_with_note_sequence() {
        let program =
            parse_program("[bass]\n  [bar1] /1 <g4 a4 a3 c3>\n").expect("parse should succeed");
        let bar = program.layers[0]
            .bars
            .get(&BarSelector::Exact(1))
            .expect("bar should exist");
        assert_eq!(bar.modifiers, vec![Modifier::Divide(1)]);
        match &bar.pattern {
            PatternSource::Sequence(values) => assert!(matches!(values[0], PatternValue::Note(_))),
            other => panic!("expected sequence, got {other:?}"),
        }
    }

    #[test]
    fn parses_compact_hit_rest_grid_sequence() {
        let program = parse_program("[bd]\n  [bar1] /16 <xxxoxxxxxxxooxxxo>\n")
            .expect("parse should succeed");
        let bar = program.layers[0]
            .bars
            .get(&BarSelector::Exact(1))
            .expect("bar should exist");
        assert_eq!(bar.modifiers, vec![Modifier::Divide(16)]);
        assert_eq!(
            bar.pattern,
            PatternSource::Sequence(vec![
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Hit,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Hit,
                PatternValue::Hit,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Rest,
                PatternValue::Hit,
            ])
        );
    }

    #[test]
    fn parses_spaced_restful_note_sequence() {
        let program =
            parse_program("[bass]\n  [bar1] /1 <g4 x a4 x>\n").expect("parse should succeed");
        let bar = program.layers[0]
            .bars
            .get(&BarSelector::Exact(1))
            .expect("bar should exist");
        assert_eq!(
            bar.pattern,
            PatternSource::Sequence(vec![
                PatternValue::Note(NoteValue {
                    label: "g4".to_string(),
                    semitone: -5.0,
                }),
                PatternValue::Rest,
                PatternValue::Note(NoteValue {
                    label: "a4".to_string(),
                    semitone: -3.0,
                }),
                PatternValue::Rest,
            ])
        );
    }

    #[test]
    fn parses_default_bar_pattern() {
        let program =
            parse_program("[bd]\n  [default] /4 <0 3 5 7>\n").expect("parse should succeed");
        let bar = program.layers[0]
            .bars
            .get(&BarSelector::Default)
            .expect("default bar should exist");
        assert_eq!(bar.modifiers, vec![Modifier::Divide(4)]);
    }

    #[test]
    fn parses_intro_bar_pattern() {
        let program =
            parse_program("[bd]\n  [intro] /4 <0 3 5 7>\n").expect("parse should succeed");
        let bar = program.layers[0]
            .bars
            .get(&BarSelector::Intro(1))
            .expect("intro bar should exist");
        assert_eq!(bar.modifiers, vec![Modifier::Divide(4)]);
    }

    #[test]
    fn parses_numbered_intro_bar_pattern() {
        let program =
            parse_program("[bd]\n  [intro2] /4 <0 3 5 7>\n").expect_err("parse should fail");
        assert!(
            program
                .message
                .contains("missing `[intro]` before `[intro2]`")
        );
    }

    #[test]
    fn parses_contiguous_numbered_intro_bar_patterns() {
        let program = parse_program("[bd]\n  [intro] /1 <7>\n  [intro2] /4 <0 3 5 7>\n")
            .expect("parse should succeed");
        let bar = program.layers[0]
            .bars
            .get(&BarSelector::Intro(2))
            .expect("intro2 bar should exist");
        assert_eq!(bar.modifiers, vec![Modifier::Divide(4)]);
    }

    #[test]
    fn parses_periodic_bar_pattern() {
        let program =
            parse_program("[bd]\n  [bar%4] /4 <0 3 5 7>\n").expect("parse should succeed");
        let bar = program.layers[0]
            .bars
            .get(&BarSelector::Every(4))
            .expect("periodic bar should exist");
        assert_eq!(bar.modifiers, vec![Modifier::Divide(4)]);
    }

    #[test]
    fn rejects_bar_outside_global_phrase_length() {
        let error = parse_program("bars = 4\n[bd]\n  [bar5] /1\n").expect_err("parse should fail");
        assert!(error.message.contains("out of range"));
    }

    #[test]
    fn rejects_duplicate_bar_definition() {
        let error =
            parse_program("[bd]\n  [bar1] /1\n  [bar1] /2\n").expect_err("parse should fail");
        assert!(error.message.contains("duplicate"));
    }

    #[test]
    fn rejects_duplicate_default_definition() {
        let error =
            parse_program("[bd]\n  [default] /1\n  [default] /2\n").expect_err("parse should fail");
        assert!(error.message.contains("[default]"));
    }

    #[test]
    fn rejects_duplicate_intro_definition() {
        let error =
            parse_program("[bd]\n  [intro] /1\n  [intro] /2\n").expect_err("parse should fail");
        assert!(error.message.contains("[intro]"));
    }

    #[test]
    fn rejects_gapped_intro_definition() {
        let error =
            parse_program("[bd]\n  [intro] /1\n  [intro3] /2\n").expect_err("parse should fail");
        assert!(
            error
                .message
                .contains("missing `[intro2]` before `[intro3]`")
        );
    }

    #[test]
    fn rejects_duplicate_periodic_definition() {
        let error =
            parse_program("[bd]\n  [bar%4] /1\n  [bar%4] /2\n").expect_err("parse should fail");
        assert!(error.message.contains("[bar%4]"));
    }

    #[test]
    fn rejects_invalid_periodic_definition() {
        let error = parse_program("[bd]\n  [bar%1] /1\n").expect_err("parse should fail");
        assert!(error.message.contains("invalid bar statement"));
    }

    #[test]
    fn rejects_duplicate_bars_assignment() {
        let error = parse_program("bars = 4\nbars = 8\n").expect_err("parse should fail");
        assert!(error.message.contains("duplicate `bars`"));
    }

    #[test]
    fn rejects_bpm_default_assignment() {
        let error = parse_program("bpm [default] = 140\n").expect_err("parse should fail");
        assert!(error.message.contains("use `bpm = <number>`"));
    }
}
