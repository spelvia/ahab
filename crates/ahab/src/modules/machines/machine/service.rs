//! What can be done with machines.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::machines::machine::model::{Machine, MachineName, MachineState, PublicKey};
use crate::modules::machines::machine::store::MachineRepository;
use crate::shared::error::ServiceError;
use crate::shared::time::Clock;

pub struct MachineService {
    repo: Arc<dyn MachineRepository>,
    clock: Arc<dyn Clock>,
}

fn not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "machine_not_found",
        message: "There is no such machine.".to_owned(),
    }
}

impl MachineService {
    pub fn new(repo: Arc<dyn MachineRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    /// Every machine: the local one first, then by name.
    pub async fn list(&self) -> Result<Vec<Machine>, ServiceError> {
        let mut machines = self.repo.list().await?;
        machines.sort_by(|a, b| {
            b.is_local
                .cmp(&a.is_local)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(machines)
    }

    pub async fn get(&self, id: Uuid) -> Result<Machine, ServiceError> {
        self.repo.get(id).await?.ok_or_else(not_found)
    }

    /// The machine `reference` names: its name or its ID.
    pub async fn find(&self, reference: &str) -> Result<Machine, ServiceError> {
        if let Ok(id) = Uuid::parse_str(reference) {
            return self.get(id).await;
        }
        let name = MachineName::parse(reference).map_err(|_| not_found())?;
        self.repo.find_by_name(&name).await?.ok_or_else(not_found)
    }

    /// The machine with this key fingerprint (`SHA256:…`), such as the one a
    /// command line runs on.
    pub async fn find_by_fingerprint(&self, fingerprint: &str) -> Result<Machine, ServiceError> {
        self.repo
            .list()
            .await?
            .into_iter()
            .find(|machine| machine.public_key.fingerprint().to_string() == fingerprint)
            .ok_or_else(not_found)
    }

    pub async fn rename(&self, id: Uuid, name: &str) -> Result<Machine, ServiceError> {
        let name = MachineName::parse(name)?;
        let mut machine = self.get(id).await?;
        if machine.name == name {
            return Ok(machine);
        }
        if self.repo.find_by_name(&name).await?.is_some() {
            return Err(ServiceError::Conflict {
                code: "machine_name_taken",
                message: format!("Another machine is called {name}."),
            });
        }
        machine.name = name;
        self.repo.update(&machine).await?;
        Ok(machine)
    }

    /// Removes a machine; its key stops working. The local machine cannot be
    /// removed: it would be created again at the next start.
    pub async fn remove(&self, id: Uuid) -> Result<(), ServiceError> {
        let machine = self.get(id).await?;
        if machine.is_local {
            return Err(ServiceError::Conflict {
                code: "machine_is_local",
                message: "This is the server's own machine. Turn its daemon off with \
                          `ahab up --daemon=false` instead."
                    .to_owned(),
            });
        }
        if self.repo.delete(id).await? {
            Ok(())
        } else {
            Err(not_found())
        }
    }

    /// A machine that joins this server: approved at once (`approved`, with a
    /// one-time code) or pending. The name comes from `name`, else the
    /// hostname, numbered when taken.
    pub async fn join(
        &self,
        public_key: PublicKey,
        name: Option<MachineName>,
        hostname: &str,
        approved: bool,
    ) -> Result<Machine, ServiceError> {
        let name = self
            .free_name(name.unwrap_or_else(|| MachineName::from_hostname(hostname)))
            .await?;
        let now = self.clock.now();
        let machine = if approved {
            Machine::joined(name, public_key, now)
        } else {
            Machine::pending(name, public_key, now)
        };
        self.repo.insert(&machine).await?;
        Ok(machine)
    }

    /// Deletes a machine outright, local or not: for undoing a join that lost
    /// a race. People remove machines with [`Self::remove`].
    pub async fn forget(&self, id: Uuid) -> Result<(), ServiceError> {
        self.repo.delete(id).await?;
        Ok(())
    }

    /// Approves a pending machine, so it may connect.
    pub async fn approve(&self, id: Uuid) -> Result<Machine, ServiceError> {
        let mut machine = self.get(id).await?;
        if !machine.approve(self.clock.now()) {
            return Err(ServiceError::Conflict {
                code: "machine_not_pending",
                message: format!("{} is already approved.", machine.name),
            });
        }
        self.repo.update(&machine).await?;
        Ok(machine)
    }

    /// Removes pending machines that waited longer than `for_at_most`.
    pub async fn expire_pending(&self, for_at_most: time::Duration) -> Result<u64, ServiceError> {
        Ok(self
            .repo
            .delete_pending_before(self.clock.now() - for_at_most)
            .await?)
    }

    /// How many machines wait for approval.
    pub async fn pending_count(&self) -> Result<usize, ServiceError> {
        Ok(self
            .repo
            .list()
            .await?
            .iter()
            .filter(|machine| machine.state == MachineState::Pending)
            .count())
    }

    /// The machine with this key, if any.
    pub async fn find_by_key(&self, key: &PublicKey) -> Result<Option<Machine>, ServiceError> {
        Ok(self.repo.find_by_key(key).await?)
    }

    /// The daemon connected and reported what it runs on.
    pub async fn record_connection(
        &self,
        id: Uuid,
        os: &str,
        arch: &str,
        version: &str,
    ) -> Result<Machine, ServiceError> {
        let mut machine = self.get(id).await?;
        machine.os = Some(os.to_owned());
        machine.arch = Some(arch.to_owned());
        machine.version = Some(version.to_owned());
        machine.last_seen_at = Some(self.clock.now());
        self.repo.update(&machine).await?;
        Ok(machine)
    }

    /// The machine was seen just now: its daemon is connected, or just left.
    pub async fn touch(&self, id: Uuid) -> Result<(), ServiceError> {
        let mut machine = self.get(id).await?;
        machine.last_seen_at = Some(self.clock.now());
        self.repo.update(&machine).await?;
        Ok(())
    }

    /// The server's own machine, created on first start with a name from the
    /// hostname. A new key (the key file was replaced) replaces the old one.
    pub async fn ensure_local(
        &self,
        public_key: PublicKey,
        hostname: &str,
    ) -> Result<Machine, ServiceError> {
        if let Some(mut machine) = self.repo.find_local().await? {
            if machine.public_key != public_key {
                machine.public_key = public_key;
                self.repo.update(&machine).await?;
            }
            return Ok(machine);
        }
        let name = self.free_name(MachineName::from_hostname(hostname)).await?;
        let machine = Machine::local(name, public_key, self.clock.now());
        self.repo.insert(&machine).await?;
        Ok(machine)
    }

    /// `name`, or `name-2`, `name-3`… when it is taken.
    async fn free_name(&self, name: MachineName) -> Result<MachineName, ServiceError> {
        if self.repo.find_by_name(&name).await?.is_none() {
            return Ok(name);
        }
        for n in 2..1000 {
            let numbered = name.numbered(n);
            if self.repo.find_by_name(&numbered).await?.is_none() {
                return Ok(numbered);
            }
        }
        Err(ServiceError::Internal(format!(
            "no free name for {name} after 1000 tries"
        )))
    }
}
