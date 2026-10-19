use super::*;

async fn instance(
    cookie_path: Option<PathBuf>,
) -> (
    SocketAddr,
    tokio::task::JoinHandle<anyhow::Result<()>>,
    AppState,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind owned instance");
    let addr = listener.local_addr().expect("owned instance address");
    let state = AppState::from_parts(
        addr,
        SharedCookieProfile::new(Vec::new(), cookie_path.into_iter().collect()),
        Arc::new(StoragePartitionState::open(None).expect("in-memory storage")),
        protocol_server_test_runtime_config(
            FetchConfig::default(),
            OptionalResourceFetchMask::NONE,
        ),
        crate::config::DEFAULT_SCREENCAST_INTERVAL_MS,
    );
    let serving = state.clone();
    let task = tokio::spawn(async move { serve_listener(listener, serving).await });
    (addr, task, state)
}

async fn browser(addr: SocketAddr) -> TestCdpSocket {
    connect_async(format!("ws://{addr}/devtools/browser/{DEFAULT_BROWSER_ID}"))
        .await
        .expect("connect owned browser")
        .0
}

async fn joined(task: tokio::task::JoinHandle<anyhow::Result<()>>) {
    timeout(Duration::from_secs(7), task)
        .await
        .expect("instance exits within total shutdown budget")
        .expect("server task joins")
        .expect("server completes shutdown without a timeout or join error");
}

async fn closed(socket: &mut TestCdpSocket) {
    timeout(Duration::from_secs(2), async {
        while let Some(Ok(message)) = socket.next().await {
            if matches!(message, WsMessage::Close(_)) {
                break;
            }
        }
    })
    .await
    .expect("owned socket terminates");
}

