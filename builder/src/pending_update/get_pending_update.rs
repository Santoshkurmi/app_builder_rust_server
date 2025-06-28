
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde_json::json;

use crate::{auth::check_auth::is_authorized, models::{app_state::{AppState, BuildResponse}, status::Status}};



/// get all pending failed attempt while sending to other server
pub async fn get_pending_update(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> impl Responder {


    if !is_authorized(&req,state.clone()).await {
        let res = BuildResponse{
            message: "Unauthorized Access".to_string(),
            status: Status::Unauthorized,
            build_id: None,
            token: None
        };
        return HttpResponse::Unauthorized().json(res);
    }

    let  error_history_guard = &mut state.builds.failed_history.lock().await;
    let error_history = error_history_guard.to_vec();


    let queue_count: usize;

    {
        queue_count = state.builds.build_queue.lock().await.len();
    }

    let json_str = json!({
        "error_history": error_history,
        "queue_count": queue_count
    });

    error_history_guard.clear();

    return HttpResponse::Ok().json(json_str);
}