//! The composition root: builds the services and starts the server.

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::sync::Arc;

use std::time::Duration;

use axum::Router;
use tokio::net::TcpListener;

use super::access::Access;
use super::hosts::AllowedHosts;
use crate::config::Settings;
use crate::modules::clients::client::store::DbClientRepository;
use crate::modules::clients::pairing::store::DbPairingRepository;
use crate::modules::clients::{ClientService, PairingService};
use crate::modules::machines::agents::store::{DbAddedAgentRepository, DbAgentRepository};
use crate::modules::machines::enrollment::store::DbJoinCodeRepository;
use crate::modules::machines::identity::store::{self as identity, KeyFileError};
use crate::modules::machines::machine::store::DbMachineRepository;
use crate::modules::machines::{
    AgentService, EnrollmentService, MachineKey, MachineService, MetricsService, Presence,
    UsageService,
};
use crate::modules::notifications::notification::store::DbNotificationRepository;
use crate::modules::notifications::push::store::DbPushRepository;
use crate::modules::notifications::{
    Intercept, NotificationService, PluginNotifications, PushService,
};
use crate::modules::platform::database::store as database;
use crate::modules::platform::feature::store::DbFeatureRepository;
use crate::modules::platform::setting::store::DbCoreSettingRepository;
use crate::modules::platform::{CoreSettingService, FeatureService};
use crate::modules::plugins::plugin::store::DbPluginRepository;
use crate::modules::plugins::setting::store::DbSettingRepository;
use crate::modules::plugins::{
    CommandService, PluginHostService, PluginPaths, PluginService, SettingService,
};
use crate::modules::projects::layout::store::DbLayoutRepository;
use crate::modules::projects::project::store::DbProjectRepository;
use crate::modules::projects::{
    FileService, GitService, LayoutService, ProjectService, SearchService,
};
use crate::modules::sessions::chat::store::{
    ChatRepository, DbChatRepository, DbModelCacheRepository,
};
use crate::modules::sessions::session::store::DbSessionRepository;
use crate::modules::sessions::terminal::store::DbTerminalRepository;
use crate::modules::sessions::{AttachmentService, ChatService, SessionService, TerminalService};
use crate::modules::tools::activity::store::DbActivityRepository;
use crate::modules::tools::problems::store::DbProblemRepository;
use crate::modules::tools::setting::store::DbToolSettingRepository;
use crate::modules::tools::{
    LanguageServerService, ProblemService, RepositoryActivityService, ToolService,
    ToolSettingService,
};
use crate::server::router;
use crate::server::state::AppState;
use crate::server::web::WebApp;
use crate::shared::database::Db;
use crate::shared::time::{Clock, SystemClock};

#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("cannot connect to the database: {0}")]
    Database(sqlx::Error),
    #[error("cannot apply database migrations: {0}")]
    Migrations(sqlx::migrate::MigrateError),
    #[error("{0}")]
    Key(KeyFileError),
    #[error(transparent)]
    PushKey(crate::modules::notifications::push::store::KeyFileError),
    #[error("cannot set up the local credential: {0}")]
    Credential(crate::shared::error::ServiceError),
    #[error("cannot recover chat submissions: {0}")]
    Submissions(crate::shared::error::RepoError),
    #[error("cannot listen on {addr}: {source}")]
    Listen {
        addr: String,
        source: std::io::Error,
    },
    #[error("the server stopped: {0}")]
    Serve(std::io::Error),
}

/// The services, on `db`, with the real clock and the server's key.
pub fn build_state(db: Db, server_key: MachineKey, plugins: PluginPaths) -> AppState {
    build_state_with(db, Arc::new(SystemClock), server_key, plugins)
}

