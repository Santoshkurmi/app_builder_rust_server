
use actix_web::{web, App, HttpServer, Responder};
use actix_web::middleware::Logger;
use rustls::{Certificate, PrivateKey};
use rustls::server::{ServerConfig, ResolvesServerCert, ClientHello};
use rustls::sign::{CertifiedKey, any_supported_type};
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime};
use std::path::PathBuf;

pub struct DynamicCertResolver {
    cert_path: PathBuf,
    key_path: PathBuf,
    cached: RwLock<Option<Arc<CachedCert>>>,
}

#[derive(Clone)]
struct CachedCert {
    certified_key: Arc<CertifiedKey>,
    cert_mtime: SystemTime,
    key_mtime: SystemTime,
    last_checked: Instant,
}

impl std::fmt::Debug for DynamicCertResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicCertResolver")
            .field("cert_path", &self.cert_path)
            .field("key_path", &self.key_path)
            .finish()
    }
}

impl DynamicCertResolver {
    pub fn new(cert_path: &str, key_path: &str) -> Self {
        Self {
            cert_path: PathBuf::from(cert_path),
            key_path: PathBuf::from(key_path),
            cached: RwLock::new(None),
        }
    }

    fn load_key_and_cert(&self) -> Result<(Vec<Certificate>, PrivateKey), Box<dyn std::error::Error>> {
        // Load cert
        let cert_file = &mut BufReader::new(File::open(&self.cert_path)?);
        let cert_chain = rustls_pemfile::certs(cert_file)?
            .into_iter()
            .map(Certificate)
            .collect();

        // Load private key
        let key_file = &mut BufReader::new(File::open(&self.key_path)?);
        let keys = rustls_pemfile::pkcs8_private_keys(key_file)?;
        if keys.is_empty() {
            return Err("No private keys found".into());
        }
        let private_key = PrivateKey(keys[0].clone());

        Ok((cert_chain, private_key))
    }

    fn get_certified_key(&self) -> Option<Arc<CertifiedKey>> {
        let now = Instant::now();

        // 1. Read lock check (fast path)
        if let Some(cached) = &*self.cached.read().unwrap() {
            if now.duration_since(cached.last_checked).as_secs() < 3600 {
                return Some(cached.certified_key.clone());
            }
        }

        // 2. We need to check or reload
        let cert_mtime = std::fs::metadata(&self.cert_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let key_mtime = std::fs::metadata(&self.key_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        // Check if we need to reload under write lock
        let mut cache_write = self.cached.write().unwrap();
        let (needs_update, certified_key) = if let Some(cached) = &*cache_write {
            if cached.cert_mtime == cert_mtime && cached.key_mtime == key_mtime {
                (true, Some(cached.certified_key.clone()))
            } else {
                (false, None)
            }
        } else {
            (false, None)
        };

        if needs_update {
            if let Some(certified_key) = certified_key {
                let updated = CachedCert {
                    certified_key: certified_key.clone(),
                    cert_mtime,
                    key_mtime,
                    last_checked: now,
                };
                *cache_write = Some(Arc::new(updated));
                return Some(certified_key);
            }
        }

        // Load files
        match self.load_key_and_cert() {
            Ok((cert_chain, private_key)) => {
                match any_supported_type(&private_key) {
                    Ok(signing_key) => {
                        let certified_key = Arc::new(CertifiedKey::new(cert_chain, signing_key));
                        let new_cached = CachedCert {
                            certified_key: certified_key.clone(),
                            cert_mtime,
                            key_mtime,
                            last_checked: now,
                        };
                        *cache_write = Some(Arc::new(new_cached));
                        log::info!("Successfully loaded certificate from disk");
                        Some(certified_key)
                    }
                    Err(e) => {
                        log::error!("Failed to parse private key: {:?}", e);
                        // Fall back to old cert if available
                        cache_write.as_ref().map(|c| c.certified_key.clone())
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to load certificate files: {:?}", e);
                // Fall back to old cert if available
                cache_write.as_ref().map(|c| c.certified_key.clone())
            }
        }
    }
}

impl ResolvesServerCert for DynamicCertResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.get_certified_key()
    }
}

pub fn load_rustls_config(cert_path: &str, key_path: &str) -> ServerConfig {
    let resolver = Arc::new(DynamicCertResolver::new(cert_path, key_path));
    
    // Attempt initial load to fail fast if config is invalid
    if resolver.get_certified_key().is_none() {
        panic!("Failed to load initial SSL certificate or key from paths: {} and {}", cert_path, key_path);
    }

    ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_cert_resolver(resolver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_dynamic_cert_resolver() {
        let temp_dir = std::env::temp_dir();
        let uuid = uuid::Uuid::new_v4().to_string();
        let cert_path = temp_dir.join(format!("{}_cert.pem", uuid));
        let key_path = temp_dir.join(format!("{}_key.pem", uuid));

        // Copy our valid cert.pem and key.pem from the workspace
        fs::copy("../cert.pem", &cert_path).expect("failed to copy cert");
        fs::copy("../key.pem", &key_path).expect("failed to copy key");

        let cert_path_str = cert_path.to_str().unwrap();
        let key_path_str = key_path.to_str().unwrap();

        let resolver = DynamicCertResolver::new(cert_path_str, key_path_str);

        // 1. Initial load
        let key1 = resolver.get_certified_key().expect("Failed to load initially");
        assert!(Arc::ptr_eq(&resolver.cached.read().unwrap().as_ref().unwrap().certified_key, &key1));

        // 2. Fetching again immediately should return cached value (fast path)
        let key2 = resolver.get_certified_key().expect("Failed to get cached key");
        assert!(Arc::ptr_eq(&key1, &key2));

        // 3. Update modification times to trigger reload
        std::thread::sleep(std::time::Duration::from_millis(150));
        fs::write(&cert_path, fs::read(&cert_path).unwrap()).unwrap();
        fs::write(&key_path, fs::read(&key_path).unwrap()).unwrap();

        // Manipulate last_checked to bypass the 1-hour check
        {
            let mut cache = resolver.cached.write().unwrap();
            let mut cached_val = (*cache.as_ref().unwrap().clone()).clone();
            cached_val.last_checked = Instant::now() - std::time::Duration::from_secs(3610);
            *cache = Some(Arc::new(cached_val));
        }

        // 4. Resolve again. It should read files, see modified time, reload them, and update cache.
        let key3 = resolver.get_certified_key().expect("Failed to load updated key");
        assert!(!Arc::ptr_eq(&key1, &key3)); // Should be a new Arc

        // Clean up files
        let _ = fs::remove_file(&cert_path);
        let _ = fs::remove_file(&key_path);
    }
}
