use actix_web::{HttpRequest, HttpResponse, web};
use std::fs;
use toml;
use std::path::PathBuf;
use chrono::Local;
use std::io::Write;
use crate::{auth::check_auth::is_authorized, models::{app_state::AppState, config::Config}};

pub async fn get_config(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> actix_web::Result<HttpResponse> {

    println!("Getting config");
    if !is_authorized(&req, state.clone()).await {
        return Ok(HttpResponse::Unauthorized().body("Unauthorized"));
    }

    println!("Getting config perfect user");


    let path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/app_builder/config.toml");

        let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read config file: {}", e);
            return Ok(HttpResponse::InternalServerError().body("Could not read config"));
        }
    };

    println!("Getting config perfect user and path");

    let config: Config = toml::from_str(&contents).unwrap();

    println!("Getting config perfect user and path");


    Ok(HttpResponse::Ok().json(config))
}


pub async fn update_config(
    req: HttpRequest,
    state: web::Data<AppState>,
    new_config: web::Json<Config>,
) -> actix_web::Result<HttpResponse> {
    if !is_authorized(&req, state.clone()).await {
        return Ok(HttpResponse::Unauthorized().body("Unauthorized"));
    }

    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let config_path = home_dir.join(".config/app_builder/config.toml");

    // Backup original
    let backup_name = format!(
        "config-{}.bak",
        Local::now().format("%Y%m%d%H%M%S")
    );
    let backup_path = config_path.with_file_name(backup_name);

    fs::copy(&config_path, &backup_path)?;

    // Convert JSON -> TOML
    let toml_str = toml::to_string_pretty(&new_config.into_inner()).unwrap();

    // Overwrite config
    let mut file = fs::File::create(&config_path)?;
    file.write_all(toml_str.as_bytes())?;

    Ok(HttpResponse::Ok().body("Config updated and backed up"))
}