//! Some commonly used functionality.

use std::cmp::min;
use std::collections::HashMap;

use crate::machine::error::{Error, Result};

/// Returns a map of data about the context in which the program is running,
/// as well as about the program itself.
pub(crate) fn get_program_metadata() -> HashMap<String, String> {
    let mut output = HashMap::new();
    // output.insert("os".to_string(), crate::TARGET_OS.to_string());

    #[cfg(feature = "machine_sysinfo")]
    output.extend(get_system_info());

    output
}

//TODO use the `whoami` crate to include information about the os and whatnot
#[cfg(feature = "machine_sysinfo")]
/// Returns a map of system information data.
pub(crate) fn get_system_info() -> HashMap<String, String> {
    use sysinfo::{ProcessExt, SystemExt};
    let mut output = HashMap::new();
    let mut system = sysinfo::System::new_all();
    system.refresh_all();
    let current_process = system.process(sysinfo::get_current_pid().unwrap()).unwrap();

    // output.insert("sysinfo.system.os".to_string(), format!("{}", system));
    output.insert(
        "sysinfo.system.total_memory".to_string(),
        format!("{}", system.total_memory()),
    );
    output.insert(
        "sysinfo.system.used_memory".to_string(),
        format!("{}", system.used_memory()),
    );
    output.insert(
        "sysinfo.process.memory".to_string(),
        format!("{}", current_process.memory()),
    );
    output.insert(
        "sysinfo.process.virtual_memory".to_string(),
        format!("{}", current_process.virtual_memory()),
    );
    output.insert(
        "sysinfo.process.cpu_usage".to_string(),
        format!("{}", current_process.cpu_usage()),
    );

    output
}
