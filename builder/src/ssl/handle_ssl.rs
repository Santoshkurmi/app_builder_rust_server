
use actix_web::{web, App, HttpServer, Responder};
use actix_web::middleware::Logger;
use rustls::{Certificate, PrivateKey};
use rustls::server::ServerConfig;
use std::fs::File;
use std::io::BufReader;

pub fn load_rustls_config(cert_path: &str, key_path: &str) -> ServerConfig {
    // Load cert
    let cert_file = &mut BufReader::new(File::open(cert_path).expect("Cannot open certificate"));
    let cert_chain = rustls_pemfile::certs(cert_file)
        .expect("Cannot read certificate")
        .into_iter()
        .map(Certificate)
        .collect();

    // Load private key
    let key_file = &mut BufReader::new(File::open(key_path).expect("Cannot open private key"));
    let keys = rustls_pemfile::pkcs8_private_keys(key_file)
        .expect("Cannot read private key");
    let private_key = PrivateKey(keys[0].clone());

    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .expect("Failed to create rustls ServerConfig");

    config
}
