//! Questions to the person at the terminal, such as confirming a step.
//!
//! A command that asks must also take the answer as a flag. Without a
//! terminal (a script, an agent, CI) or with `--json`, asking fails with the
//! usage exit code and names that flag, instead of waiting for an answer
//! that never comes.

use std::io::IsTerminal;

use dialoguer::{Confirm, Input};

use crate::cli::exit::Exit;
use crate::cli::output;

/// Whether someone can answer: standard input and standard error (where
/// questions are written) are a terminal.
pub fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Asks a yes-or-no question. `flag` is the flag that answers it instead,
/// such as `--yes`.
pub fn confirm(json: bool, question: &str, default: bool, flag: &str) -> Result<bool, Exit> {
    can_ask(interactive(), json, question, flag)?;
    Confirm::new()
        .with_prompt(question)
        .default(default)
        .interact()
        .map_err(|error| cancelled(json, &error))
}

/// Asks for a line of text. `flag` is the flag that answers it instead.
pub fn input(json: bool, question: &str, flag: &str) -> Result<String, Exit> {
    can_ask(interactive(), json, question, flag)?;
    Input::new()
        .with_prompt(question)
        .interact_text()
        .map_err(|error| cancelled(json, &error))
}

fn can_ask(interactive: bool, json: bool, question: &str, flag: &str) -> Result<(), Exit> {
    if interactive && !json {
        return Ok(());
    }
    Err(output::failure(
        json,
        Exit::Usage,
        "An answer is needed",
        &format!("{question} There is no terminal to ask in: pass {flag} instead."),
    ))
}

fn cancelled(json: bool, error: &dialoguer::Error) -> Exit {
    output::failure(json, Exit::Other, "Cancelled", &error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_only_at_a_terminal_without_json() {
        assert_eq!(can_ask(true, false, "Join?", "--yes"), Ok(()));
        assert_eq!(can_ask(false, false, "Join?", "--yes"), Err(Exit::Usage));
        assert_eq!(can_ask(true, true, "Join?", "--yes"), Err(Exit::Usage));
    }
}
