//! Bounded cross-platform child-process supervision for Loom runtimes.

mod command;
mod error;
mod isolation;
mod managed;
mod model;
mod path;
mod runner;
mod spawn;
#[cfg(windows)]
mod windows_spawn;

pub use error::ProcessError;
pub use managed::{ManagedChild, ManagedChildPipes};
pub use model::{ProcessLimits, ProcessSpec, SupervisedOutput};
pub use path::executable_path_within;
pub use runner::{run_with_input, run_with_input_cancellable};

#[cfg(test)]
mod tests;
#[cfg(all(test, windows))]
mod windows_spawn_tests;
