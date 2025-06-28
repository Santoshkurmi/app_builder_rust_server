use std::{collections::HashMap};

use actix_web::{ web, Error, HttpRequest, HttpResponse};
use actix_ws::handle;

use crate::models::app_state::{AppState, ChannelMessage};


/*
|--------------------------------------------------------------------------
| This is the route to connect to the build process output using websocket
|-----------------------------------------------------------------------
|
*/
/// connect to the build socket on a particular build
pub async fn connect_and_stream_ws_build(
    req: HttpRequest,
    stream: web::Payload,
    data: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, Error> {


    /*
    |--------------------------------------------------------------------------
    | Handle to check if token is matched or not, this is used in single place, so no need to crate middleware for that
    |--------------------------------------------------------------------------
    |
    */
    let unique_build_key = &data.config.project.build.unique_build_key;

    // let build_id = query.get(unique_build_key).clone(); 
    let token = query.get("token").clone(); 
    println!("Connecting to build websocket");
    println!("Token: {:?}",token);
    if let None = token {
        return Ok(HttpResponse::Unauthorized().body("Socket Token is Required"));
    }
    // println!("Token: {:?}",token);

    // if let None = build_id {
    //     return Ok(HttpResponse::Unauthorized().body(format!("No build id found with key {} ",unique_build_key)));
    // }
    // let build_id = build_id.unwrap();
    let token = token.unwrap();

    println!("Connecting to build websocket 2");

    let state = data.as_ref().clone();
    // let current_token_lock = state.token.lock().await;

    let current_build_guard = state.builds.current_build.lock().await;
    if current_build_guard.is_none() {
        return Ok(HttpResponse::Unauthorized().body("No build is running"));
    }
    let current_build  = current_build_guard.as_ref().unwrap();

    if &current_build.socket_token != token {
        return Ok(HttpResponse::Unauthorized().body("Invalid token"));
    }
    println!("Connecting to build websocket success");


    let (res, mut session, _msg_stream) = handle(&req, stream)?;

    // Send old buffered messages first
    {
        let buf = current_build.logs.clone();
        drop(current_build_guard);
        let json_array = serde_json::to_string(&*buf).unwrap();
        // for line in buf.iter() {
            let _ = session.text(json_array).await;
        // }
    }

    // Subscribe to broadcast channel
    let mut rx = data.build_sender.subscribe();
    
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

