use std::{collections::HashMap, hash::Hash};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub name: String,
    pub port: u16,
    pub listen_address: String,
    pub log_path: String,
    pub enable_logs: bool,
    pub ssl: SslConfig,
    pub auth: AuthConfig,
    pub project: ProjectConfig,
    pub token_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SslConfig {
    pub enable_ssl: bool,
    pub certificate_path: String,
    pub certificate_key_path: String,
}
#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    Token,
    Address,
    Both,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddressType {
    IP,
    Hostname,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub auth_type: AuthType,       // "token", "address", "both"
    pub address_type: AddressType, // "ip", "hostname"
    pub allowed_addresses: Vec<String>,
    pub allowed_tokens: Vec<String>,
    pub command_handle_token: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub allow_multi_build: bool,
    pub max_pending_build: u32,
    pub next_build_delay: u32,
    pub flush_interval: u32,
    // pub base_endpoint_path: String,
    pub build: BuildConfig,
    pub project_path: String,

}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Payload {
    pub r#type: PayloadType,
    pub key1: String,
    pub key2: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Serialize,PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PayloadType {
    Env,
    Param,
    File,
}

impl std::str::FromStr for PayloadType {
    type Err = ();
    fn from_str(input: &str) -> Result<PayloadType, Self::Err> {
        match input {
            "env" => Ok(PayloadType::Env),
            "param" => Ok(PayloadType::Param),
            "file" => Ok(PayloadType::File),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildConfig {
    pub payload: Vec<Payload>,
    pub unique_build_key: String,
    pub on_success_failure: String,
    pub on_success_error_payload: Vec<Payload>,
    pub commands: Vec<CommandConfig>,
    #[serde(default)]
    pub setup_commands: HashMap<String,String>,
    #[serde(default)]
    pub run_on_success: Vec<CommandConfig>,
    #[serde(default)]
    pub run_on_failure: Vec<CommandConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandConfig {
    pub command: String,
    pub title: String,
    #[serde(default)]
    pub extract_envs: Vec<String>,
    #[serde(default="default_on_error")]
    pub abort_on_error: bool, // "abort", "continue"
    #[serde(default="default_to_sock")]
    pub send_to_sock: bool,

}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

fn default_to_sock() -> bool {
    true
}

fn default_on_error() -> bool {
    true
}
