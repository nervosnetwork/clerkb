use crate::error::{Error, Result};
use ckb_jsonrpc_types::JsonBytes;
use ckb_types::{core::ScriptHashType, packed::Byte, H256};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Backend {
    pub ckb_rpc: String,
    pub ckb_indexer_rpc: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Credential {
    pub private_key: H256,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScriptConfig {
    pub code_hash: H256,
    pub hash_type: u8,
    pub tx_hash: H256,
    pub index: u32,
}

impl ScriptConfig {
    pub fn core_hash_type_string(&self) -> String {
        match self.core_hash_type() {
            ScriptHashType::Data => "data",
            ScriptHashType::Type => "type",
        }
        .to_string()
    }

    pub fn core_hash_type(&self) -> ScriptHashType {
        self.core_hash_type_safe().unwrap()
    }

    pub fn core_hash_type_safe(&self) -> Result<ScriptHashType> {
        ScriptHashType::try_from(Byte::new(self.hash_type)).map_err(|e| e.into())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Lock {
    pub identity_size: u8,
    pub block_interval_seconds: u32,
    pub data_info_offset: u32,
    pub aggregators: Vec<JsonBytes>,
    pub binary: ScriptConfig,
    pub verification_library: ScriptConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub backend: Backend,
    pub credential: Credential,
    pub lock: Lock,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.lock.binary.core_hash_type_safe().is_err() {
            return Err(Error::InvalidConfig(
                "lock.binary does not have a valid hash type!".to_string(),
            )
            .into());
        }
        if self
            .lock
            .verification_library
            .core_hash_type_safe()
            .is_err()
        {
            return Err(Error::InvalidConfig(
                "lock.verification_library does not have a valid hash type!".to_string(),
            )
            .into());
        }
        if self.lock.aggregators.len() > u16::max_value() as usize {
            return Err(Error::InvalidConfig("Too many aggregators!".to_string()).into());
        }
        for aggregator in &self.lock.aggregators {
            if aggregator.len() != self.lock.identity_size as usize {
                return Err(Error::InvalidConfig(
                    "Invalid aggregator identity length!".to_string(),
                )
                .into());
            }
        }
        Ok(())
    }
}
