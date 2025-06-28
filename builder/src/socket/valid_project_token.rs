use std::collections::HashMap;

use actix_web::{web, HttpRequest, HttpResponse, Responder};

use crate::{auth::check_auth::is_authorized, helpers::utils::{create_file_with_dirs_and_content, save_token_to_user_home}, models::{app_state::{AppState, BuildResponse}, status::Status}};



pub async fn set_valid_project_token(req: HttpRequest,payload: web::Json<HashMap<String,String>>,state: web::Data<AppState>,)-> impl Responder {

    if !is_authorized(&req,state.clone()).await {
        let res = BuildResponse{
            message: "Unauthorized Access".to_string(),
            status: Status::Unauthorized,
            build_id: None,
            token: None
        };
        return HttpResponse::Unauthorized().json(res);
    }

    

    let project_token_s = payload.get("project_token");
    if project_token_s.is_none() {
        let res = BuildResponse{
            message: "Missing project token".to_string(),
            status: Status::MissingProjectToken,
            build_id: None,
            token: None
        };
        return HttpResponse::Unauthorized().json(res);
    }

    let mut project_token = state.project_token.lock().await;

    let project_token_s = project_token_s.unwrap();
    
    println!("project_token_s: {}", project_token_s);

    let is_created = save_token_to_user_home(&state.config.token_path.as_str(), project_token_s);
    if is_created.is_err() {
        let res = BuildResponse{
            message: "Failed to save project token".to_string(),
            status: Status::Error,
            build_id: None,
            token: None
        };
        return HttpResponse::BadRequest().json(res);
    }



    project_token.replace(project_token_s.to_string());

        let res = BuildResponse{
            message: "Changed the project token".to_string(),
            status: Status::ChangeProjectToken,
            build_id: None,
            token: None
        };
        return HttpResponse::Ok().json(res);
   



}