/// The services, on `db`, with `clock` (tests fix the time).
pub fn build_state_with(
    db: Db,
    clock: Arc<dyn Clock>,
    server_key: MachineKey,
    plugin_paths: PluginPaths,
) -> AppState {
    let machines = Arc::new(MachineService::new(
        Arc::new(DbMachineRepository::new(db.clone())),
        Arc::clone(&clock),
    ));
    let presence = Arc::new(Presence::new(
        Arc::clone(&machines),
        Arc::new(server_key),
        crate::daemon::dispatch::METHODS,
    ));
    let projects = Arc::new(ProjectService::new(
        Arc::new(DbProjectRepository::new(db.clone())),
        Arc::clone(&machines),
        Arc::clone(&presence),
        Arc::clone(&clock),
    ));
    let agents = Arc::new(AgentService::new(
        Arc::new(DbAgentRepository::new(db.clone())),
        Arc::new(DbAddedAgentRepository::new(db.clone())),
        Arc::clone(&machines),
        Arc::clone(&presence),
        Arc::clone(&clock),
    ));
    let usage = Arc::new(UsageService::new(
        Arc::clone(&machines),
        Arc::clone(&presence),
    ));
    let metrics = Arc::new(MetricsService::new(
        Arc::clone(&machines),
        Arc::clone(&presence),
    ));
    let chat_repo: Arc<dyn ChatRepository> = Arc::new(DbChatRepository::new(db.clone()));
    let sessions = Arc::new(SessionService::new(
        Arc::new(DbSessionRepository::new(db.clone())),
        Arc::clone(&chat_repo),
        Arc::clone(&projects),
        Arc::clone(&clock),
    ));
    let files = Arc::new(FileService::new(
        Arc::clone(&projects),
        Arc::clone(&machines),
        Arc::clone(&presence),
    ));
    let chats = Arc::new(ChatService::new(
        chat_repo,
        Arc::clone(&projects),
        Arc::clone(&sessions),
        Arc::clone(&agents),
        Arc::clone(&presence),
        Arc::clone(&files),
        Arc::clone(&clock),
        Arc::new(DbModelCacheRepository::new(db.clone())),
    ));
    let terminals = Arc::new(TerminalService::new(
        Arc::new(DbTerminalRepository::new(db.clone())),
        Arc::clone(&sessions),
        Arc::clone(&projects),
        Arc::clone(&presence),
        Arc::clone(&clock),
    ));
    let plugins = Arc::new(PluginService::new(
        Arc::new(DbPluginRepository::new(db.clone())),
        Arc::clone(&machines),
        plugin_paths,
        Arc::clone(&clock),
    ));
    let settings = Arc::new(SettingService::new(
        Arc::new(DbSettingRepository::new(db.clone())),
        Arc::clone(&plugins),
        Arc::clone(&projects),
        Arc::clone(&clock),
    ));
    let plugin_host = Arc::new(PluginHostService::new(
        Arc::clone(&plugins),
        Arc::clone(&settings),
        Arc::clone(&machines),
        Arc::clone(&presence),
        Arc::clone(&clock),
    ));
    let commands = Arc::new(CommandService::new(
        Arc::clone(&plugins),
        Arc::clone(&settings),
        Arc::clone(&plugin_host),
        Arc::clone(&projects),
        Arc::clone(&presence),
    ));
    let clients = Arc::new(ClientService::new(
        Arc::new(DbClientRepository::new(db.clone())),
        Arc::clone(&clock),
        Arc::clone(presence.bus()),
    ));
    let pairings = Arc::new(PairingService::new(
        Arc::new(DbPairingRepository::new(db.clone())),
        Arc::clone(&clients),
        Arc::clone(&clock),
        Arc::clone(presence.bus()),
    ));
    let notifications = Arc::new(NotificationService::new(
        Arc::new(DbNotificationRepository::new(db.clone())),
        Arc::clone(&clock),
    ));
    let plugin_notifications = Arc::new(PluginNotifications::new(
        Arc::clone(&notifications),
        Arc::clone(&plugins),
        Arc::clone(&plugin_host),
        Arc::clone(&settings),
        Arc::clone(&projects),
        Arc::clone(&presence),
    ));
    let intercept: Arc<dyn Intercept> = plugin_notifications.clone();
    notifications.intercept_with(&intercept);
    let push = Arc::new(PushService::new(
        Arc::new(DbPushRepository::new(db.clone())),
        Arc::clone(&clients),
        Arc::clone(&notifications),
        Arc::clone(&clock),
    ));
    let features = Arc::new(FeatureService::new(
        Arc::new(DbFeatureRepository::new(db.clone())),
        Arc::clone(&clock),
    ));
    let core_settings = Arc::new(CoreSettingService::new(
        Arc::new(DbCoreSettingRepository::new(db.clone())),
        Arc::clone(&clock),
    ));
    let tool_settings = Arc::new(ToolSettingService::new(
        Arc::new(DbToolSettingRepository::new(db.clone())),
        Arc::clone(&clock),
    ));
    let tools = Arc::new(ToolService::new(
        Arc::clone(&tool_settings),
        Arc::clone(&plugins),
        Arc::clone(&machines),
        Arc::clone(&presence),
    ));
    let repository_activity = Arc::new(RepositoryActivityService::new(
        Arc::new(DbActivityRepository::new(db.clone())),
        Arc::clone(&projects),
        Arc::clone(&tool_settings),
        Arc::clone(&presence),
        Arc::clone(&clock),
    ));
    let language_servers = Arc::new(LanguageServerService::new(
        Arc::clone(&tools),
        Arc::clone(&projects),
        Arc::clone(&repository_activity),
        Arc::clone(&presence),
    ));
    let problems = Arc::new(ProblemService::new(
        Arc::new(DbProblemRepository::new(db.clone())),
        Arc::clone(&tools),
        Arc::clone(&tool_settings),
        Arc::clone(&repository_activity),
        Arc::clone(&projects),
        Arc::clone(&chats),
        Arc::clone(&presence),
        Arc::clone(&clock),
    ));
    AppState {
        tool_settings,
        tools,
        language_servers,
        problems,
        repository_activity,
        core_settings,
        allowed_hosts: Arc::default(),
        access: Arc::default(),
        clients,
        pairings,
        notifications,
        push,
        plugin_notifications,
        features,
        settings,
        commands,
        plugins,
        plugin_host,
        agents,
        usage,
        metrics,
        sessions,
        chats,
        terminals,
        attachments: Arc::new(AttachmentService::new(
            Arc::clone(&projects),
            Arc::clone(&presence),
        )),
        search: Arc::new(SearchService::new(
            Arc::clone(&projects),
            Arc::clone(&presence),
        )),
        git: Arc::new(GitService::new(
            Arc::clone(&projects),
            Arc::clone(&presence),
        )),
        files,
        layouts: Arc::new(LayoutService::new(
            Arc::new(DbLayoutRepository::new(db.clone())),
            Arc::clone(&projects),
            Arc::clone(&clock),
        )),
        enrollment: Arc::new(EnrollmentService::new(
            Arc::clone(&machines),
            Arc::new(DbJoinCodeRepository::new(db)),
            clock,
        )),
        presence,
        projects,
        machines,
    }
}

