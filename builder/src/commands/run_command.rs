use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::{fs, io::Write, path::Path};
use std::process::Command;

use crate::auth::check_auth::is_authorized;
use crate::helpers::utils::replace_placeholders;
use crate::models::app_state::AppState;

pub async fn handle_multipart(req: HttpRequest,state: web::Data<AppState>,mut payload: Multipart) -> impl Responder {
    
    if state.config.auth.command_handle_token.is_empty() {
        return HttpResponse::BadRequest().body("🚫 Command handle token is not configured");
    }

    let mut is_valid_user = false;
    if let Some(header_token) = req.headers().get("token") {
        if let Ok(token_str) = header_token.to_str() {        
            if state.config.auth.command_handle_token == token_str {
                is_valid_user = true;

            }
        }
    }

    if is_valid_user == false {
        return HttpResponse::Unauthorized().body("🔒 Unauthorized access");
    }

    {
       if  *state.is_queue_running.lock().await{
           return HttpResponse::Conflict().body("🚧 Build is in progress, please be patient");
       }
    }

    let mut is_file: Option<bool> = None;
    let mut command: Option<String> = None;
    let mut file_saved = false;
    let mut param: Option<String> = None;

    let base_path = state.config.project.project_path.clone();

    let temp_path = Path::new(&base_path).join(".temp");
    fs::create_dir_all(&temp_path).unwrap();


    // Ensure temp directory exists

    while let Some(mut field) = payload.next().await {
        let field = field.as_mut();
        if field.is_err(){
            continue;
        }
        let field = field.unwrap();
        let content_disposition = field.content_disposition();
        let name = content_disposition
            .get_name()
            .unwrap_or_default();

        match name {
            "is_file" => {
                let mut bytes = web::BytesMut::new();
                while let Some(chunk) = field.next().await {
                    bytes.extend_from_slice(&chunk.unwrap());
                }
                let val = std::str::from_utf8(&bytes).unwrap().trim();
                is_file = Some(matches!(val, "true" | "1" | "yes"));
            }
            "param" => {
                let mut bytes = web::BytesMut::new();
                while let Some(chunk) = field.next().await {
                    bytes.extend_from_slice(&chunk.unwrap());
                }
                param = Some(String::from_utf8(bytes.to_vec()).unwrap());
            }

            "command" => {
                let mut bytes = web::BytesMut::new();
                while let Some(chunk) = field.next().await {
                    bytes.extend_from_slice(&chunk.unwrap());
                }
                command = Some(String::from_utf8(bytes.to_vec()).unwrap());
            }

            "file" => {
                let temp_path = Path::new(&temp_path).join("temp.file");
                let mut f = fs::File::create(temp_path).unwrap();

                let mut size: usize = 0;
                const MAX_SIZE: usize = 400 * 1024 * 1024; // 400 MB

                while let Some(chunk) = field.next().await {
                    let data = chunk.unwrap();
                    size += data.len();
                    if size > MAX_SIZE {
                        return HttpResponse::BadRequest().body("File size exceeds 400MB limit");
                    }
                    f.write_all(&data).unwrap();
                }

                file_saved = true;
            }

            _ => {}
        }
    }



    // Validation
    if is_file == Some(true) && !file_saved {
        return HttpResponse::BadRequest().body("📁 Expected file to be uploaded with is_file=true");

    }

    if command.is_none() {
        return HttpResponse::BadRequest().body("⚠️ Missing command field");

    }
    let command = command.unwrap();

    let command_script = state.config.project.build.setup_commands.get(&command);

    if command_script.is_none(){
        return  HttpResponse::BadRequest().body("⚠️ Missing command script");
    }

    let command_script = command_script.unwrap();
    let mut payload_map = HashMap::new();
    payload_map.insert("param".to_string(), param.unwrap());
    
    let replace_command_script = replace_placeholders(&command_script, &payload_map);

    let (success, output) = run_bash_command(&replace_command_script,&base_path);

    if success {
        return HttpResponse::Ok().body(format!("✅ Command executed successfully:\n{}", output));
    } else {
        return HttpResponse::BadRequest().body(format!("❌ Command execution failed:\n{}", output));
    }

    // You can handle the `command` as needed from here
    // Example: let command_to_run = command.unwrap();

    // Ok(HttpResponse::Ok().body("Upload and validation successful"))
}



fn run_bash_command(command_str: &str,base_path: &str) -> (bool, String) {
    let output = Command::new("bash")
        .arg("-c")
        .current_dir(base_path)
        .arg(command_str)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);

            let combined_output = format!(
                "{}{}",
                if !stdout.is_empty() { format!("stdout:\n{}\n", stdout) } else { "".into() },
                if !stderr.is_empty() { format!("stderr:\n{}\n", stderr) } else { "".into() },
            );

            (out.status.success(), combined_output)
        }
        Err(e) => (false, format!("Failed to execute command: {}", e)),
    }
}


pub async fn abort_server(req: HttpRequest,state:web::Data<AppState>)->impl Responder{

    if  !is_authorized(&req, state).await {
        return HttpResponse::Unauthorized().body("🔒 Unauthorized access");
    }

    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(1));
        std::process::exit(0);
    });

    return HttpResponse::Ok().body("🛑 Aborting server");
}