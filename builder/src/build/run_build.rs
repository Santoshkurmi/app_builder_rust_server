use std::{collections::HashMap, process::Stdio};

use actix_web::web;
use chrono::format;
use tokio::{ process::Command};

use crate::{helpers::utils::{extract_payload, read_stderr, read_stdout, replace_placeholders}, models::{app_state::{ AppState, BuildLog, ChannelMessage, ProjectLog}, config::{CommandConfig}, status::Status}};

/// execute commands and handle the output
pub async fn run_build(state: web::Data<AppState>) {

    let mut env_map: HashMap<String, String> = HashMap::new();
    let mut param_map: HashMap<String, String> = HashMap::new();

    extract_payload(&state, &mut env_map, &mut param_map).await;



    let mut step = 1;
    let total_step = state.config.project.build.commands.len() as usize;
    for command in &state.config.project.build.commands {


        {
            
            let log = BuildLog {
                timestamp: chrono::Local::now(),
                status: Status::StartingCommand,
                step,
                total_steps: total_step,
                message: format!("{}", command.title),
             };
            if command.send_to_sock {
                    let json_str = serde_json::to_string(&log).unwrap();
                    let _ = state.build_sender.send(ChannelMessage::Data(json_str));
            }

            let mut  current_build_guard = state.builds.current_build.lock().await;
            let  current_build = current_build_guard.as_mut().unwrap();
            current_build.current_step = step;

            current_build.logs.push(log.clone());

            let project_log = ProjectLog{
                id: current_build.id.clone(),
                total_steps: state.config.project.build.commands.len() as usize,
                unique_id: current_build.unique_id.clone(),
                socket_token: current_build.socket_token.clone(),
                step: step,
                state: Status::StartingCommand,
                timestamp: chrono::Local::now(),

                message: command.title.clone()
            };
            drop(current_build_guard);

            let project_log_json = serde_json::to_string(&project_log).unwrap();
            let _ = state.project_sender.send(ChannelMessage::Data(project_log_json));

            let mut project_logs = state.project_logs.lock().await;
            project_logs.push(project_log);
        }

        

        let  command_with_params = replace_placeholders(&command.command, &param_map);

        // let command_with_env = format!("{} && echo '+_+_+_\n' && env", command_with_params);
       
    //    println!("Running command: {}", command_with_env);
        let  child = Command::new("bash")
            .arg("-c")
            .envs(&env_map)
            .current_dir(state.config.project.project_path.as_str())
            .arg( &command_with_params )
            
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            ;

         
        
        let mut child = child.unwrap();

        let  stdout = child.stdout.take().unwrap();
        let  stderr = child.stderr.take().unwrap();

       
        
        tokio::join!(
            read_stdout(stdout, step, &state,command.send_to_sock,false,&command.extract_envs,&mut env_map ),
            read_stderr(stderr, step, &state,command.send_to_sock,false)
        );
        
        

        let status = child.wait().await.expect("Failed to wait on child");
        
         if * state.is_terminated.lock().await {
            child.kill().await.unwrap();
            let mut  current_build = state.builds.current_build.lock().await;
            let  current_build = current_build.as_mut().unwrap();
            current_build.status = Status::Aborted;
            break;
        }
        
        if status.success() {
            let mut  current_build = state.builds.current_build.lock().await;
            let  current_build = current_build.as_mut().unwrap();
            current_build.status = Status::Success; //nothing much to do
            
        } else {

            let mut  current_build = state.builds.current_build.lock().await;
            let  current_build = current_build.as_mut().unwrap();
            current_build.status = Status::Error; //
            if command.abort_on_error {
                // *state.is_terminated.lock().await = true;
                break;
            }//handle the case here all the other will also be terminated, handle here
        }

       

        step += 1;


    }//loop each command

    
        let mut  current_build_guard = state.builds.current_build.lock().await;
        let  current_build = current_build_guard.as_mut().unwrap();
        let commands = if current_build.status == Status::Success{

            &state.config.project.build.run_on_success
        }
        else{
            &state.config.project.build.run_on_failure
        };

        drop(current_build_guard);   

        
       
        run_on_success_error_payload(&state, &mut env_map, &mut param_map,&commands, step).await;
        {

            let mut  current_build_guard = state.builds.current_build.lock().await;
            let  current_build = current_build_guard.as_mut().unwrap();
            
            let project_log = ProjectLog{
                id: current_build.id.clone(),
                unique_id: current_build.unique_id.clone(),
                socket_token: current_build.socket_token.clone(),
                step: step,
                total_steps: state.config.project.build.commands.len() as usize,
                timestamp: chrono::Local::now(),

                state: current_build.status.clone(),
                message: "Finalizing build".to_string()
            };

            let project_log_json = serde_json::to_string(&project_log).unwrap();
            let _ = state.project_sender.send(ChannelMessage::Data(project_log_json));

            let mut project_logs = state.project_logs.lock().await;
            project_logs.push(project_log);
                    
        }

}


pub async fn run_on_success_error_payload(state: &web::Data<AppState>,env_map:&mut HashMap<String,String>,param_map:&mut HashMap<String,String>,commands:&Vec<CommandConfig>,step: usize) {

    println!("Running on success error payload");
    let mut step = step;
    for command in commands {

        let  command_with_params = replace_placeholders(&command.command, &param_map);

        // let command_with_env = format!("{} && echo '+_+_+_\n' && env", command_with_params);
        
        
        let  child = Command::new("bash")
            .arg("-c")
            .envs(&*env_map)
            .arg( &command_with_params )
            .current_dir(state.config.project.project_path.as_str())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            ;

           
            
        let mut child = child.unwrap();
        

        let  stdout: tokio::process::ChildStdout = child.stdout.take().unwrap();
        let  stderr = child.stderr.take().unwrap();

       
        
        tokio::join!(
            read_stdout(stdout, step, &state,command.send_to_sock,true,&command.extract_envs, env_map ),
            read_stderr(stderr, step, &state,command.send_to_sock,true)
        );
        
    

        let status = child.wait().await.expect("Failed to wait on child");
        if !status.success() {
            if command.abort_on_error {
                break;
            }//handle the case here all the other will also be terminated, handle here
        }


        step += 1;


    }//loop each command




}