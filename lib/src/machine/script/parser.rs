//! Parser logic.
//!
//! Provides an interface for parsing scripts into sets of instructions.

use crate::{string, ShortString};

use super::{DirectivePrototype, Instruction, InstructionType};

use crate::util;

use crate::machine::error::{Error, ErrorKind, Result};
use crate::machine::{CommandPrototype, LocationInfo};

static DIRECTIVE_SYMBOL: char = '!';
static COMMENT_SYMBOL: &str = "#";
static TAG_SYMBOL: char = '@';
static MULTILINE_SYMBOL: char = '\\';
static END_LINE_SYMBOL: char = ';';

/// Parses script at the given path. Returns a list of instructions or an error.
pub(crate) fn parse_script_at<FS: vfs::FileSystem>(
    fs: &FS,
    script_relative_path: &str,
    project_path: &str,
) -> Result<Vec<Instruction>> {
    let location = LocationInfo::empty().with_source(project_path, script_relative_path);
    println!("project path: {}", project_path);
    let full_path = if project_path == "" || project_path == "/" {
        script_relative_path.to_string()
    } else {
        format!("{}/{}", project_path, script_relative_path)
    };

    debug!("parsing script at: {}", full_path);
    let text = util::read_text_file(fs, &full_path).map_err(|e| {
        Error::new(
            location.clone(),
            ErrorKind::ErrorReadingFile(script_relative_path.to_string()),
        )
    })?;
    parse_script(&text, location)
}

/// Parses multi-line script string into a sequence of instructions.
pub fn parse_script(lines: &str, location: LocationInfo) -> Result<Vec<Instruction>> {
    let mut instructions = vec![];

    // multiline switch
    let mut multiline_str = None;
    let mut multiline_line = 0;

    let mut line_number = 1;
    for mut line in lines.lines() {
        // trim whitespace on both ends of the line
        let mut line = line.trim().to_string();
        // create a location info struct for current line
        let mut location_info = location.clone();
        location_info.source_line = Some(line_number);
        line_number = line_number + 1;

        // is the current line supposed to be continuation of the previous line?
        if multiline_str.is_some() {
            line = format!("{}{}", multiline_str.as_ref().unwrap(), line);
            // instructions.push(Instruction {
            // location: location_info.clone(),
            // type: InstructionType::None,
            //});
        }

        // is the multiline symbol present?
        if line.trim_end().ends_with(MULTILINE_SYMBOL) {
            // is this the start of a new multiline?
            if multiline_str.is_none() {
                // if so then mark current line as start of multiline
                multiline_line = location_info.source_line.unwrap();
            }
            // set multiline_str to current line, without the multiline
            // symbol
            multiline_str = Some(line[..line.len() - 1].to_string());
            // continue until end of multiline
            continue;
        }
        // multiline symbol not present
        else {
            // conclude multiline concatenation if it was ongoing
            if multiline_str.is_some() {
                location_info.source_line = Some(multiline_line);
                multiline_str = None;
                multiline_line = 0;
            }
        }

        // does the line start with a comment symbol?
        if line.starts_with(COMMENT_SYMBOL) {
            continue;
        }

        // is end line symbol present inside the line?
        if line.contains(END_LINE_SYMBOL) {
            // if so then split the line
            let mut split_lines = line
                .split(END_LINE_SYMBOL)
                .map(|sl| sl.trim())
                .filter(|sl| sl != &"")
                .collect::<Vec<&str>>();
            //println!("{:?}", split_lines);
            for split_line in split_lines {
                match parse_line(split_line, location_info.clone()) {
                    Ok(instruction) => instructions.push(instruction),
                    Err(e) => return Err(e),
                };
            }
        } else {
            match parse_line(&line, location_info) {
                Ok(instruction) => instructions.push(instruction),
                Err(e) => return Err(e),
            };
        }
    }

    Ok(instructions)
}

/// Parses a single line into a single instruction.
pub fn parse_line(line_text: &str, location_info: LocationInfo) -> Result<Instruction> {
    let trimmed_text = line_text.trim();

    if trimmed_text.is_empty() || trimmed_text.starts_with(&COMMENT_SYMBOL) {
        Ok(Instruction {
            type_: InstructionType::None,
            location: location_info,
        })
    } else {
        if trimmed_text.starts_with(DIRECTIVE_SYMBOL) {
            parse_directive(&trimmed_text, 1, location_info)
        } else {
            parse_command(&trimmed_text, 0, location_info)
        }
    }
}

