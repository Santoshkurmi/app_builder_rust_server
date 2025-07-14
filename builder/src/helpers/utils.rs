use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use actix_web::body::None;
use actix_web::web;
use rand::{distributions::Alphanumeric, Rng};
use regex::Regex;
use reqwest::Client;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdout};
use tokio::time;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use chrono::Local;
use crate::models::app_state::ChannelMessage;
use crate::models::app_state::{AppState, BuildLog};
use crate::models::config::PayloadType;
use crate::models::status::Status;

///generate a random token

pub fn generate_token(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

pub fn is_path_exits(path: &str) -> bool {
    Path::new(path).exists()
}


pub fn read_token_from_user_home(relative_path: &str) -> io::Result<String> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Could not determine the user home directory")
    })?;

    let full_path: PathBuf = home_dir.join(Path::new(relative_path));

    // Read and return the file content as String
    fs::read_to_string(full_path)
}


pub fn save_token_to_user_home(relative_path: &str, content: &str) -> io::Result<()> {
    // Get the user home directory
    let home_dir = dirs::home_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Could not determine the user home directory")
    })?;

    // Build the full path
    let full_path: PathBuf = home_dir.join(Path::new(relative_path));

    // Create parent directories if needed
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write the content to the file
    fs::write(full_path, content)?;

    Ok(())
}


/// create a file with content and parent directories
/// if the file already exists, it will be overwritten
/// if the parent directories do not exist, they will be created
pub fn create_file_with_dirs_and_content(file_path: &str, content: &str) -> io::Result<()> {
    
    
    let path = Path::new(file_path);

    // Create parent directories if they don't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Create or overwrite the file and write content
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}
/// join path with secure path
/// if the path is not secure, it will be returned as None (If outside the base path)
pub fn secure_join_path(base: &str, user_input: &str) -> Option<String> {
    // Canonicalize base directory
    let base = fs::canonicalize(base).ok()?;
    // Join and canonicalize the full path
    let full_path = fs::canonicalize(base.join(user_input)).ok()?;
    // Ensure full path is within base
    if full_path.starts_with(&base) {
        let str_path = full_path.to_str()?;
        // Ensure path is not empty
        if str_path.is_empty() {
            return None;
        }
        Some(str_path.into())
    } else {
        None
    }
}


/// extract payload from the request
pub async fn extract_payload(state: &Arc<AppState>,env_map:&mut HashMap<String,String>,param_map:&mut HashMap<String,String>) {


    for payload in &state.config.project.build.payload {

        if PayloadType::Param == payload.r#type {
            let mut  current_build = state.builds.current_build.lock().await;
            let  current_build = current_build.as_mut().unwrap();
            let param_value = current_build.payload.get(payload.key1.as_str()).unwrap();
            param_map.insert(payload.key1.to_string(), param_value.to_string());
            continue;
        }

        if payload.r#type != PayloadType::Env{
            continue;
        }
        let env_name = if payload.key2.is_some() {
            payload.key2.as_ref().unwrap()
        } else {
            payload.key1.as_str()
        };

        let mut  current_build = state.builds.current_build.lock().await;
        let  current_build = current_build.as_mut().unwrap();
        let env_value = current_build.payload.get(payload.key1.as_str()).unwrap();
        env_map.insert(env_name.to_string(), env_value.to_string());
    }
}



