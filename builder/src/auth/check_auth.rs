use std::net::IpAddr;

use actix_web::{HttpRequest, web};

use crate::models::{
    app_state::AppState,
    config::{AddressType, AuthConfig, AuthType},
};

/// Check if the request is authorized

pub async fn is_authorized(
    req: &HttpRequest,
    state: web::Data<AppState>,
) -> bool {
    let auth_config = &state.config.auth;
  

    match auth_config.auth_type {
        AuthType::Token => check_token_auth(req, auth_config),
        AuthType::Address => check_address_auth(req, auth_config),
        AuthType::Both => {
            check_token_auth(req, auth_config) && check_address_auth(req, auth_config)
        }
    }
}
/// Check if the token is authorized
fn check_token_auth(req: &HttpRequest, auth_config: &AuthConfig) -> bool {
    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = &auth_str[7..];
                return auth_config.allowed_tokens.contains(&token.to_string());
            }
        }
    }

    // Also check query parameter
    if let Some(query_string) = req.uri().query() {
        for pair in query_string.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                if key == "token" && auth_config.allowed_tokens.contains(&value.to_string()) {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if the address is authorized
fn check_address_auth(req: &HttpRequest, auth_config: &AuthConfig) -> bool {
    let conn_info = req.connection_info();
    let remote_addr = conn_info.realip_remote_addr().unwrap_or("unknown");

    match auth_config.address_type {
        AddressType::IP => {
            if let Ok(ip) = remote_addr.parse::<IpAddr>() {
                auth_config.allowed_addresses.contains(&ip.to_string())
            } else {
                false
            }
        }
        AddressType::Hostname => auth_config
            .allowed_addresses
            .contains(&remote_addr.to_string()),
    }
}
