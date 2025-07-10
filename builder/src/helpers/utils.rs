use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rand::{distributions::Alphanumeric, Rng};
use regex::Regex;
use reqwest::Client;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{ChildStderr, ChildStdout};
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

/// read stdout of the command to build logs and send to socket
pub async fn read_stdout_bak(
    stdout: ChildStdout,
    step: usize,
    state: &Arc<AppState>,
    send_to_sock: bool,
    bypass_termination: bool,
    extract_envs: &Vec<String>,
    env_map: &mut HashMap<String, String>,
) {

    let total_step = state.config.project.build.commands.len() as usize;

    let reader = &mut BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut is_env = false;
    let mut buffer: Vec<BuildLog> = Vec::new();


    let flush_interval = if state.config.project.flush_interval >=500{
            state.config.project.flush_interval
        }
        else{
            500
        };

    // Interval timer for flushing logs every 200ms (adjust as needed)
    let mut interval = time::interval(Duration::from_millis(flush_interval as u64));

    // Set flush interval (e.g. 1 second)

    loop {
        tokio::select! {
            line_opt = lines.next_line() => {
                match line_opt {
                    Ok(Some(line)) => {
                        if line.contains("+_+_+_") {
                            is_env = true;
                            continue;
                        }

                        if is_env {
                            if let Some((key, value)) = line.split_once('=') {
                                if extract_envs.contains(&key.to_string()) {
                                    let mut current_build = state.builds.current_build.lock().await;
                                    if let Some(build) = current_build.as_mut() {
                                        build.payload.insert(key.to_string(), value.to_string());
                                    }
                                    env_map.insert(key.to_string(), value.to_string());
                                }
                            }
                            continue;
                        }

                        if !bypass_termination && *state.is_terminated.lock().await {
                            break;
                        }

                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        let log = BuildLog {
                            timestamp: chrono::Local::now(),
                            status: Status::Success,
                            step,
                            total_steps: total_step,
                            message: trimmed.to_string(),
                        };


                        // Buffer log for batch sending
                        buffer.push(log);
                    }
                    Ok(None) => {
                        // EOF
                        break;
                    }
                    Err(e) => {
                        eprintln!("Error reading stdout line: {:?}", e);
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    // Lock once and push all buffered logs
                    let mut current_build = state.builds.current_build.lock().await;
                    if let Some(build) = current_build.as_mut() {
                        for log in &buffer {
                            build.logs.push(log.clone());
                        }
                    }
                    drop(current_build);

                    if send_to_sock {
                        let json_str = serde_json::to_string(&buffer).unwrap();
                        let _ = state.build_sender.send(ChannelMessage::Data(json_str));
                    }

                    buffer.clear();
                }
            }
        }
    }

    // Send remaining buffered logs on EOF or termination
    if !buffer.is_empty() {
        let mut current_build = state.builds.current_build.lock().await;
        if let Some(build) = current_build.as_mut() {
            for log in &buffer {
                build.logs.push(log.clone());
            }
        }
        drop(current_build);

        if send_to_sock {
            let json_str = serde_json::to_string(&buffer).unwrap();
            let _ = state.build_sender.send(ChannelMessage::Data(json_str));
        }
    }
}



pub async fn read_stdout(
    stdout: ChildStdout,
    step: usize,
    state: &Arc<AppState>,
    send_to_sock: bool,
    bypass_termination: bool,
    extract_envs: &Vec<String>,
    env_map: &mut HashMap<String, String>,
) {
    let total_step = state.config.project.build.commands.len();
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut is_env = false;

    while let Ok(Some(line)) = lines.next_line().await {
        if line.contains("+_+_+_") {
            is_env = true;
            continue;
        }

        if is_env {
            if let Some((key, value)) = line.split_once('=') {
                if extract_envs.contains(&key.to_string()) {
                    // Save to current_build.payload
                    if let Some(build) = state.builds.current_build.lock().await.as_mut() {
                        build.payload.insert(key.to_string(), value.to_string());
                    }
                    // Also update env_map
                    env_map.insert(key.to_string(), value.to_string());
                }
            }
            continue;
        }

        if !bypass_termination && *state.is_terminated.lock().await {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let log = BuildLog {
            timestamp: Local::now(),
            status: Status::Success,
            step,
            total_steps: total_step,
            message: trimmed.to_string(),
        };

        // Push to in-memory build logs
        if let Some(build) = state.builds.current_build.lock().await.as_mut() {
            build.logs.push(log.clone());
        }

        // Immediately send to socket
        if send_to_sock {
            if let Ok(json_str) = serde_json::to_string(&log) {
                let _ = state.build_sender.send(ChannelMessage::Data(json_str));
            }
        }
    }
}
    // Send remaining buffered logs on EOF or termination
  

pub async fn read_stderr_bak(
    stderr: ChildStderr,
    step: usize,
    state: &Arc<AppState>,
    send_to_sock: bool,
    bypass_termination: bool,
) {
    let reader = &mut BufReader::new(stderr);
    let mut lines = reader.lines();
    let total_step = state.config.project.build.commands.len() as usize;

    // Buffer to hold logs before sending
    let mut buffer: Vec<BuildLog> = Vec::new();

    let flush_interval = if state.config.project.flush_interval >=500{
            state.config.project.flush_interval
        }
        else{
            500
        };

    // Interval timer for flushing logs every 200ms (adjust as needed)
    let mut interval = time::interval(Duration::from_millis(flush_interval as u64));

    loop {
        tokio::select! {
            line_opt = lines.next_line() => {
                match line_opt {
                    Ok(Some(line)) => {
                        if !bypass_termination && *state.is_terminated.lock().await {
                            break;
                        }

                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        let log = BuildLog {
                            timestamp: chrono::Local::now(),
                            status: Status::Error,
                            step,
                            total_steps: total_step,
                            message: trimmed.to_string(),
                        };

                        // Add log to buffer
                        buffer.push(log);
                    }
                    Ok(None) => {
                        // EOF reached
                        break;
                    }
                    Err(e) => {
                        eprintln!("Error reading stderr line: {:?}", e);
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    // Lock current_build once per flush
                    let mut current_build = state.builds.current_build.lock().await;
                    if let Some(build) = current_build.as_mut() {
                        for log in &buffer {
                            build.logs.push(log.clone());
                        }
                    }
                    drop(current_build);

                    // Send batch if requested
                    if send_to_sock {
                        let json_str = serde_json::to_string(&buffer).unwrap();
                        let _ = state.build_sender.send(ChannelMessage::Data(json_str));
                    }

                    buffer.clear();
                }
            }
        }
    }

    // Send any remaining logs after EOF or termination
    if !buffer.is_empty() {
        let mut current_build = state.builds.current_build.lock().await;
        if let Some(build) = current_build.as_mut() {
            for log in &buffer {
                build.logs.push(log.clone());
            }
        }
        drop(current_build);
        if send_to_sock {
            let json_str = serde_json::to_string(&buffer).unwrap();
            let _ = state.build_sender.send(ChannelMessage::Data(json_str));
        }
    }
}


pub async fn read_stderr(
    stderr: ChildStderr,
    step: usize,
    state: &Arc<AppState>,
    send_to_sock: bool,
    bypass_termination: bool,
) {
    let total_step = state.config.project.build.commands.len();
    let mut reader = BufReader::new(stderr);

    let mut buf = [0u8; 1024];
    let mut partial = Vec::new();

    loop {
        let n = match reader.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error reading stderr: {:?}", e);
                break;
            }
        };

        partial.extend_from_slice(&buf[..n]);

        // Process any complete lines or \r-delimited partial lines
        while let Some(pos) = partial.iter().position(|&b| b == b'\r' || b == b'\n') {
            let line_bytes = partial.drain(..=pos).collect::<Vec<u8>>();
            if let Ok(mut line) = String::from_utf8(line_bytes) {
                line = line.trim_matches(&['\r', '\n'][..]).to_string();
                if line.is_empty() {
                    continue;
                }

                if !bypass_termination && *state.is_terminated.lock().await {
                    return;
                }

                let log = BuildLog {
                    timestamp: Local::now(),
                    status: Status::Error,
                    step,
                    total_steps: total_step,
                    message: line,
                };

                if let Some(build) = state.builds.current_build.lock().await.as_mut() {
                    build.logs.push(log.clone());
                }

                if send_to_sock {
                    if let Ok(json_str) = serde_json::to_string(&log) {
                        let _ = state.build_sender.send(ChannelMessage::Data(json_str));
                    }
                }
            }
        }
    }

    // Handle any remaining data after EOF
    if !partial.is_empty() {
        if let Ok(line) = String::from_utf8(partial) {
            let line = line.trim_matches(&['\r', '\n'][..]).to_string();
            if !line.is_empty() {
                let log = BuildLog {
                    timestamp: Local::now(),
                    status: Status::Error,
                    step,
                    total_steps: total_step,
                    message: line,
                };

                if let Some(build) = state.builds.current_build.lock().await.as_mut() {
                    build.logs.push(log.clone());
                }

                if send_to_sock {
                    if let Ok(json_str) = serde_json::to_string(&log) {
                        let _ = state.build_sender.send(ChannelMessage::Data(json_str));
                    }
                }
            }
        }
    }
}

/// read stderr of the command to build logs and send to socket
pub async fn read_stderr_bak2(
    stderr: ChildStderr,
    step: usize,
    state: &Arc<AppState>,
    send_to_sock: bool,
    bypass_termination: bool,
) {
    let reader = BufReader::new(stderr);
    let mut lines = reader.lines();
    let total_step = state.config.project.build.commands.len();

    while let Ok(Some(line)) = lines.next_line().await {
        if !bypass_termination && *state.is_terminated.lock().await {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let log = BuildLog {
            timestamp: Local::now(),
            status: Status::Error,
            step,
            total_steps: total_step,
            message: trimmed.to_string(),
        };

        // Store to build logs
        if let Some(build) = state.builds.current_build.lock().await.as_mut() {
            build.logs.push(log.clone());
        }

        // Immediately send over socket
        if send_to_sock {
            if let Ok(json_str) = serde_json::to_string(&log) {
                let _ = state.build_sender.send(ChannelMessage::Data(json_str));
            }
        }
    }
}
    

pub async fn read_stdout_bak2(
    stdout: ChildStdout,
    step: usize,
    state: &Arc<AppState>,
    send_to_sock: bool,
    bypass_termination: bool,
    extract_envs: &Vec<String>,
    env_map: &mut HashMap<String, String>,
) {
    let total_step = state.config.project.build.commands.len();
    let mut reader = BufReader::new(stdout);

    let mut partial = Vec::new();
    let mut is_env = false;
    let mut buf = [0u8; 1024];

    loop {
        let n = reader.read(&mut buf).await.expect("failed to read stdout");
        if n == 0 {
            break; // EOF
        }

        partial.extend_from_slice(&buf[..n]);

        // Look for '\r' or '\n' in the buffered data
        while let Some(pos) = partial.iter().position(|&b| b == b'\r' || b == b'\n' || b == b'\n') {
            let line_bytes = partial.drain(..=pos).collect::<Vec<u8>>();
            if let Ok(mut line) = String::from_utf8(line_bytes) {
                line = line.trim_matches(&['\r', '\n','\t'][..]).to_string();
                if line.is_empty() {
                    continue;
                }

                if line.contains("+_+_+_") {
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

                if !bypass_termination && *state.is_terminated.lock().await {
                    return;
                }

                let log = BuildLog {
                    timestamp: Local::now(),
                    status: Status::Success,
                    step,
                    total_steps: total_step,
                    message: line,
                };

                if let Some(build) = state.builds.current_build.lock().await.as_mut() {
                    build.logs.push(log.clone());
                }

                if send_to_sock {
                    if let Ok(json_str) = serde_json::to_string(&log) {
                        let _ = state.build_sender.send(ChannelMessage::Data(json_str));
                    }
                }
            }
        }
    }

    // Handle any remaining data after EOF
    if !partial.is_empty() {
        if let Ok(line) = String::from_utf8(partial) {
            let line = line.trim_matches(&['\r', '\n'][..]).to_string();
            if !line.is_empty() {
                let log = BuildLog {
                    timestamp: Local::now(),
                    status: Status::Success,
                    step,
                    total_steps: total_step,
                    message: line,
                };
                if let Some(build) = state.builds.current_build.lock().await.as_mut() {
                    build.logs.push(log.clone());
                }
                if send_to_sock {
                    if let Ok(json_str) = serde_json::to_string(&log) {
                        let _ = state.build_sender.send(ChannelMessage::Data(json_str));
                    }
                }
            }
        }
    }
}
/// replace placeholders in the template with values

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