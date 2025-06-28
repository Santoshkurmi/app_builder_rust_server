use std::{collections::HashMap};

use actix_web::{ web, Error, HttpRequest, HttpResponse};
use actix_ws::handle;

use crate::models::app_state::{AppState, ChannelMessage};

/// connect to the project socket on the project in whole
pub async fn connect_and_stream_ws_project(
    req: HttpRequest,
    stream: web::Payload,
    data: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, Error> {


    // /*
    // |--------------------------------------------------------------------------
    // | Handle to check if token is matched or not, this is used in single place, so no need to crate middleware for that
    // |--------------------------------------------------------------------------
    // |
    // */
    let token = query.get("token").clone(); 
   



    if token.is_none() {
        return Ok(HttpResponse::Unauthorized().body("Missing token"));
    }

    let token = token.unwrap();

    let state = data.as_ref().clone();
    // // let current_token_lock = state.token.lock().await;

    let project_token_guard = state.project_token.lock().await;
    if project_token_guard.is_none() {
        return Ok(HttpResponse::Unauthorized().body("Invalid Token"));
    }
    let project_token  = project_token_guard.as_ref().unwrap();
    
    if project_token != token {
        return Ok(HttpResponse::Unauthorized().body("Invalid token"));
    }

    drop(project_token_guard);

    // drop(project_token_guard);
    println!("Connecting to project websocket");


    let (res, mut session, _msg_stream) = handle(&req, stream)?;

    // Send old buffered messages first
    {
        let buf = state.project_logs.lock().await;
        
        let json_array = serde_json::to_string(&*buf).unwrap_or_default();
        drop(buf);
        // for line in buf.iter() {
            let _ = session.text(json_array).await;
        // }
    }

    // Subscribe to broadcast channel
    let mut rx = data.project_sender.subscribe();
    
    // Stream new output to client
    actix_web::rt::spawn(async move {
        while let Ok(line) = rx.recv().await {


            match line {
                ChannelMessage::Data(data) => {
                    if session.text(data).await.is_err(){
                        session.close(None).await.unwrap_or_default();
                        break;
                    };
                }
                ChannelMessage::Shutdown => {
                    session.close(None).await.unwrap_or_default();
                    break;
                }
                
            }

           
        }
    });

    Ok(res)
}