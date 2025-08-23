//! Module implementing non-interactive, *autonomous* simulation runner
//! functionality.
//!
//! These simulation runs are *autonomous* in so much as they are not
//! supervised during execution. Instead they are spawned based on *experiment*
//! manifests.
//!
//! An *experiment* is defined in a `toml` structured data file, and describes
//! various aspects of an *autonomous* simulation run, or indeed a collection
//! of runs. Experiments can be used to run one or more simulation runs, with
//! additional rules given to each run, such as different start state,
//! end conditions, triggers for data export, and more.
