use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid config: {0}!")]
    InvalidConfig(String),
    #[error("Invalid command `{0}`!")]
    InvalidCommand(String),
}

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
