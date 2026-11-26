//! The command tree: every command `ahab` has, and where each one runs.
//! `up` runs Ahab on this computer (`up.rs`); the client commands that
//! arrive with the modules talk to a running server.

use std::io::{ErrorKind, Write};
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use crate::cli::client::Client;
use crate::cli::exit::Exit;
use crate::cli::lifecycle;
use crate::cli::output;
use crate::cli::remote;
use crate::cli::setup::{self, Setup};
use crate::cli::up::{self, Up};
use crate::config;
use crate::modules::clients::client::cli::{self as clients, ClientsArgs};
use crate::modules::clients::pairing::cli::{self as pairing, LoginArgs};
use crate::modules::machines::identity::store as identity;
use crate::modules::machines::machine::cli::{self as machines, MachinesArgs, MachinesCommand};
use crate::modules::notifications::notification::cli::{self as notifications, NotificationsArgs};
use crate::modules::platform::setting::cli::{self as settings, SettingsArgs};
use crate::modules::plugins::command::cli::{self as plugin_commands, RunArgs};
use crate::modules::plugins::plugin::cli::{self as plugins, PluginsArgs};
use crate::modules::projects::project::cli::{self as projects, OpenArgs, ProjectsArgs};
use crate::modules::sessions::session::cli::{self as sessions, SessionsArgs};
use crate::modules::sessions::terminal;
use crate::modules::tools::problems::cli::{self as problems, CheckArgs, ProblemsArgs};
use crate::modules::tools::registry::cli::{self as lsp, LspArgs};

#[derive(Debug, Parser)]
#[command(
    name = "ahab",
    version,
    about = "Ahab: run the server, and work with it from the command line",
    arg_required_else_help = true
)]
struct Cli {
    /// The Ahab server client commands talk to, such as
    /// <https://ahab.example.internal>. Default: the server this computer runs.
    #[arg(long, global = true, env = "AHAB_SERVER")]
    server: Option<String>,
    /// Print the API's JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start Ahab on this computer.
    Up(Box<Up>),
    /// Stop Ahab in the background; it no longer starts at login.
    Down {
        /// Also delete the service file.
        #[arg(long)]
        remove: bool,
    },
    /// What runs on this computer, and whether it answers.
    Status,
    /// Ahab's logs, from its service.
    Logs {
        /// Keep showing new lines.
        #[arg(long, short)]
        follow: bool,
        /// How many lines to show first.
        #[arg(long, short = 'n', default_value_t = 100)]
        lines: u32,
    },
    /// Join this computer to an existing server, as a machine.
    Setup(Setup),
    /// Stop being a machine of the server this computer joined: its daemon
    /// stops, and the server is forgotten here (the machine key stays).
    Leave {
        /// Do not ask first.
        #[arg(long, short)]
        yes: bool,
    },
    /// The computers that run a daemon. Alone, it lists them.
    #[command(visible_alias = "machine")]
    Machines(MachinesArgs),
    /// Open a folder in Ahab, like `code .`: its project, or a new one. The
    /// browser it opens is paired. `--no-folder` opens Ahab itself.
    Open(OpenArgs),
    /// Pair this command line with a server: it asks, and a paired client
    /// approves (or pass a code from Add client).
    Login(LoginArgs),
    /// Revoke this command line's client on a server and forget its login.
    Logout {
        /// The server; default: `--server`, `AHAB_SERVER`, or this computer's.
        server: Option<String>,
    },
    /// Clients paired with the server: browsers, phones, command lines and
    /// tokens. Alone, it lists them.
    // `devices`, their former name, still works for existing scripts.
    #[command(visible_alias = "client", alias = "devices", alias = "device")]
    Clients(ClientsArgs),
    /// The inbox: agents that finished, need you or stopped, machines
    /// offline, a newer Ahab.
    #[command(visible_alias = "inbox")]
    Notifications(NotificationsArgs),
    /// Projects: what you work on. Alone, it lists them.
    #[command(visible_alias = "project")]
    Projects(ProjectsArgs),
    /// Sessions: work with an agent in a project. Alone, it lists them.
    #[command(visible_alias = "session")]
    Sessions(SessionsArgs),
    /// Plugins: what extends Ahab. Alone, it lists them.
    #[command(visible_alias = "plugin")]
    Plugins(PluginsArgs),
    /// Ahab's own settings, such as the editor's: `ahab settings show
    /// editor`, `ahab settings set editor fontSize 14`.
    Settings(SettingsArgs),
    /// Language tools on a machine: `ahab lsp list`, `install`,
    /// `update`, `remove`, `status`.
    Lsp(LspArgs),
    /// A project's problems, from its language servers, linters and checks.
    Problems(ProblemsArgs),
    /// Run a project's check commands now; exits 1 when errors remain.
    Check(CheckArgs),
    /// Run a plugin's command: `ahab run docker.containers.list all=true`.
    /// A plugin's ID alone lists its commands.
    Run(RunArgs),
    /// Keep this machine's terminals (started by the daemon).
    #[command(hide = true)]
    TerminalHost {
        /// The Unix socket to serve on.
        #[arg(long)]
        socket: std::path::PathBuf,
    },
    /// Print the shell completion script for `ahab`.
    ///
    /// For example, for zsh: `ahab completions zsh > ~/.zfunc/_ahab`.
    Completions {
        /// The shell to complete in.
        shell: Shell,
    },
}

