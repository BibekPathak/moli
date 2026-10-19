use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::Duration;

use super::instance_shutdown::InstanceShutdown;
use crate::{
    cdp_frontend::CdpFrontendEndpoint,
    cdp_writer::{CdpSocketSink, spawn_socket_sink},
};

const BROWSER_CLOSE_FLUSH_BUDGET: Duration = Duration::from_secs(1);

pub(super) enum CdpFrontendSocketKind {
    Browser,
    Target { target_id: String },
}

pub(super) async fn run_cdp_frontend_socket(
    socket: WebSocket,
    endpoint: CdpFrontendEndpoint,
    kind: CdpFrontendSocketKind,
    shutdown: InstanceShutdown,
) -> bool {
    let Some(_connection) = shutdown.connection() else {
        return false;
    };
    let browser_socket = matches!(&kind, CdpFrontendSocketKind::Browser);
    let (socket_tx, mut socket_rx) = socket.split();
    let (sink, mut writer_finished_rx) = spawn_socket_sink(socket_tx);
    let socket_sink = sink.clone();
    let frontend_id = match &kind {
        CdpFrontendSocketKind::Browser => endpoint.attach_browser(sink).await,
        CdpFrontendSocketKind::Target { target_id } => {
            endpoint.attach_target(target_id.clone(), sink).await
        }
    };
    let frontend_id = match frontend_id {
        Ok(frontend_id) => frontend_id,
        Err(error) => {
            tracing::warn!(?error, "failed to attach CDP WebSocket frontend");
            return false;
        }
    };
    let mut detach_guard = CdpFrontendDetachGuard {
        endpoint: endpoint.clone(),
        frontend_id: Some(frontend_id),
        kind: match kind {
            CdpFrontendSocketKind::Browser => CdpFrontendKind::Browser,
            CdpFrontendSocketKind::Target { .. } => CdpFrontendKind::Target,
        },
    };

    let exit = loop {
        tokio::select! {
            _ = shutdown.requested() => break CdpSocketExit::TransportClose,
            _ = endpoint.wait_for_shutdown() => break CdpSocketExit::TransportClose,
            _ = &mut writer_finished_rx => break CdpSocketExit::WriterFinished,
            maybe_message = socket_rx.next() => {
                let Some(Ok(message)) = maybe_message else {
                    break CdpSocketExit::TransportClose;
                };
                match message {
                    Message::Text(text) => {
                        if let Some((response, should_close)) = browser_close_response(&text, browser_socket) {
                            if should_close {
                                acknowledge_browser_close(&socket_sink, &shutdown, response).await;
                                break CdpSocketExit::TransportClose;
                            }
                            if !socket_sink.enqueue_owned_message(response) {
                                break CdpSocketExit::TransportClose;
                            }
                            continue;
                        }
                        if !endpoint.command(frontend_id, text.to_string()).await {
                            break CdpSocketExit::TransportClose;
                        }
                    }
                    Message::Close(_) => break CdpSocketExit::PeerClose,
                    _ => {}
                }
            }
        }
    };
    detach_guard.detach();
    match exit {
        CdpSocketExit::WriterFinished => wait_for_peer_close(&mut socket_rx).await,
        CdpSocketExit::PeerClose => {
            socket_sink.peer_close_received();
            let _ = writer_finished_rx.await;
        }
        CdpSocketExit::TransportClose => {
            socket_sink.close();
            let _ = writer_finished_rx.await;
        }
    }
    true
}

// Instance control belongs to the browser-root transport, not an attached page
// or an individual owner actor. Every other command keeps its existing route.
fn browser_close_response(text: &str, browser_socket: bool) -> Option<(Value, bool)> {
    if !browser_socket {
        return None;
    }
    let command: Value = serde_json::from_str(text).ok()?;
    if command.get("method")?.as_str()? != "Browser.close" || command.get("sessionId").is_some() {
        return None;
    }
    let id = command.get("id")?.as_u64()?;
    if command
        .get("params")
        .is_some_and(|params| !params.as_object().is_some_and(serde_json::Map::is_empty))
    {
        return Some((
            json!({"id": id, "error": {"code": -32602, "message": "InvalidParams"}}),
            false,
        ));
    }
    Some((json!({"id": id, "result": {}}), true))
}

async fn acknowledge_browser_close(
    sink: &CdpSocketSink,
    shutdown: &InstanceShutdown,
    response: Value,
) -> bool {
    if !sink.enqueue_owned_message(response) {
        return false;
    }
    if !matches!(
        tokio::time::timeout(BROWSER_CLOSE_FLUSH_BUDGET, sink.flush()).await,
        Ok(true)
    ) {
        return false;
    }
    shutdown.request();
    true
}

enum CdpSocketExit {
    WriterFinished,
    PeerClose,
    TransportClose,
}

async fn wait_for_peer_close(socket_rx: &mut futures_util::stream::SplitStream<WebSocket>) {
    let wait = async {
        while let Some(message) = socket_rx.next().await {
            match message {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(1), wait).await;
}

enum CdpFrontendKind {
    Browser,
    Target,
}

struct CdpFrontendDetachGuard {
    endpoint: CdpFrontendEndpoint,
    frontend_id: Option<u64>,
    kind: CdpFrontendKind,
}

impl CdpFrontendDetachGuard {
    fn detach(&mut self) {
        let Some(frontend_id) = self.frontend_id.take() else {
            return;
        };
        match self.kind {
            CdpFrontendKind::Browser => self.endpoint.detach_browser(frontend_id),
            CdpFrontendKind::Target => self.endpoint.detach_target(frontend_id),
        }
    }
}

impl Drop for CdpFrontendDetachGuard {
    fn drop(&mut self) {
        self.detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_valid_browser_root_close_can_request_instance_shutdown() {
        for params in [
            json!({"id": 7, "method": "Browser.close"}),
            json!({"id": 7, "method": "Browser.close", "params": {}}),
        ] {
            let (response, close) = browser_close_response(&params.to_string(), true).unwrap();
            assert!(close);
            assert_eq!(response, json!({"id": 7, "result": {}}));
        }
        for command in [
            json!({"id": 7, "method": "Browser.close", "sessionId": "page"}),
            json!({"id": 7, "method": "Browser.close", "sessionId": null}),
            json!({"id": -1, "method": "Browser.close"}),
            json!({"id": "7", "method": "Browser.close"}),
            json!({"method": "Browser.close"}),
            json!({"id": 7, "method": "browser.close"}),
        ] {
            assert!(browser_close_response(&command.to_string(), true).is_none());
        }
        assert!(browser_close_response(r#"{"id":7,"method":"Browser.close"}"#, false).is_none());
        for params in [json!(null), json!([]), json!({"force": true})] {
            let (response, close) = browser_close_response(
                &json!({"id": 7, "method": "Browser.close", "params": params}).to_string(),
                true,
            )
            .unwrap();
            assert!(!close);
            assert_eq!(response["error"]["code"], -32602);
        }
    }

    #[tokio::test]
    async fn failed_or_unconfirmed_flush_does_not_stop_the_instance() {
        let shutdown = InstanceShutdown::default();
        let sink = CdpSocketSink::for_test();
        assert!(!acknowledge_browser_close(&sink, &shutdown, json!({"id": 1, "result": {}})).await);
        let (sink, _stalled_writer) = CdpSocketSink::with_stalled_writer_for_test(2);
        assert!(!acknowledge_browser_close(&sink, &shutdown, json!({"id": 2, "result": {}})).await);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), shutdown.requested())
                .await
                .is_err()
        );
        assert!(shutdown.connection().is_some());
    }
}
