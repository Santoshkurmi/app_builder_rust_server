use std::{collections::HashMap};

use actix_web::web;
use chrono::Local;

use crate::{error_success::handle_error_success::{ handle_error_success, notify_on_build_started, notify_on_queue_ended}, models::{app_state::{ AppState, BuildProcess, ChannelMessage, ProjectLog}, status::Status}};

use super::run_build::run_build;

/// hanldes the builds queue and execution
pub async fn build_manager(state: web::Data<AppState>) {
    
    let mut is_first = true;

    {
        let mut is_queue_running = state.is_queue_running.lock().await;
        *is_queue_running = true;
    }
   

    loop{
        let mut build_queue = state.builds.build_queue.lock().await;
        
        if build_queue.is_empty() {
            break;
        }

        let build = build_queue.remove(0);
        drop(build_queue);

        let build_process = BuildProcess{
            id: build.id.clone(),
            unique_id: build.unique_id.clone(),
            status: crate::models::status::Status::Building,
            current_step: 1,
            total_steps: state.config.project.build.commands.len() as usize,
            started_at: chrono::Local::now(),
            end_at: chrono::Local::now(),
            duration:0,
            socket_token: build.socket_token.clone(),
            logs: Vec::new(),
            payload: build.payload.clone(),
            out_payload: HashMap::new(),
        };
        println!("Starting build for {}", build.unique_id);

        if !is_first{
            is_first = false;
            notify_on_build_started(&state.config.project.build.on_success_failure, &build_process).await;
            println!("Notifying build is sending in the background");
        }
        else{
            println!("No notifying as this is the first build");
        }

      

        {
            state.builds.current_build.lock().await.replace(build_process);
        }

        // start the thread to perform the build operation here
        // and await
        let project_log = ProjectLog{
            id: build.id.clone(),
            unique_id: build.unique_id.clone(),
            socket_token: build.socket_token.clone(),
            step: 0,
            total_steps: state.config.project.build.commands.len() as usize,
            timestamp: Local::now(),
            state: Status::Building,
            message: "Starting build".to_string()
        };


        
        {
            let mut project_logs = state.project_logs.lock().await;
            project_logs.push(project_log.clone());
            let log = serde_json::to_string(&project_log).unwrap();
            let _ = state.project_sender.send(ChannelMessage::Data(log));
        }


        run_build(state.clone()).await;

        
        
        {
            let mut terminated = state.is_terminated.lock().await;
            *terminated = false;
        }

        //check the status of the build whether its failed or success
        {
            let mut current_build = state.builds.current_build.lock().await;
        
            let cur_build = current_build.as_mut().unwrap();
            cur_build.end_at = chrono::Local::now();
            cur_build.duration = cur_build.end_at.signed_duration_since(cur_build.started_at).num_seconds();
            let cur_build_clone = cur_build.clone();

           
            drop(current_build);

            let project_log = ProjectLog{
                id: build.id.clone(),
                unique_id: build.unique_id.clone(),
                socket_token: build.socket_token.clone(),
                step: 1000,
                total_steps: state.config.project.build.commands.len() as usize,
                timestamp: Local::now(),
                state:  cur_build_clone.status.clone(),
                message: "ended".to_string()
            };
            let project_log_json = serde_json::to_string(&project_log).unwrap();
            
            handle_error_success(state.clone(),cur_build_clone.clone()).await;
            
            let _ = state.project_sender.send(ChannelMessage::Data(project_log_json));
            let _ = state.build_sender.send(ChannelMessage::Shutdown);
        }

        {
            let mut current_build = state.builds.current_build.lock().await;
        
            *current_build = None;
        }
            

        {

            let build_queue = state.builds.build_queue.lock().await;
            
            if build_queue.is_empty() {
                break;
            }

        }

            
            if state.config.project.next_build_delay > 0 {
                println!("Sleeping for {} seconds", state.config.project.next_build_delay);
                tokio::time::sleep(std::time::Duration::from_secs(state.config.project.next_build_delay as u64)).await;
            }

        

    }//loop forever(or until shutdown)

    
 
    {
        let mut is_queue_running = state.is_queue_running.lock().await;
        *is_queue_running = false;
    }
    
    {
        let mut project_logs = state.project_logs.lock().await;
        project_logs.clear();
    }

    // notify_on_queue_ended(&state.config.project.build.on_success_failure).await;

    //delte all the logs once get
    // let mut build_queue = state.builds.build_queue.lock().await;
    // *build_queue = Vec::new();

    println!("Bulid manager ended! Nothing to do.");


}