/// Runs the command line and returns its exit code. Wrong usage exits with
/// code 2, through clap.
pub fn run() -> ExitCode {
    // In development the server's settings come from .env; real environment
    // variables win.
    let _ = dotenvy::dotenv();
    // Two TLS backends are in the build (reqwest's and the database
    // driver's); choose one for the whole process before anything connects.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let cli = Cli::parse();
    match cli.command {
        Command::Up(command) => up::run(*command, cli.json),
        Command::Down { remove } => lifecycle::down(remove, cli.json),
        Command::Status => lifecycle::status(cli.json),
        Command::Logs { follow, lines } => lifecycle::logs(follow, lines, cli.json),
        Command::Setup(command) => {
            let json = cli.json;
            let start = !command.no_start;
            let joined = match runtime(false) {
                Ok(runtime) => runtime.block_on(setup::run(command, json)),
                Err(error) => Err(output::failure(json, Exit::Other, "Cannot start", &error)),
            };
            match joined {
                // As `ahab up` would: in the background, after the runtime
                // above is gone.
                Ok(()) if start => up::start_after_setup(json),
                Ok(()) => ExitCode::SUCCESS,
                Err(exit) => exit.into(),
            }
        }
        Command::Leave { yes } => setup::leave(yes, cli.json),
        Command::Machines(command) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                match command.command {
                    Some(MachinesCommand::Add {
                        destination: Some(destination),
                        name,
                        server_url,
                        yes,
                        no_start,
                    }) => {
                        let add = remote::AddMachine {
                            destination,
                            name,
                            server_url,
                            yes,
                            no_start,
                        };
                        remote::add_machine(&client, add).await
                    }
                    _ => machines::run(&client, command).await,
                }
            })
        }
        Command::Open(args) => {
            let fingerprint = local_fingerprint();
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                projects::open(&client, args, fingerprint).await
            })
        }
        Command::Login(args) => {
            let json = cli.json;
            let over_ssh = args.server.starts_with("ssh://");
            match runtime(false) {
                Ok(runtime) if over_ssh => match runtime.block_on(remote::login(&args, json)) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(exit) => exit.into(),
                },
                Ok(runtime) => match runtime.block_on(pairing::login(args, json)) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(exit) => exit.into(),
                },
                Err(error) => output::failure(json, Exit::Other, "Cannot start", &error).into(),
            }
        }
        Command::Logout { server } => {
            let json = cli.json;
            let server = server
                .or(cli.server)
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            match runtime(false) {
                Ok(runtime) => match runtime.block_on(pairing::logout(&server, json)) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(exit) => exit.into(),
                },
                Err(error) => output::failure(json, Exit::Other, "Cannot start", &error).into(),
            }
        }
        Command::Notifications(command) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                notifications::run(&client, command).await
            })
        }
        Command::Clients(command) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                clients::run(&client, command).await
            })
        }
        Command::Projects(command) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                projects::run(&client, command).await
            })
        }
        Command::Sessions(command) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                sessions::run(&client, command).await
            })
        }
        Command::Plugins(command) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                plugins::run(&client, command).await
            })
        }
        Command::Settings(command) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                settings::run(&client, command).await
            })
        }
        Command::Lsp(args) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                lsp::run(&client, args).await
            })
        }
        Command::Problems(args) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                problems::problems(&client, args).await
            })
        }
        Command::Check(args) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                problems::check(&client, args).await
            })
        }
        Command::Run(args) => {
            let server = cli
                .server
                .unwrap_or_else(|| config::local_server_url(&config::process_env));
            client(&server, cli.json, |client| async move {
                plugin_commands::run(&client, args).await
            })
        }
        Command::TerminalHost { socket } => match runtime(true) {
            Ok(runtime) => match runtime.block_on(terminal::daemon::serve_host(&socket)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("The terminal host stopped: {error}");
                    ExitCode::FAILURE
                }
            },
            Err(error) => {
                eprintln!("Cannot start the terminal host: {error}");
                ExitCode::FAILURE
            }
        },
        Command::Completions { shell } => completions(shell),
    }
}