async fn close_instance(socket: &mut TestCdpSocket, id: u64) {
    let response = send_cdp_command(socket, id, "Browser.close", None, json!({})).await;
    assert_eq!(
        response_by_id(&response, id),
        &json!({"id": id, "result": {}}),
        "the close response must arrive before transport EOF"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_close_joins_its_listener_connections_and_checkpoint_only() {
    let profile = TempDir::new("browser-close-checkpoint");
    fs::create_dir_all(&profile.path).expect("own checkpoint directory");
    let cookie_path = profile.path.join("cookies.json");
    let (addr, task, state) = instance(Some(cookie_path.clone())).await;
    let (other_addr, other_task, other_state) = instance(None).await;
    let mut root = browser(addr).await;
    let mut other_root = browser(other_addr).await;
    let first = create_dynamic_target(&mut root, 1).await;
    let _second = create_dynamic_target(&mut root, 2).await;
    let mut page = connect_dynamic_page(addr, &first).await;
    let cookie = send_cdp_command(&mut page, 3, "Network.setCookie", None,
        json!({"name": "owned-close", "value": "persisted", "url": "https://browser-close.invalid/"})).await;
    assert!(response_by_id(&cookie, 3)["error"].is_null());

    // Keep both independent BiDi and Classic-attached BiDi sockets open while
    // closing the instance; upgraded connections cannot be left detached.
    let (mut bidi, _) = connect_async(format!("ws://{addr}/session"))
        .await
        .expect("BiDi");
    let bidi_session = send_cdp_command(&mut bidi, 4, "session.new", None, json!({})).await;
    assert_eq!(response_by_id(&bidi_session, 4)["type"], "success");
    let classic = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/session")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .expect("Classic session request"),
        )
        .await
        .expect("Classic response");
    assert_eq!(classic.status(), StatusCode::OK);
    let classic: serde_json::Value = serde_json::from_slice(
        &to_bytes(classic.into_body(), 65536)
            .await
            .expect("Classic body"),
    )
    .expect("Classic JSON");
    let classic_id = classic["value"]["sessionId"].as_str().expect("Classic ID");
    let (mut attached, _) = connect_async(format!("ws://{addr}/session/{classic_id}"))
        .await
        .expect("attached BiDi");
    let attached_ready =
        send_cdp_command(&mut attached, 5, "browser.getUserContexts", None, json!({})).await;
    assert_eq!(response_by_id(&attached_ready, 5)["type"], "success");
    assert!(state.classic_session_registry.has_session(classic_id));

    close_instance(&mut root, 100).await;
    joined(task).await;
    closed(&mut root).await;
    closed(&mut page).await;
    closed(&mut bidi).await;
    closed(&mut attached).await;
    assert!(
        tokio::net::TcpStream::connect(addr).await.is_err(),
        "closed listener must not accept"
    );
    assert_eq!(state.cdp_owner_registry.owner_count(), 0);
    assert!(!state.classic_session_registry.has_session(classic_id));
    assert_eq!(state.bidi_session_registry.lock().active_session_count(), 0);
    let persisted = cookie_cache::load_cookie_cache(&cookie_path).expect("checkpoint on disk");
    assert!(
        persisted
            .iter()
            .any(|cookie| cookie.name == "owned-close" && cookie.value == "persisted")
    );

    let version =
        send_cdp_command(&mut other_root, 101, "Browser.getVersion", None, json!({})).await;
    assert!(response_by_id(&version, 101)["result"]["product"].is_string());
    assert!(!other_task.is_finished(), "another instance survives");
    assert!(other_state.shutdown.connection().is_some());
    close_instance(&mut other_root, 102).await;
    joined(other_task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn page_attached_invalid_close_and_frontend_disconnect_do_not_stop_instance() {
    let (addr, task, state) = instance(None).await;
    let mut root = browser(addr).await;
    let target = create_dynamic_target(&mut root, 1).await;
    let mut page = connect_dynamic_page(addr, &target).await;
    let attach = send_cdp_command(
        &mut root,
        2,
        "Target.attachToTarget",
        None,
        json!({"targetId": target, "flatten": true}),
    )
    .await;
    let session = response_by_id(&attach, 2)["result"]["sessionId"]
        .as_str()
        .expect("attached ID");
    let page_close = send_cdp_command(&mut page, 3, "Browser.close", None, json!({})).await;
    assert!(response_by_id(&page_close, 3)["error"].is_object());
    let attached_close =
        send_cdp_command(&mut root, 4, "Browser.close", Some(session), json!({})).await;
    assert!(response_by_id(&attached_close, 4)["error"].is_object());
    let invalid =
        send_cdp_command(&mut root, 5, "Browser.close", None, json!({"force": true})).await;
    assert_eq!(response_by_id(&invalid, 5)["error"]["code"], -32602);
    root.close(None).await.expect("disconnect viewer only");
    closed(&mut root).await;
    let alive = send_cdp_command(
        &mut page,
        6,
        "Runtime.evaluate",
        None,
        json!({"expression": "21 * 2"}),
    )
    .await;
    assert_eq!(response_by_id(&alive, 6)["result"]["result"]["value"], 42);
    assert!(!task.is_finished());
    assert!(state.shutdown.connection().is_some());
    let mut replacement = browser(addr).await;
    close_instance(&mut replacement, 7).await;
    joined(task).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simultaneous_close_requests_finish_once_without_hanging() {
    let (addr, task, state) = instance(None).await;
    let mut first = browser(addr).await;
    let mut second = browser(addr).await;
    let (left, right) = tokio::join!(
        first.send(WsMessage::Text(
            json!({"id": 1, "method": "Browser.close"})
                .to_string()
                .into()
        )),
        second.send(WsMessage::Text(
            json!({"id": 2, "method": "Browser.close", "params": {}})
                .to_string()
                .into()
        ))
    );
    assert!(left.is_ok() || right.is_ok());
    async fn accepted(socket: &mut TestCdpSocket) -> bool {
        timeout(Duration::from_secs(2), async {
            while let Some(Ok(message)) = socket.next().await {
                if let WsMessage::Text(text) = message {
                    let response: serde_json::Value =
                        serde_json::from_str(&text).expect("close JSON");
                    if response["result"] == json!({}) {
                        return true;
                    }
                }
            }
            false
        })
        .await
        .unwrap_or(false)
    }
    let (left, right) = tokio::join!(accepted(&mut first), accepted(&mut second));
    assert!(
        left || right,
        "at least one close is acknowledged before exit"
    );
    joined(task).await;
    assert_eq!(state.cdp_owner_registry.owner_count(), 0);
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn incomplete_worker_drain_is_bounded_and_not_reported_as_clean_shutdown() {
    let (addr, task, state) = instance(None).await;
    let held_worker = state
        .shutdown
        .connection()
        .expect("controlled stalled worker");
    let mut root = browser(addr).await;
    close_instance(&mut root, 1).await;
    let error = timeout(Duration::from_secs(7), task)
        .await
        .expect("bounded shutdown")
        .expect("server task joins")
        .expect_err("stalled worker cannot be clean shutdown");
    assert!(error.to_string().contains("checkpoints may be incomplete"));
    assert_eq!(state.cdp_owner_registry.owner_count(), 0);
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
    drop(held_worker);
}
