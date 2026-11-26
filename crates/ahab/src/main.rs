//! The `ahab` binary; the command line is in `lib.rs`.

use std::process::ExitCode;

fn main() -> ExitCode {
    ahab::cli::run()
}