/// This computer's machine key fingerprint, when it has a key; never creates
/// one.
fn local_fingerprint() -> Option<String> {
    let paths = config::paths::Paths::resolve(&config::process_env).ok()?;
    let file = paths.data_dir.join("machine.key");
    if !file.exists() {
        return None;
    }
    identity::load_or_create(&file)
        .ok()
        .map(|key| key.public_key().fingerprint().to_string())
}

/// Prints the completion script for `shell`.
fn completions(shell: Shell) -> ExitCode {
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut completable(), "ahab", &mut script);
    match std::io::stdout().write_all(&script) {
        // `ahab completions zsh | head` closes the pipe early: that is fine.
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: cannot write the completion script: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The command tree without hidden commands, which `clap_complete` would
/// otherwise offer: the same options and the visible subcommands.
fn completable() -> clap::Command {
    let cli = Cli::command();
    clap::Command::new("ahab")
        .version(env!("CARGO_PKG_VERSION"))
        .args(cli.get_arguments().cloned())
        .subcommands(
            cli.get_subcommands()
                .filter(|command| !command.is_hide_set())
                .cloned(),
        )
}

/// A tokio runtime: one with worker threads for the server, a single thread
/// for everything else.
pub fn runtime(multi_thread: bool) -> Result<tokio::runtime::Runtime, String> {
    let mut builder = if multi_thread {
        tokio::runtime::Builder::new_multi_thread()
    } else {
        tokio::runtime::Builder::new_current_thread()
    };
    builder
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
}

/// Runs a client command against the server at `server`, with this
/// command line's credential for it.
fn client<F, Fut>(server: &str, json: bool, command: F) -> ExitCode
where
    F: FnOnce(Client) -> Fut,
    Fut: Future<Output = Result<(), Exit>>,
{
    let runtime = match runtime(false) {
        Ok(runtime) => runtime,
        Err(error) => return output::failure(json, Exit::Other, "Cannot start", &error).into(),
    };
    let done = runtime.block_on(async {
        let client = Client::authenticated(server, json)?;
        command(client).await
    });
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(exit) => exit.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parser_is_consistent() {
        Cli::command().debug_assert();
    }

    fn machines(args: &[&str]) -> Option<MachinesCommand> {
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Command::Machines(args) => args.command,
            other => panic!("not machines: {other:?}"),
        }
    }

    #[test]
    fn parses_the_machines_commands_like_other_clis() {
        assert!(machines(&["ahab", "machines"]).is_none());
        assert!(matches!(
            machines(&["ahab", "machine", "ls"]),
            Some(MachinesCommand::List)
        ));
        assert!(matches!(
            machines(&["ahab", "machines", "rm", "build-box", "-y"]),
            Some(MachinesCommand::Remove { yes: true, .. })
        ));
        assert!(matches!(
            machines(&["ahab", "machines", "edit", "atlas", "--name", "home"]),
            Some(MachinesCommand::Edit { name: Some(name), .. }) if name == "home"
        ));
        assert!(Cli::try_parse_from(["ahab", "machines", "edit"]).is_err());
    }

    #[test]
    fn devices_still_works_for_clients_but_is_not_shown() {
        for name in ["clients", "client", "devices", "device"] {
            let cli = Cli::try_parse_from(["ahab", name, "approve", "7F3K"]).unwrap();
            assert!(
                matches!(cli.command, Command::Clients(_)),
                "{name} is not clients"
            );
        }
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("clients"));
        assert!(!help.contains("devices"));
    }

    #[test]
    fn completions_offer_the_visible_commands_only() {
        let mut script = Vec::new();
        clap_complete::generate(Shell::Bash, &mut completable(), "ahab", &mut script);
        let script = String::from_utf8(script).unwrap();
        for command in [
            "up",
            "--foreground",
            "--listen-addr",
            "machines",
            "setup",
            "down",
            "status",
            "logs",
            "edit",
            "show",
            "completions",
            "open",
            "projects",
            "--server",
            "--json",
        ] {
            assert!(script.contains(command), "{command} is missing");
        }
    }
}
