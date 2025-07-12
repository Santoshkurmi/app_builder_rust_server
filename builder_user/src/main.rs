use std::io;
use std::path::{Path, PathBuf};

use actix_web::{web, App, HttpServer};
use app_builder::commands::run_command::handle_multipart;
use app_builder::handle_config::get_set_config::{get_config, update_config};
use app_builder::socket::valid_project_token::set_valid_project_token;
use app_builder::ssl::handle_ssl::load_rustls_config;
use app_builder::{build::{abort::{abort, abort_all}, build_init::build_initialize}, models::{app_state::AppState, config::Config}, pending_update::get_pending_update::get_pending_update, socket::{handle_socket::connect_and_stream_ws_build, handle_socket_project::connect_and_stream_ws_project}};
// use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    
    // Load configuration
     let home_dir = dirs::home_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Could not determine the user home directory")
    })?;

    let full_path: PathBuf = home_dir.join(Path::new(".config/app_builder/config.toml"));

    // let config = Config::load("/etc/build_server.toml").expect("Failed to load config");
    let config = Config::load(full_path.to_str().unwrap()).expect(&format!("Failed to load config file: {}", full_path.to_str().unwrap())  );
    let port = config.port;
    let ssl_enabled = config.ssl.enable_ssl;
    
    let certificate_key_path = config.ssl.certificate_key_path.clone();
    let cetificate_path = config.ssl.certificate_path.clone();

    let listen_address = config.listen_address.clone();
    // Create shared application state
    let app_state = AppState::new(config).await;

    
    let app_data = web::Data::new(app_state);
    let http_protcol: &'static str = if ssl_enabled { "https" } else { "http" };

    println!("Listening server on  {}://{}:{}" ,http_protcol, listen_address, port);
    let server = HttpServer::new(move || {
        let  app = App::new()
            .app_data(app_data.clone())
            .service(web::resource("/api/init/build").route(web::post().to(build_initialize)))
            .service(web::resource("/api/connect/build").route(web::get().to(connect_and_stream_ws_build)))
            .service(web::resource("/api/connect/project").route(web::get().to(connect_and_stream_ws_project)))
            .service(web::resource("/api/abort/all").route(web::post().to(abort_all)))
            .service(web::resource("/api/abort").route(web::post().to(  abort  )))
            .service(web::resource("/api/pending/updates").route(web::get().to(  get_pending_update  )))
            .service(web::resource("/api/set/token").route(web::post().to(  set_valid_project_token  )))
            .service(web::resource("/api/command").route(web::post().to(  handle_multipart  )))
            .service(web::resource("/api/get/config").route(web::get().to(  get_config  )))
            .service(web::resource("/api/update/config").route(web::post().to(  update_config  )))
            ;
        app
    });
    if ssl_enabled {
        let rustls_config = load_rustls_config(&cetificate_path,&certificate_key_path);

        server.bind_rustls(format!("{}:{}", listen_address.as_str(), port), rustls_config)?.run().await
    }
    else{
        server.bind((listen_address.as_str(), port))?.run().await
    }
    
}