use std::io;
use std::path::{Path, PathBuf};

use actix_web::{web, App, HttpServer};
use app_builder::commands::run_command::handle_multipart;
use app_builder::socket::valid_project_token::set_valid_project_token;
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
    let config = Config::load(full_path.to_str().unwrap()).expect("Failed to load config");
    let port = config.port;
    // let ssl_enabled = config.ssl.enable_ssl;
    
    // let certificate_key_path = config.ssl.certificate_key_path.clone();
    // let cetificate_path = config.ssl.certificate_path.clone();

    let listen_address = config.listen_address.clone();
    // Create shared application state
    let app_state = AppState::new(config).await;

    
    let app_data = web::Data::new(app_state);


    println!("Listening server on  {}:{}" , listen_address, port);
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
            ;
        app
    });

    server.bind((listen_address.as_str(), port))?.run().await
    
    // if ssl_enabled {
    //     let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    //     builder.set_private_key_file(&certificate_key_path, SslFiletype::PEM).unwrap();
    //     builder.set_certificate_chain_file(&cetificate_path).unwrap();
        
    //     server.bind_openssl(format!("0.0.0.0:{}", port), builder)?.run().await
    // } else {
    //     server.bind(("0.0.0.0", port))?.run().await
    // }
}