/// Parses a directive string into an intermediate instruction.
fn parse_directive(text: &str, start_index: usize, location: LocationInfo) -> Result<Instruction> {
    if text.is_empty() {
        Err(Error::new(location, ErrorKind::NoDirectivePresent))
    } else {
        let mut directive = String::new();
        let end_index = text.len();
        let mut directive_end_index = end_index;
        for index in start_index..end_index {
            let character = text.chars().collect::<Vec<char>>()[index];
            if character == ' ' {
                if !directive.is_empty() {
                    directive_end_index = index;
                    break;
                }
            } else {
                directive.push(character);
            }
        }

        if directive.is_empty() {
            Err(Error::new(location, ErrorKind::NoDirectivePresent))
        } else {
            match parse_arguments_posix(&text, directive_end_index, &location) {
                Ok(arguments) => Ok(Instruction {
                    location,
                    type_: InstructionType::Directive(DirectivePrototype {
                        name: Some(directive),
                        arguments: Some(arguments),
                    }),
                }),
                Err(error) => Err(error),
            }
        }
    }
}

/// Parses a command string into an intermediate instruction.
fn parse_command(
    text: &str,
    start_index: usize,
    mut location_info: LocationInfo,
) -> Result<Instruction> {
    let end_index = text.len();

    if text.is_empty() {
        Ok(Instruction {
            location: location_info,
            type_: InstructionType::None,
        })
    } else {
        // search for label
        let mut index = start_index;
        let mut instruction = CommandPrototype {
            output: None,
            name: None,
            arguments: None,
        };
        match find_tag(&location_info, &text, index) {
            Ok((next_index, value)) => {
                index = next_index;

                if let Some(val) = value {
                    location_info.tag = Some(string::new_truncate(&val[1..]));
                }
                //if value.is_some() {
                //location_info.tag = Some(ArrStr10::from_str_truncate(value));
                //}
            }
            Err(error) => return Err(error),
        };

        // find output variable and command
        index = match find_output_and_command(&location_info, &text, index, &mut instruction) {
            Ok(next_index) => next_index,
            Err(error) => return Err(error),
        };

        match parse_arguments_posix(&text, index, &location_info) {
            Ok(arguments) => {
                // println!("{:?}", arguments);
                instruction.arguments = Some(arguments);

                let instruction_kind = if location_info.tag.is_none()
                    && instruction.output.is_none()
                    && instruction.name.is_none()
                {
                    InstructionType::None
                } else {
                    InstructionType::Command(instruction)
                };

                Ok(Instruction {
                    location: location_info,
                    type_: instruction_kind,
                })
            }
            Err(error) => Err(error),
        }
    }
}

fn parse_arguments_posix(
    text: &str,
    start_index: usize,
    location: &LocationInfo,
) -> Result<Vec<String>> {
    shlex::split(&text[start_index..])
        .ok_or(Error::new(location.clone(), ErrorKind::MissingEndQuotes))
}

fn parse_arguments(
    line_text: &str,
    start_index: usize,
    location_info: &LocationInfo,
) -> Result<Option<Vec<String>>> {
    let mut arguments = vec![];

    let mut index = start_index;
    loop {
        match parse_next_argument(&location_info, &line_text, index) {
            Ok(output) => {
                let (next_index, argument) = output;

                if argument.is_none() {
                    break;
                }

                arguments.push(argument.unwrap());
                index = next_index;
            }
            Err(error) => return Err(error),
        }
    }

    if arguments.is_empty() {
        Ok(None)
    } else {
        Ok(Some(arguments))
    }
}

fn parse_next_argument(
    location_info: &LocationInfo,
    line_text: &str,
    start_index: usize,
) -> Result<(usize, Option<String>)> {
    parse_next_value(&location_info, &line_text, start_index, true, true, false)
}

