use actix_web::{HttpRequest, HttpResponse, web};
use crate::auth::check_auth::is_authorized;
use crate::models::app_state::AppState;
use super::ScheduleRequest;

pub async fn schedule_notifications(
    req: HttpRequest,
    state: web::Data<AppState>,
    schedule_req: web::Json<ScheduleRequest>,
) -> actix_web::Result<HttpResponse> {
    if !is_authorized(&req, state.clone()).await {
        return Ok(HttpResponse::Unauthorized().body("Unauthorized"));
    }

    match state.notification_manager.process_request(schedule_req.into_inner()).await {
        Ok(results) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "status": "success",
                "message": "Processed scheduler request",
                "ids": results
            })))
        }
        Err(e) => {
            Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "status": "error",
                "message": e
            })))
        }
    }
}
