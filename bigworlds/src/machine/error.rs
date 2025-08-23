use std::fmt;

// use thiserror::Error;
// use crate::signal::Signal;

use super::LocationInfo;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    location: LocationInfo,
    kind: ErrorKind,
}

impl Error {
    pub fn new(location: LocationInfo, kind: ErrorKind) -> Self {
        Self { location, kind }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorKind {
    CoreError(String),
    CommonError(String),
    FromSignalError(String),

    ParseError(String),

    // basic parsing
    ErrorReadingFile(String),
    Initialization(String),
    ControlWithoutValidValue,
    InvalidControlLocation,
    MissingEndQuotes,
    MissingOutputVariableName,
    InvalidEqualsLocation,
    InvalidQuotesLocation,
    EmptyTag,
    CommandSearchFailed(String),

    // directives
    NoDirectivePresent,
    UnknownDirective,
    ErrorProcessingDirective(String),

    // commands
    NoCommandPresent,
    UnknownCommand(String),
    InvalidCommandBody(String),

    // addresses
    InvalidAddress(String),

    // processing
    Panic,
    StackEmpty,
    FailedGettingFromStorage(String),
    FailedGettingComponent(String),

    MissingLocationLine,

    Other(String),
}

impl From<ErrorKind> for Error {
    fn from(value: ErrorKind) -> Self {
        Self::new(LocationInfo::empty(), value)
    }
}

impl From<crate::error::Error> for ErrorKind {
    fn from(e: crate::error::Error) -> Self {
        ErrorKind::CoreError(e.to_string())
    }
}

// impl From<Signal> for Error {
//     fn from(e: Signal) -> Self {
//         Self {
//             location: Default::default(),
//             kind: ErrorKind::FromSignalError(format!("{:?}", e)),
//         }
//     }
// }

impl From<Fail> for Error {
    fn from(e: Fail) -> Self {
        Self {
            location: Default::default(),
            kind: ErrorKind::ParseError(e.to_string()),
        }
    }
}

impl From<crate::error::Error> for Error {
    fn from(e: crate::error::Error) -> Self {
        Self {
            location: Default::default(),
            kind: ErrorKind::CoreError(e.to_string()),
        }
    }
}

impl fmt::Display for Error {
    /// Formats the script error using the given formatter.
    fn fmt(&self, formatter: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        match self.kind {
            ErrorKind::ErrorReadingFile(ref file) => {
                writeln!(formatter, "error reading file: {}", file)?;
                Ok(())
            }
            ErrorKind::Initialization(ref message) => write!(formatter, "{}", message),
            ErrorKind::ControlWithoutValidValue => fmt_err_msg(
                formatter,
                &self.location,
                "control character found without a valid value",
            ),
            ErrorKind::InvalidControlLocation => fmt_err_msg(
                formatter,
                &self.location,
                "invalid control character location",
            ),
            ErrorKind::MissingEndQuotes => {
                fmt_err_msg(formatter, &self.location, "missing end quotes")
            }
            ErrorKind::MissingOutputVariableName => {
                fmt_err_msg(formatter, &self.location, "missing variable name")
            }
            ErrorKind::InvalidEqualsLocation => {
                fmt_err_msg(formatter, &self.location, "invalid equals sign location")
            }
            ErrorKind::InvalidQuotesLocation => {
                fmt_err_msg(formatter, &self.location, "invalid quotes location")
            }
            ErrorKind::EmptyTag => fmt_err_msg(formatter, &self.location, "empty tag"),
            ErrorKind::NoDirectivePresent => {
                fmt_err_msg(formatter, &self.location, "no directive present")
            }
            ErrorKind::UnknownDirective => {
                fmt_err_msg(formatter, &self.location, "unknow directive")
            }
            ErrorKind::ErrorProcessingDirective(ref message) => {
                fmt_err_msg(formatter, &self.location, &message)
            }
            ErrorKind::NoCommandPresent => {
                fmt_err_msg(formatter, &self.location, "no command present")
            }
            ErrorKind::UnknownCommand(ref cmd_name) => {
                let msg = format!("unknown command: {}", cmd_name);
                fmt_err_msg(formatter, &self.location, &msg)
            }
            ErrorKind::InvalidCommandBody(ref message) => {
                // format_error_message(formatter, &self.location, &message)
                // write!(formatter, "{}", message);
                // Ok(())
                write!(
                    formatter,
                    "{}",
                    format_err_init_cmd(message, &self.location)
                );
                Ok(())
            }
            ErrorKind::FailedGettingFromStorage(ref addr) => fmt_err_msg(
                formatter,
                &self.location,
                &format!(
                    "failed getting variable from storage: {}",
                    &addr.to_string()
                ),
            ),
            ErrorKind::FailedGettingComponent(ref addr) => fmt_err_msg(
                formatter,
                &self.location,
                &format!("failed getting component: {}", &addr.to_string()),
            ),
            ErrorKind::CommandSearchFailed(ref msg) => fmt_err_msg(
                formatter,
                &self.location,
                &format!("command search failed: {}", msg),
            ),
            ErrorKind::InvalidAddress(ref msg) => fmt_err_msg(
                formatter,
                &self.location,
                &format!("invalid address: {}", msg),
            ),
            ErrorKind::CoreError(ref msg) => {
                fmt_err_msg(formatter, &self.location, &format!("core error: {}", msg))
            }
            ErrorKind::ParseError(ref msg) => {
                fmt_err_msg(formatter, &self.location, &format!("parse error: {}", msg))
            }
            ErrorKind::Panic => fmt_err_msg(formatter, &self.location, &format!("panic")),
            ErrorKind::StackEmpty => {
                fmt_err_msg(formatter, &self.location, &format!("stack empty"))
            }
            ErrorKind::CommonError(ref msg) => {
                fmt_err_msg(formatter, &self.location, &format!("common error: {}", msg))
            }
            ErrorKind::FromSignalError(ref msg) => fmt_err_msg(
                formatter,
                &self.location,
                &format!("from signal error: {}", msg),
            ),
            ErrorKind::MissingLocationLine => fmt_err_msg(
                formatter,
                &self.location,
                &format!("missing line in locationinfo"),
            ),
            ErrorKind::Other(ref msg) => {
                fmt_err_msg(formatter, &self.location, &format!("other error: {}", msg))
            }
        }
    }
}

fn fmt_err_msg(
    formatter: &mut fmt::Formatter,
    location_info: &LocationInfo,
    message: &str,
) -> std::result::Result<(), fmt::Error> {
    let source = match location_info.source {
        Some(ref value) => value.to_string(),
        None => "Unknown".to_string(),
    };
    let line = match location_info.source_line {
        Some(value) => value.to_string(),
        None => "Unknown".to_string(),
    };

    write!(
        formatter,
        "source: {}, line: {} - {}",
        source, line, message
    )
}

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use getopts::Fail;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;

fn format_err_init_cmd(msg: &str, location: &LocationInfo) -> String {
    // println!("{:?}", location.source);
    let source = PathBuf::new()
        .join(location.root.as_ref().unwrap().as_str())
        .join(location.source.as_ref().unwrap().as_str());
    // let source = format!("{}/{}", &location.root.unwrap(),
    // &location.source.unwrap()); println!("{:?}", source);
    let mut source_file = File::open(source).unwrap();
    let start_line = location.source_line.unwrap();
    let source_string: String = BufReader::new(source_file)
        .lines()
        .nth(start_line - 1)
        .unwrap()
        .unwrap();

    let split_nth = source_string.split(' ').nth(1).unwrap();
    let range_start = source_string
        .find(source_string.split(' ').nth(1).unwrap())
        .unwrap();
    let range_end = range_start + split_nth.len();

    let snippet = Snippet {
        title: Some(Annotation {
            label: Some("failed initializing command"),
            id: None,
            annotation_type: AnnotationType::Error,
        }),
        footer: vec![Annotation {
            label: Some("possible arguments: another"),
            id: None,
            annotation_type: AnnotationType::Help,
        }],
        slices: vec![Slice {
            source: &source_string,
            line_start: start_line,
            origin: Some(location.source.as_ref().unwrap()),
            fold: true,
            annotations: vec![SourceAnnotation {
                label: msg,
                annotation_type: AnnotationType::Error,
                range: (range_start, range_end),
            }],
        }],
        opt: FormatOptions {
            color: true,
            ..Default::default()
        },
    };

    let dl = DisplayList::from(snippet);
    // println!("{}\n", dl);
    dl.to_string()
}