/// A server that is listening, not yet serving: `ahab up` starts the local
/// daemon in between.
pub struct Started {
    pub state: AppState,
    /// The address it listens on (the real port when 0 was asked for).
    pub addr: SocketAddr,
    listener: TcpListener,
    router: Router,
}

/// Connects to and migrates the database, builds the services, and starts
/// listening.
pub async fn start(settings: &Settings) -> Result<Started, StartupError> {
    let db = database::connect(&settings.database_url, 10)
        .await
        .map_err(StartupError::Database)?;
    database::migrate(&db)
        .await
        .map_err(StartupError::Migrations)?;
    tracing::info!(
        backend = database::backend(&settings.database_url),
        "database ready"
    );
    let server_key = identity::load_or_create(&settings.paths.data_dir.join("server.key"))
        .map_err(StartupError::Key)?;
    tracing::info!(fingerprint = %server_key.public_key().fingerprint(), "server key");
    let web = WebApp::from_settings(settings.web, settings.ui_dir.as_deref());
    let plugins = PluginPaths {
        store: settings.paths.data_dir.join("plugins").join("store"),
        node: settings.plugin_node.clone(),
    };
    let push_key = crate::modules::notifications::push::store::load_or_create_key(
        &settings.paths.data_dir.join("push.key"),
    )
    .map_err(StartupError::PushKey)?;
    let mut state = build_state(db, server_key, plugins);
    state.push.use_key(push_key);
    state
        .plugin_notifications
        .use_public_url(settings.public_url.clone());
    let public_host = settings
        .public_url
        .as_deref()
        .and_then(|url| url.split_once("://"))
        .map(|(_, rest)| rest.split('/').next().unwrap_or_default().to_owned());
    state.allowed_hosts = Arc::new(AllowedHosts::new(
        settings.allowed_hosts.iter().cloned().chain(public_host),
    ));
    state.access = Arc::new(Access {
        trusted_proxies: settings.trusted_proxies.clone(),
        allowed_origins: settings.allowed_origins.clone(),
        public_url: settings.public_url.clone(),
        allow_insecure_http: settings.allow_insecure_http,
        outward_url: outward_url(settings.listen_addr),
    });
    let credential = settings.paths.data_dir.join("credential");
    state
        .clients
        .ensure_local(&credential)
        .await
        .map_err(StartupError::Credential)?;
    let started = listen(state, settings.listen_addr, &web).await?;
    announce_first_run(&started, settings).await;
    // Only a real server asks for releases, and only when not turned off;
    // tests use `listen` alone.
    if settings.update.enabled {
        started.state.notifications.spawn_update_check(
            crate::modules::notifications::ReleaseCheck {
                url: settings.update.url.clone(),
                prereleases: settings.update.prereleases,
            },
        );
    }
    Ok(started)
}

/// Where another device reaches a server listening on `listen`: `None`
/// when it listens on loopback only. On every address, this computer's
/// address on its main network.
pub fn outward_url(listen: SocketAddr) -> Option<String> {
    let ip = listen.ip();
    if ip.is_loopback() {
        return None;
    }
    let ip = if ip.is_unspecified() {
        primary_address(ip.is_ipv4())?
    } else {
        ip
    };
    Some(format!("http://{}", SocketAddr::new(ip, listen.port())))
}

