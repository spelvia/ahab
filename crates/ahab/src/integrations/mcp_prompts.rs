//! Optional host-owned prompt connections. Launch definitions come exclusively
//! from the daemon operator. Project files and HTTP clients cannot supply executables.
use crate::shared::error::{DaemonError, fail};
use futures_util::future::join_all;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use stubb::mcp::{Prompt, PromptClient};
use tokio::sync::{Mutex, OnceCell};

#[derive(Deserialize)]
struct Launch {
    command: PathBuf,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Default)]
pub struct Connections {
    launches: OnceCell<BTreeMap<String, Launch>>,
    clients: Mutex<BTreeMap<String, Arc<OnceCell<PromptClient>>>>,
}

fn unavailable(error: impl std::fmt::Display) -> DaemonError {
    fail(
        "command_unavailable",
        format!("Cannot retrieve the MCP prompt: {error}"),
    )
}

impl Connections {
    async fn launches(&self, cwd: &Path) -> Result<&BTreeMap<String, Launch>, DaemonError> {
        self.launches
            .get_or_try_init(|| async {
                let Some(path) = std::env::var_os("AHAB_MCP_PROMPT_CONFIG") else {
                    return Ok(BTreeMap::new());
                };
                let data = tokio::fs::read(path).await.map_err(unavailable)?;
                let mut projects: BTreeMap<PathBuf, BTreeMap<String, Launch>> =
                    serde_json::from_slice(&data).map_err(unavailable)?;
                Ok(projects.remove(cwd).unwrap_or_default())
            })
            .await
    }

    async fn client(
        &self,
        cwd: &Path,
        server: &str,
    ) -> Result<Arc<OnceCell<PromptClient>>, DaemonError> {
        let launch = self
            .launches(cwd)
            .await?
            .get(server)
            .ok_or_else(|| unavailable("server unavailable in this project"))?;
        if !launch.command.is_absolute() {
            return Err(unavailable("configure an absolute executable path"));
        }
        let cell = self
            .clients
            .lock()
            .await
            .entry(server.to_owned())
            .or_default()
            .clone();
        cell.get_or_try_init(|| {
            PromptClient::stdio(server.into(), &launch.command, &launch.args, cwd)
        })
        .await
        .map_err(unavailable)?;
        Ok(cell)
    }

    pub async fn list(&self, cwd: &Path) -> Result<Vec<Prompt>, DaemonError> {
        let servers = self.launches(cwd).await?;
        // A slow or offline prompt server should not hide other servers' commands.
        let names: Vec<_> = servers.keys().collect();
        let mut prompts = Vec::new();
        for batch in names.chunks(4) {
            let mut requests = Vec::new();
            for name in batch {
                requests.push(self.list_server(cwd, name));
            }
            for (server, result) in batch.iter().zip(join_all(requests).await) {
                match result {
                    Ok(page) => prompts.extend(page),
                    Err(error) => tracing::warn!(%server, %error, "MCP prompt discovery failed"),
                }
            }
        }
        Ok(prompts)
    }

    async fn list_server(&self, cwd: &Path, server: &str) -> Result<Vec<Prompt>, DaemonError> {
        let cell = self.client(cwd, server).await?;
        let client = cell.get().ok_or_else(|| unavailable("connection closed"))?;
        let page = client.prompts().await.map_err(unavailable);
        if page.is_err() {
            let mut clients = self.clients.lock().await;
            if clients
                .get(server)
                .is_some_and(|current| Arc::ptr_eq(current, &cell))
            {
                clients.remove(server);
            }
        }
        page
    }

    pub async fn expand(
        &self,
        cwd: &Path,
        server: &str,
        name: &str,
        args: &BTreeMap<String, String>,
    ) -> Result<String, DaemonError> {
        let cell = self.client(cwd, server).await?;
        let client = cell.get().ok_or_else(|| unavailable("connection closed"))?;
        let prompt = client
            .prompts()
            .await
            .map_err(unavailable)?
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| unavailable("prompt was removed"))?;
        let fields = prompt.arguments.unwrap_or_default();
        if fields.iter().any(|field| {
            field.required && args.get(&field.name).is_none_or(|v| v.trim().is_empty())
        }) || args
            .keys()
            .any(|key| !fields.iter().any(|field| &field.name == key))
        {
            return Err(unavailable(
                "prompt arguments have changed; check the required fields",
            ));
        }
        let messages = client.get(name, args).await.map_err(unavailable)?;
        // Providers accept a single user turn. Preserve the message order,
        // role and block type when embedding the template in that turn.
        serde_json::to_string(&messages).map_err(unavailable)
    }
}
