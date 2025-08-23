//! Module implementing the default script processor.
//!
//! In general, scripts follow quite closely the way the *machine* handles
//! logic, that is it focuses directly on commands. This processor also
//! includes a preprocessor.
//!
//! Note that this processor is not capable of creating actual `Command`
//! objects used by the *machine* to store and organize logic execution.
//!
//! # Basic rules and syntax
//!
//! Internal command representation within the engine is based on the idea of
//! *one line equals one command*. In general this also holds true for the
//! script processor, though it does allow including more than one command
//! within a single line. As it happens it's also totally fine to have lines
//! without any commands at all. You know, for comments and stuff. In the end
//! though, it's helpful to keep in mind that the processor will attempt to cut
//! your script into small chunks that it will label as commands and put on
//! a flat list.
//!
//! In the context of the processor we introduce a concept if an `instruction`.
//! Instruction is either a `directive` or a `command`. This is an important
//! distinction since we have a preprocessor to deal with, and we need to have
//! a way of distinguishing preprocessor instructions from the regular ones.
//! Directives are the domain of the preprocessor, while commands are what
//! will be eventually turned into internal engine representation of actual
//! executable commands.
//!
//!
//! Rules for directives are very simple, the most important being that to
//! define a directive instruction you will need to prepend the whole line with
//! an exclamation mark (`!`).
//!
//! ```text
//! ![directive] [arguments]
//! ```
//!
//! As the preprocessor directives are executed well before the regular
//! commands get any attention, they can be used to introduce some on-the-fly
//! modifications to the scripts. One example is conditional inclusion of parts
//! of a script, another would be importing other scripts into the currently
//! procesed one. For the complete list of available directives see directives.
//!
//!
//! Command instructions are a bit more complex. Generally speaking, the syntax
//! for a single command matches the following format:
//!
//! ```text
//! [@tag] [command] [arguments]
//! ```
//!
//! Any command instruction can begin with a tag. Tags are a way of identifying
//! commands that we may want to jump to during execution.
//!
//! Commands take different number of arguments. Arguments can include both
//! positional and optional arguments. Optional argument syntax follows the
//! regular POSIX style convention. For information about a specific command
//! and it's arguments consult the command reference.
//!
//! End of a command instruction is signalled by either a newline or
//! a semicolon (`;`). It's possible to create a single command spanning
//! multiple lines with the use of a backslash (`\`).
//!
//! ```text
//! # the following is a valid line
//! print "hello"; print "world"
//! print \
//!     "hello"; print "world" # this is valid as well
//! ```
//!
//! As seen above, the hash symbol (`#`) indicates a comment, meaning part of
//! the script that is completely ignored by the processor. Comments can be
//! placed either in their own line or they can be put after an existing
//! instruction.
//!
//! Indentations within scripts are recommended as they increase visibility.
//! They are however not required and are strictly optional. Any whitespace
//! found before or after the body of a command is automatically trimmed.
//!
//! ```text
//! if $foo
//!     if $bar
//!         print "foobar"
//!     else
//!         print "foo"
//!     end
//! end
//! ```
//!
//! One of the key syntax features available to the user is variable
//! substitution, indicated with the dolarsign symbol (`$`). Following the
//! dolarsign is an `Address`. `bigworlds` uses it's universal notion of
//! addresses to point to data throughout it's, potentially distributed,
//! storage space. To learn more about addresses see the appropriate module's
//! documentation.
//!
//! Component-level local variables are dynamically typed, addressed using
//! a simple single-part identifier and private, meaning they are only
//! accessible by commands that are running on the same component. Entity-level
//! local variables are typed and addressed using an identifier made up of
//! either two or four parts separated with a colon (`:`). They are also
//! public, in that they can be read (and mutated) both from the level of other
//! components and other entities.
//!
//! ```text
//! # component-level, private, dynamically typed
//! set component_level true
//! set component_level 0
//!
//! # entity-level, component context, public, types need to match
//! set $bool:entity_level true
//!
//! # entity-level, entity context
//! set $component:id:bool:entity_level true
//!
//! # sim-level
//! set $entity:id:component:id:bool:entity_level true
//! ```
//!
//! Addresses can also contain substitutions within their different parts.
//! We all such addresses *dynamic*.
//!
//! ```text
//! set component_name foo
//! set local_integer $component_type:$component_name:int:main
//! ```
//!
//!
//!
//! # Preprocessor and available directives
//!
//! This file processor includes a simple preprocessor, which will execute
//! a set of custom preprocessing instructions before starting the command
//! parsing. Preprocessing instructions are called *directives*.
//!
//! Following preprocessing instructions are available:
//! - `!log "message"` logs a message, default is the `info` buffer but it can
//!   be changed to `trace`, `debug`, `warn`, `error` with the use of an
//!   appropriate option, e.g. `!log --trace "message"`
//! - `!include foo.os bar.os` copies selected files' contents to the current
//!   file
//! - `!set some_option` sets preprocessor variable to the given value, creates
//!   the variable if it doesn't already exist
//! - `!if [condition]`, `!elseif [condition]` and `!else` allow to
//!   conditionally include or exclude instructions, both those meant for the
//!   preprocessor as well as the regular commands. Conditional checks can be
//!   run on data about the machine the program is running on as well as data
//!   about the program itself, including of course the preprocessor variable
//!   store created with `!set`.
//!
//! # Discussion
//!
//! Current implementation treats the `script` module as a feature that can be
//! replaced with another file script. Any alternative processor will have to
//! implement `from_scenario_at` and `from_scenario` on the `Sim` struct. This
//! means that the system only supports a single file type for module scripts
//! at any given time. While a fine solution, it may not be the most flexible
//! one. A more flexible solution could be implemented, but is it really
//! needed?
//!
//! Right now it would be totally feasible to swap out the default processor
//! for a `yaml` or `json` one that would use structured data files as the base
//! format for module files. Such new processor would be able to assemble
//! models based on the input data from the files, but it would probably lack
//! some of the functionality like a preprocessor or even comments (I'm looking
//! at you json). On the other side of the spectrum, one coule write
//! a processor that used more advanced scripting language like `lua` to
//! assemble the model.

// pub mod bridge;
pub mod parser;
pub mod preprocessor;
pub mod util;

pub(crate) use self::parser::parse_script;
pub(crate) use self::parser::parse_script_at;

pub(crate) const SCRIPT_FILE_EXTENSION: &str = ".bw";

use super::{CommandPrototype, LocationInfo};

/// Result of parsing a single line.
#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub location: LocationInfo,
    pub type_: InstructionType,
}

/// All the possible kinds of instructions, including `None`
/// for empty lines.
#[derive(Debug, Clone, PartialEq)]
pub enum InstructionType {
    Directive(DirectivePrototype),
    Command(CommandPrototype),
    None,
}

/// Directive instruction in it's simplest form.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectivePrototype {
    /// Directive name
    pub name: Option<String>,
    /// Directive arguments
    pub arguments: Option<Vec<String>>,
}