pub async fn read_combined_output(
    stdout: ChildStdout,
    stderr: ChildStderr,
    step: usize,
    state: web::Data<AppState>,
    send_to_sock: bool,
    flush_interval_ms: u64,
    extract_envs: &Vec<String>,
    env_map: &mut HashMap<String, String>,
) {
    let total_step = state.config.project.build.commands.len();
    let mut flush_timer = tokio::time::interval(Duration::from_millis(flush_interval_ms));
    let mut logs: Vec<BuildLog> = Vec::new();

    let mut stdout_reader = BufReader::new(stdout);
    let mut stderr_reader = BufReader::new(stderr);

    let mut stdout_buf = Vec::new();
    let mut stdout_partial = Vec::new();
    let mut stderr_buf = Vec::new();
    let mut stderr_partial = Vec::new();

    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut is_env = false;

    loop {
        tokio::select! {
            // Read stdout bytes
            read = stdout_reader.read_buf(&mut stdout_buf), if !stdout_done => {
                match read {
                    Ok(0) => stdout_done = true,
                    Ok(_) => {
                        stdout_partial.extend_from_slice(&stdout_buf);
                        stdout_buf.clear();

                        while let Some(pos) = stdout_partial.iter().position(|&b| b == b'\r' || b == b'\n') {
                            let line_bytes = stdout_partial.drain(..=pos).collect::<Vec<u8>>();
                            let ending_char = line_bytes.last().copied();
                            if let Ok( line) = String::from_utf8(line_bytes) {

                                let line_trimmed = line.trim_matches(&['\r', '\n'][..]);

                                if line_trimmed.is_empty() {
                                    continue;
                                }


                                if line.contains("+_+_+") {
                                    is_env = true;
                                    continue;
                                }

                                if is_env {
                                    if let Some((key, value)) = line.split_once('=') {
                                        if extract_envs.contains(&key.to_string()) {
                                            if let Some(build) = state.builds.current_build.lock().await.as_mut() {
                                                build.payload.insert(key.to_string(), value.to_string());
                                            }
                                            env_map.insert(key.to_string(), value.to_string());
                                        }
                                    }
                                    continue;
                                }

                                let message = if ending_char == Some(b'\r') {
                                    format!("\r{}", line_trimmed)
                                } else {
                                    line_trimmed.to_string()
                                };

                                logs.push(BuildLog {
                                    timestamp: Local::now(),
                                    status: Status::Success,
                                    step,
                                    total_steps: total_step,
                                    message: message,
                                });
                            }
                        }
                    },
                    Err(_) => stdout_done = true,
                }
            }

            // Read stderr bytes
            read = stderr_reader.read_buf(&mut stderr_buf), if !stderr_done => {
                match read {
                    Ok(0) => stderr_done = true,
                    Ok(_) => {
                        stderr_partial.extend_from_slice(&stderr_buf);
                        stderr_buf.clear();

                        while let Some(pos) = stderr_partial.iter().position(|&b| b == b'\r' || b == b'\n') {
                            let line_bytes = stderr_partial.drain(..=pos).collect::<Vec<u8>>();
                            let ending_char = line_bytes.last().copied();
                            
                            if let Ok(mut line) = String::from_utf8(line_bytes) {
                                line = line.trim_matches(&['\r', '\n'][..]).to_string();
                                if !line.is_empty() {

                                    let message = if ending_char == Some(b'\r') {
                                        format!("\r{}", line)
                                    } else {
                                        line.to_string()
                                    };

                                    logs.push(BuildLog {
                                        timestamp: Local::now(),
                                        status: Status::Error,
                                        step,
                                        total_steps: total_step,
                                        message: message,
                                    });
                                }
                            }
                        }
                    },
                    Err(_) => stderr_done = true,
                }
            }

            // Periodic flush
            _ = flush_timer.tick() => {
                flush_logs(&mut logs, send_to_sock, &state).await;
            }
        }

        if stdout_done && stderr_done {
            break;
        }
    }

    // Final stdout flush
    if !stdout_partial.is_empty() {
        let ending_char = stdout_partial.last().copied(); // Get the final byte
    
        if let Ok(raw_line) = String::from_utf8(stdout_partial.clone()) {
            let line_trimmed = raw_line.trim_matches(&['\r', '\n'][..]);
    
            if !line_trimmed.is_empty() {
                let mut message = line_trimmed.to_string();
                if ending_char == Some(b'\r') {
                    message.push('\r');
                }
    
                logs.push(BuildLog {
                    timestamp: Local::now(),
                    status: Status::Success,
                    step,
                    total_steps: total_step,
                    message,
                });
            }
        }
    }
    

    // Final stderr flush
    if !stderr_partial.is_empty() {
        let ending_char = stderr_partial.last().copied(); // Get the final byte

        if let Ok(line) = String::from_utf8(stderr_partial.clone()) {
            let line = line.trim_matches(&['\r', '\n'][..]).to_string();
            if !line.is_empty() {

                let mut message = line.to_string();
                if ending_char == Some(b'\r') {
                    message.push('\r');
                }

                logs.push(BuildLog {
                    timestamp: Local::now(),
                    status: Status::Error,
                    step,
                    total_steps: total_step,
                    message,
                });
            }
        }
    }

    flush_logs(&mut logs,  send_to_sock, &state).await;
}





async fn flush_logs(
    buffer: &mut Vec<BuildLog>,
    send_to_sock: bool,
    state: &Arc<AppState>,
) {
    if buffer.is_empty() {
        return;
    }

    if let Some(build) = state.builds.current_build.lock().await.as_mut() {
        build.logs.extend(buffer.iter().cloned());
    }

    if send_to_sock {
        for log in buffer.iter() {
            if let Ok(json_str) = serde_json::to_string(log) {
                let _ = state.build_sender.send(ChannelMessage::Data(json_str));
            }
        }
    }

    buffer.clear();
}

// end here

pub fn replace_placeholders(template: &str, values: &HashMap<String, String>) -> String {
    let re = Regex::new(r"\{([^}]+)\}").unwrap();

    re.replace_all(template, |caps: &regex::Captures| {
        let key = &caps[1];
        values.get(key)
            .map(|s| s.to_string()) // convert &str to String
            .unwrap_or_else(|| caps[0].to_string()) // fallback: whole match
    }).into_owned()
}

/// save the logs to the log path
pub async  fn save_log(log_path:&String,logs:String,build_id:String){

   
 
    let full_path = log_path;

    // Create logs directory if it doesn't exist
    fs::create_dir_all( &full_path).expect("Failed to create logs directory");

    // Create a file inside ~/logs
    let mut file_path = PathBuf::from(&full_path);

    let now = Local::now();
    file_path.push(format!("{}_{}.log", now.format("%Y-%m-%d_%H-%M-%S"), build_id ));

    println!("File path: {}", file_path.to_str().unwrap());

    let mut file = File::create(file_path).expect("Failed to create file");
    // writeln!(file, "This is a new log entry!").expect("Failed to write to file");

    file.write_all(logs.as_bytes()).expect("Failed to write to file");

   
}

/// send the logs to the other server
pub async fn send_to_other_server(path:String,data:String) ->bool{
    
    let client = Client::new();
    println!("{}",path);

    let res = client
        .post(path)
        .body(data)
        .header("Content-Type", "application/json")
        .timeout(Duration::new(20, 0))
        .send()
        .await;

    match res {
        Ok(response) => {
            let status = response.status();
            if  !status.is_success(){
                println!("failed to send data to other server: {}", status);
                let body = response.text().await.unwrap_or_default();
                println!("{}",body);
                return  false;
            }
            let body = response.text().await.unwrap_or_default();
            println!("Successfully sent data to other server: {}", status);
            println!("Response body: {}", body);
            return  true;
        }
        Err(err) => {
            println!("failed to send data to other server: {}", err);
            return  false;
        } 
    }


}