fn parse_next_value(
    location: &LocationInfo,
    line_text: &str,
    start_index: usize,
    allow_quotes: bool,
    allow_control: bool,
    stop_on_equals: bool,
) -> Result<(usize, Option<String>)> {
    let end_index = line_text.len();

    if start_index >= end_index {
        Ok((start_index, None))
    } else {
        let mut argument = String::new();
        let mut index = start_index;
        let mut in_argument = false;
        let mut using_quotes = false;
        let mut in_control = false;
        let mut found_end = false;
        let mut found_variable_prefix = false;
        for _i in index..end_index {
            let character = line_text.chars().collect::<Vec<char>>()[index];
            index = index + 1;

            if in_argument {
                if in_control {
                    if found_variable_prefix {
                        if character == '{' {
                            argument.push_str("\\${");
                            in_control = false;
                            found_variable_prefix = false;
                        } else {
                            return Err(Error::new(
                                location.clone(),
                                ErrorKind::ControlWithoutValidValue,
                            ));
                        }
                    } else if character == '\\' || character == '"' {
                        argument.push(character);
                        in_control = false;
                    } else if character == 'n' {
                        argument.push('\n');
                        in_control = false;
                    } else if character == 'r' {
                        argument.push('\r');
                        in_control = false;
                    } else if character == 't' {
                        argument.push('\t');
                        in_control = false;
                    } else if character == '$' {
                        found_variable_prefix = true;
                    } else {
                        return Err(Error::new(
                            location.clone(),
                            ErrorKind::ControlWithoutValidValue,
                        ));
                    }
                } else if character == '\\' {
                    if allow_control {
                        in_control = true;
                        found_variable_prefix = false;
                    } else {
                        // return Err(Error::new(
                        //     location.clone(),
                        //     ErrorKind::InvalidControlLocation,
                        // ));
                    }
                } else if using_quotes && character == '"' {
                    found_end = true;
                    break;
                } else if !using_quotes
                    && (character == ' '
                        || character == '#'
                        || (stop_on_equals && character == '='))
                {
                    if character == ' ' || character == '=' {
                        index = index - 1;
                    } else if character == '#' {
                        index = end_index;
                    }
                    found_end = true;
                    break;
                } else {
                    argument.push(character);
                }
            } else if character == '#' {
                index = end_index;
                break;
            } else if character != ' ' {
                in_argument = true;

                if character == '"' {
                    if allow_quotes {
                        using_quotes = true;
                    } else {
                        return Err(Error::new(
                            location.clone(),
                            ErrorKind::InvalidQuotesLocation,
                        ));
                    }
                } else if character == '\\' {
                    if allow_control {
                        in_control = true;
                    } else {
                        // return Err(Error::new(
                        //     location.clone(),
                        //     ErrorKind::InvalidControlLocation,
                        // ));
                    }
                } else {
                    argument.push(character);
                }
            }
        }

        if in_argument && !found_end && (in_control || using_quotes) {
            if in_control {
                Err(Error::new(
                    location.clone(),
                    ErrorKind::ControlWithoutValidValue,
                ))
            } else {
                Err(Error::new(location.clone(), ErrorKind::MissingEndQuotes))
            }
        } else if argument.is_empty() {
            if using_quotes {
                Ok((index, Some(argument)))
            } else {
                Ok((index, None))
            }
        } else {
            Ok((index, Some(argument)))
        }
    }
}

fn find_tag(
    location: &LocationInfo,
    line_text: &str,
    start_index: usize,
) -> Result<(usize, Option<String>)> {
    let end_index = line_text.len();

    if start_index >= end_index {
        Ok((start_index, None))
    } else {
        let mut label = None;
        let mut index = start_index;
        for _i in index..end_index {
            let character = line_text.chars().collect::<Vec<char>>()[index];
            index = index + 1;

            if character == TAG_SYMBOL {
                match parse_next_value(&location, &line_text, index, false, false, false) {
                    Ok(output) => {
                        let (next_index, value) = output;
                        index = next_index;

                        match value {
                            Some(label_value) => {
                                if label_value.is_empty() {
                                    return Err(Error::new(location.clone(), ErrorKind::EmptyTag));
                                }

                                let mut text = String::new();
                                text.push(TAG_SYMBOL);
                                text.push_str(&label_value);

                                label = Some(text);
                            }
                            None => (),
                        };

                        break;
                    }
                    Err(error) => return Err(error),
                };
            } else if character != ' ' {
                index = index - 1;
                break;
            }
        }

        Ok((index, label))
    }
}

fn find_output_and_command(
    location_info: &LocationInfo,
    line_text: &str,
    start_index: usize,
    instruction: &mut CommandPrototype,
) -> Result<usize> {
    match parse_next_value(&location_info, &line_text, start_index, false, false, true) {
        Ok(output) => {
            let (next_index, value) = output;

            if value.is_none() {
                Ok(next_index)
            } else {
                let mut index = next_index;
                let end_index = line_text.len();
                for _i in index..end_index {
                    let character = line_text.chars().collect::<Vec<char>>()[index];
                    index = index + 1;

                    if character != ' ' {
                        if character == '=' {
                            instruction.output = value.clone();
                        }

                        break;
                    }
                }

                if instruction.output.is_some() {
                    match parse_next_value(&location_info, &line_text, index, false, false, false) {
                        Ok(output) => {
                            let (next_index, value) = output;

                            if value.is_none() {
                                Ok(index)
                            } else {
                                instruction.name = value.clone();
                                Ok(next_index)
                            }
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    instruction.name = value.clone();

                    Ok(next_index)
                }
            }
        }
        Err(error) => Err(error),
    }
}

#[test]
fn test_parse_directive() {
    let text = r###"
        !include foo.bar
        !log "foo"
    "###;
    let instructions = parse_script(text.trim(), LocationInfo::empty()).unwrap();
    let expected = vec![
        Instruction {
            location: LocationInfo {
                source_line: Some(1),
                ..Default::default()
            },
            type_: InstructionType::Directive(DirectivePrototype {
                name: Some("include".to_owned()),
                arguments: Some(vec!["foo.bar".to_owned()]),
            }),
        },
        Instruction {
            location: LocationInfo {
                source_line: Some(2),
                ..Default::default()
            },
            type_: InstructionType::Directive(DirectivePrototype {
                name: Some("log".to_owned()),
                arguments: Some(vec!["foo".to_owned()]),
            }),
        },
    ];

    assert_eq!(instructions, expected)
}
