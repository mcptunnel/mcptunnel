use rmcp::{
    model::{ProgressNotificationParam, ProgressToken},
    service::{NotificationContext, RoleClient, RoleServer},
    ClientHandler, Peer,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct ProgressSink {
    pub peer: Peer<RoleServer>,
    pub token: ProgressToken,
}

#[derive(Clone, Default)]
pub struct UpstreamHandler {
    // Hold registration across send: a fast notification cannot beat insertion of its token.
    pub registration: Arc<tokio::sync::Mutex<()>>,
    routes: Arc<Mutex<HashMap<ProgressToken, ProgressSink>>>,
}

pub struct ProgressRoute {
    handler: UpstreamHandler,
    token: ProgressToken,
}
impl Drop for ProgressRoute {
    fn drop(&mut self) {
        self.handler.routes.lock().unwrap().remove(&self.token);
    }
}
impl UpstreamHandler {
    pub fn register(&self, token: ProgressToken, sink: Option<ProgressSink>) -> ProgressRoute {
        if let Some(sink) = sink {
            self.routes.lock().unwrap().insert(token.clone(), sink);
        }
        ProgressRoute {
            handler: self.clone(),
            token,
        }
    }
}
impl ClientHandler for UpstreamHandler {
    async fn on_progress(
        &self,
        mut params: ProgressNotificationParam,
        _: NotificationContext<RoleClient>,
    ) {
        let sink = {
            let _gate = self.registration.lock().await;
            self.routes
                .lock()
                .unwrap()
                .get(&params.progress_token)
                .cloned()
        };
        if let Some(sink) = sink {
            params.progress_token = sink.token;
            // Backpressure and disconnect cancellation remain SDK-owned.
            let _ = sink.peer.notify_progress(params).await;
        }
    }
}
