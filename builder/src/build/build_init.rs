
use actix_web::{HttpRequest, HttpResponse, Responder,  web};
use uuid::Uuid;

use std::{collections::HashMap};

use crate::{auth::check_auth::is_authorized, build::build_manager::{ build_manager}, helpers::utils::{create_file_with_dirs_and_content, generate_token, secure_join_path}, models::{app_state::{AppState,  BuildRequest, BuildResponse,  ChannelMessage, ProjectLog}, config::{ PayloadType}, status::Status}};


/// Initialize a build
pub async fn build_initialize(
    req: HttpRequest,
    payload: web::Json<HashMap<String, String>>,
    state: web::Data<AppState>,
) -> impl Responder {

  
    if !is_authorized(&req,state.clone()).await {
        
        let res = BuildResponse{
            message:"Unauthorized Access".to_string(),
            status: Status::MissingUniqueId,
            build_id: None,
            token: None
        };

        return HttpResponse::Unauthorized().json(res);
    }

    {
        let project_token = payload.get("project_token");
        if project_token.is_none() {
            
            return HttpResponse::Unauthorized().body("Missing project token key project_token");
        }

        {
            let project_token = project_token.unwrap();
            let mut project_token_lock = state.project_token.lock().await;
            *project_token_lock = Some(project_token.to_string());
        }

    }

    let unique_id = payload.get(&state.config.project.build.unique_build_key);

    if unique_id.is_none() {
        let res = BuildResponse{
            message: format!("Missing unique build key: {}", state.config.project.build.unique_build_key),
            status: Status::MissingUniqueId,
            build_id: None,
            token: None
        };
        return HttpResponse::BadRequest().json(res);
    }

    for reqired_payload in &state.config.project.build.payload {
        if ! payload.contains_key(&reqired_payload.key1) {
            let res = BuildResponse{
                message: format!("Missing payload key: {}", reqired_payload.key1),
                status: Status::MissingPayload,
                build_id: None,
                token: None
            };
            return HttpResponse::BadRequest().json(res);
        }//if key not found
    }



    if state.config.project.max_pending_build == state.builds.build_queue.lock().await.len() as u32{
        let res = BuildResponse{
            message: format!("Max Pending Reached: {}", state.config.project.max_pending_build),
            build_id: None,
            token: None,
            status: Status::MaxPending,
        };
        return HttpResponse::Conflict().json(res);
    }

    let mut is_already_running = false;
    let guard = state.builds.current_build.lock().await;

    if let Some(current_build) = &*guard {
        if &current_build.unique_id == unique_id.unwrap()  {
            let res = BuildResponse{
            message: format!("Build already in progress: {}", current_build.unique_id),
            build_id: Some(current_build.id.clone()),
            token: Some( current_build.socket_token.clone() ),
            status: Status::AlreadyBuilding,
        };
            return HttpResponse::Conflict().json(res);
        }
        is_already_running = true;
    }//if current build exists

    drop(guard);

    let mut build_queue = state.builds.build_queue.lock().await;
    
    let is_already_queued = build_queue.iter().any(|build| {
        &build.unique_id == unique_id.unwrap()
    });

    if is_already_queued {
        let res = BuildResponse{
            message: format!("Build already in queue"),
            token: None,
            build_id: None,
            status: Status::AlreadyQueue,
        };
        return HttpResponse::Conflict().json(res);
    }


    for reqired_payload in &state.config.project.build.payload {
        if reqired_payload.r#type != PayloadType::File{
            continue;
        }//continue if not file
        let file_path = if reqired_payload.key2.is_some() {
            reqired_payload.key2.as_ref().unwrap()
        } else {
            reqired_payload.key1.as_str()
        };

        // let path_relative = format!("{}/{}", state.config.project.project_path, file_path);

        let path_relative = secure_join_path(&state.config.project.project_path, &file_path);
        if path_relative.is_none(){
            let res = BuildResponse{
                message: format!("Failed to create payload file: Path is not secure"),
                status: Status::FileCreateFailed,
                build_id: None,
                token: None
            };
            return HttpResponse::BadRequest().json(res);
        }

        let path_relative = path_relative.unwrap();

        let create_path = create_file_with_dirs_and_content(&path_relative, payload.get(&reqired_payload.key1).unwrap().as_str());
        if let Err(e) = create_path {
            let res = BuildResponse{
                message: format!("Failed to create payload file: {}", e),
                status: Status::FileCreateFailed,
                build_id: None,
                token: None
            };
            return HttpResponse::BadRequest().json(res);
        }
        
    }
   

    let new_token = generate_token(32);
    let id = Uuid::new_v4();

    let build_state =  BuildRequest{
        id: id.to_string(),
        unique_id: unique_id.unwrap().to_string(),
        payload: payload.clone(),
        socket_token: new_token.clone(),
    };

    build_queue.push(build_state);
    drop(build_queue);

    let project_log = ProjectLog{
        id: id.to_string(),
        unique_id: unique_id.unwrap().to_string(),
        socket_token: new_token.clone(),
        step: 0,
        total_steps: state.config.project.build.commands.len() as usize,
        timestamp: chrono::Local::now(),
        state: if !*state.is_queue_running.lock().await {Status::Building} else {Status::Pending},
        message: "In Queue".to_string()
    
    };
    {

        let project_log_json = serde_json::to_string(&project_log).unwrap();
        let _ = state.project_sender.send(ChannelMessage::Data(project_log_json));
        
        let mut project_logs = state.project_logs.lock().await;
        project_logs.push(project_log);
    }

    println!("Starting build manager to handle build for {}", unique_id.unwrap());


    if !*state.is_queue_running.lock().await{


        tokio::spawn(async move {
            build_manager(state.clone()).await;
        });
    }
     
    let res = BuildResponse{
        message:  if is_already_running {"Build is in pending state".to_string()} else {"Build started".to_string()},
        token: Some(new_token.clone()),
        build_id: Some( id.to_string() ),
        status:if is_already_running {Status::Pending} else {Status::Building},
    }; //need to handle here

    


    HttpResponse::Ok().json(res)
}