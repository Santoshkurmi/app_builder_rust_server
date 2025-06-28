use std::collections::HashMap;

use actix_web::{web, HttpRequest, HttpResponse, Responder};

use crate::{auth::check_auth::is_authorized, models::{app_state::{AppState, BuildResponse}, status::Status}};


/// abort a particular build
pub async fn abort(req: HttpRequest,payload: web::Json<HashMap<String, String>>,state: web::Data<AppState>,)-> impl Responder {

     if !is_authorized(&req,state.clone()).await {
        
        let res = BuildResponse{
            message:"Unauthorized Access".to_string(),
            status: Status::MissingUniqueId,
            build_id: None,
            token: None
        };

        return HttpResponse::Unauthorized().json(res);
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

    let guard = state.builds.current_build.lock().await;

    if let Some(current_build) = &*guard {
        if &current_build.unique_id == unique_id.unwrap()  {
            let mut is_terminated = state.is_terminated.lock().await;
            *is_terminated = true;
            println!("Terminating build");

            let res = BuildResponse{
                message: "Aborted".to_string(),
                status: Status::Aborted,
                build_id: Some( current_build.id.clone() ),
                token: None
            };
            return HttpResponse::Ok().json(res);
        }
    }

    drop(guard);

    let mut queue = state.builds.build_queue.lock().await;
    if let Some(index) = queue.iter().position(|build| {
        &build.unique_id == unique_id.unwrap()
    }) {
        let id = queue.get(index).unwrap().id.clone();
        queue.remove(index);
        let res = BuildResponse{
            message: "Aborted".to_string(),
            status: Status::Aborted,
            build_id: Some(id),
            token: None
            };
        return HttpResponse::Ok().json(res);
    }

    let res = BuildResponse{
        message: "No Build Found".to_string(),
        status: Status::NotFound,
        build_id: None,
        token: None
    };

    HttpResponse::BadRequest().json(res)


}


pub async fn abort_all(req: HttpRequest,state: web::Data<AppState>,)-> impl Responder {

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
        let mut is_terminated = state.is_terminated.lock().await;
        *is_terminated = true;

    }

   
    let mut queue = state.builds.build_queue.lock().await;
    queue.clear();

    let res = BuildResponse{
        message: "Aborted".to_string(),
        status: Status::Aborted,
        build_id: None,
        token: None
    };
    

    HttpResponse::Ok().json(res)


}