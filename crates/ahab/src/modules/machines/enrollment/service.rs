//! Joining: enrolling machines, approving them, and one-time codes.

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use uuid::Uuid;

use crate::modules::machines::enrollment::model::{
    JoinCode, MAX_PENDING, code_from_random, code_valid_for, enroll_message, format_code,
    hash_code, normalize_code, pending_for,
};
use crate::modules::machines::enrollment::store::JoinCodeRepository;
use crate::modules::machines::identity::model::verify;
use crate::modules::machines::machine::model::{Machine, MachineName, PublicKey};
use crate::modules::machines::machine::service::MachineService;
use crate::shared::error::{FieldError, ServiceError};
use crate::shared::time::Clock;

/// A machine asking to join.
#[derive(Debug, Clone)]
pub struct EnrollRequest {
    /// Base64 Ed25519 public key.
    pub public_key: String,
    /// Base64 signature of [`enroll_message`] for this host and key.
    pub signature: String,
    pub hostname: String,
    /// A one-time code, to be approved at once.
    pub code: Option<String>,
}

pub struct EnrollmentService {
    machines: Arc<MachineService>,
    codes: Arc<dyn JoinCodeRepository>,
    clock: Arc<dyn Clock>,
}

fn code_invalid() -> ServiceError {
    // One error for unknown, used and expired codes, so codes cannot be
    // probed.
    ServiceError::Forbidden {
        code: "join_code_invalid",
        message: "This code does not work: it may be wrong, used or expired. Ask for a new one."
            .to_owned(),
    }
}

impl EnrollmentService {
    pub fn new(
        machines: Arc<MachineService>,
        codes: Arc<dyn JoinCodeRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            machines,
            codes,
            clock,
        }
    }

    /// A machine asks to join a server it reached as `host`. With a valid
    /// code it is approved at once; otherwise it waits for approval. The same
    /// key asking again gets its machine back.
    pub async fn enroll(
        &self,
        request: &EnrollRequest,
        host: &str,
    ) -> Result<Machine, ServiceError> {
        let public_key = STANDARD
            .decode(&request.public_key)
            .ok()
            .and_then(|bytes| PublicKey::from_bytes(&bytes).ok())
            .ok_or_else(|| {
                ServiceError::Invalid(vec![FieldError::new(
                    "publicKey",
                    "Not an Ed25519 public key in base64.",
                )])
            })?;
        let signature = STANDARD.decode(&request.signature).unwrap_or_default();
        if !verify(
            &public_key,
            &enroll_message(host, &request.public_key),
            &signature,
        ) {
            return Err(ServiceError::Unauthorized {
                code: "signature_invalid",
                message: "The request is not signed by this key.".to_owned(),
            });
        }
        self.machines.expire_pending(pending_for()).await?;

        let code = match &request.code {
            Some(code) => Some(self.usable_code(code).await?),
            None => None,
        };
        if let Some(mut machine) = self.machines.find_by_key(&public_key).await? {
            if let Some(code) = code
                && machine.state == crate::modules::machines::MachineState::Pending
            {
                if !self.codes.mark_used(code.id, machine.id).await? {
                    return Err(code_invalid());
                }
                machine = self.machines.approve(machine.id).await?;
            }
            return Ok(machine);
        }

        if let Some(code) = code {
            let machine = self
                .machines
                .join(public_key, code.name.clone(), &request.hostname, true)
                .await?;
            if !self.codes.mark_used(code.id, machine.id).await? {
                // Another machine used the code first.
                self.machines.forget(machine.id).await?;
                return Err(code_invalid());
            }
            Ok(machine)
        } else {
            if self.machines.pending_count().await? >= MAX_PENDING {
                return Err(ServiceError::Conflict {
                    code: "too_many_pending",
                    message: "Too many machines wait for approval: approve or remove some \
                              first, or join with a one-time code."
                        .to_owned(),
                });
            }
            self.machines
                .join(public_key, None, &request.hostname, false)
                .await
        }
    }

    /// A joining machine checks whether it was approved. A rejected or
    /// expired request is gone.
    pub async fn status(&self, id: Uuid) -> Result<Machine, ServiceError> {
        self.machines.expire_pending(pending_for()).await?;
        self.machines
            .get(id)
            .await
            .map_err(|_| ServiceError::NotFound {
                code: "enrollment_not_found",
                message: "This request to join was rejected or expired: run `ahab setup` again."
                    .to_owned(),
            })
    }

    async fn usable_code(&self, code: &str) -> Result<JoinCode, ServiceError> {
        let normalized = normalize_code(code).ok_or_else(code_invalid)?;
        let found = self.codes.find_by_hash(&hash_code(&normalized)).await?;
        found
            .filter(|code| code.is_usable(self.clock.now()))
            .ok_or_else(code_invalid)
    }

    /// A new one-time code, shown once: the code itself, and its record.
    pub async fn create_code(
        &self,
        name: Option<&str>,
    ) -> Result<(JoinCode, String), ServiceError> {
        let name = name.map(MachineName::parse).transpose()?;
        let mut random = [0u8; 8];
        getrandom::fill(&mut random)
            .map_err(|error| ServiceError::Internal(format!("no randomness: {error}")))?;
        let code = code_from_random(&random);
        let now = self.clock.now();
        let join_code = JoinCode {
            id: Uuid::now_v7(),
            code_hash: hash_code(&code),
            name,
            expires_at: now + code_valid_for(),
            used_by: None,
            created_at: now,
        };
        self.codes.insert(&join_code).await?;
        Ok((join_code, format_code(&code)))
    }

    /// The codes that still work, newest first.
    pub async fn list_codes(&self) -> Result<Vec<JoinCode>, ServiceError> {
        let now = self.clock.now();
        let mut codes: Vec<JoinCode> = self
            .codes
            .list()
            .await?
            .into_iter()
            .filter(|code| code.is_usable(now))
            .collect();
        codes.sort_by_key(|code| std::cmp::Reverse(code.created_at));
        Ok(codes)
    }

    pub async fn delete_code(&self, id: Uuid) -> Result<(), ServiceError> {
        if self.codes.delete(id).await? {
            Ok(())
        } else {
            Err(ServiceError::NotFound {
                code: "join_code_not_found",
                message: "There is no such code.".to_owned(),
            })
        }
    }
}