/// This computer's address on the network its default route leads to. No
/// packet is sent: connecting a UDP socket only picks the route.
fn primary_address(v4: bool) -> Option<std::net::IpAddr> {
    let (bind, probe) = if v4 {
        ("0.0.0.0:0", "192.0.2.1:9")
    } else {
        ("[::]:0", "[2001:db8::1]:9")
    };
    let socket = std::net::UdpSocket::bind(bind).ok()?;
    socket.connect(probe).ok()?;
    let address = socket.local_addr().ok()?.ip();
    (!address.is_unspecified() && !address.is_loopback()).then_some(address)
}

/// While no client but this computer is paired, every start prints a fresh
/// first-run link to the log, and to the terminal with a QR code.
async fn announce_first_run(started: &Started, settings: &Settings) {
    let offered = match started.state.pairings.first_run().await {
        Ok(Some(offered)) => offered,
        Ok(None) => return,
        Err(error) => {
            tracing::error!(%error, "cannot create the first-run pairing link");
            return;
        }
    };
    let base = settings
        .public_url
        .clone()
        .or_else(|| started.state.access.outward_url.clone())
        .unwrap_or_else(|| {
            let mut addr = started.addr;
            if addr.ip().is_unspecified() {
                addr.set_ip(std::net::Ipv4Addr::LOCALHOST.into());
            }
            format!("http://{addr}")
        });
    let link = format!("{base}/pair#{}", offered.secret);
    tracing::warn!(
        link = %link,
        "No client is paired yet. Open this link to pair your first one (once, within 24 hours)"
    );
    if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        eprintln!(
            "\nAhab is running at {base}. No client is paired yet. Open this link to pair your \
             first one (once, within 24 hours):\n\n  {link}\n"
        );
        if let Ok(code) = qrcode::QrCode::new(link.as_bytes()) {
            let picture = code
                .render::<qrcode::render::unicode::Dense1x2>()
                .quiet_zone(true)
                .build();
            eprintln!("{picture}\n");
        }
    }
}

/// Starts listening on `addr` with `state`.
pub async fn listen(
    state: AppState,
    addr: SocketAddr,
    web: &WebApp,
) -> Result<Started, StartupError> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| StartupError::Listen {
            addr: addr.to_string(),
            source,
        })?;
    let addr = listener
        .local_addr()
        .map_err(|source| StartupError::Listen {
            addr: addr.to_string(),
            source,
        })?;
    state
        .chats
        .recover_submissions()
        .await
        .map_err(StartupError::Submissions)?;
    tracing::info!(%addr, "listening");
    // Bring each daemon up to date as it connects: its projects, its
    // agents.
    state.projects.spawn_background();
    state.agents.spawn_background();
    state.chats.spawn_background();
    state
        .notifications
        .spawn_background(crate::modules::notifications::ChatSources {
            chats: Arc::clone(&state.chats),
            projects: Arc::clone(&state.projects),
            sessions: Arc::clone(&state.sessions),
            agents: Arc::clone(&state.agents),
        });
    state.push.spawn();
    state.plugin_notifications.spawn();
    state
        .notifications
        .spawn_machine_watch(crate::modules::notifications::Watched {
            chats: Arc::clone(&state.chats),
            terminals: Arc::clone(&state.terminals),
            machines: Arc::clone(&state.machines),
            presence: Arc::clone(&state.presence),
        });
    state.terminals.spawn_background(Arc::clone(&state.chats));
    state.plugin_host.spawn_background();
    state.problems.spawn_background();
    Ok(Started {
        router: router::router(state.clone(), web),
        state,
        addr,
        listener,
    })
}

/// How long open requests may finish after a stop is asked for. Event
/// streams from open web app tabs never finish on their own, so without a
/// limit a stop waits until the service manager kills Ahab.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

impl Started {
    /// Serves until `stop` completes, then gives open requests
    /// [`SHUTDOWN_GRACE`] to finish.
    pub async fn run(
        self,
        stop: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), StartupError> {
        let (stopping, stopped) = tokio::sync::oneshot::channel::<()>();
        let serve = axum::serve(
            self.listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            stop.await;
            let _ = stopping.send(());
        })
        .into_future();
        tokio::pin!(serve);
        tokio::select! {
            result = &mut serve => return result.map_err(StartupError::Serve),
            _ = stopped => {}
        }
        let Ok(result) = tokio::time::timeout(SHUTDOWN_GRACE, serve).await else {
            tracing::info!("closing the connections still open");
            return Ok(());
        };
        result.map_err(StartupError::Serve)
    }
}

/// Completes on Ctrl-C or SIGTERM.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "cannot listen for Ctrl-C");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::warn!(%error, "cannot listen for SIGTERM"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}
