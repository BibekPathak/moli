// Tests grouped by behavior. Shared fixtures live in the parent module.
use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "ok"),
            ],
            "runtime fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 199).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 200,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": { "expression": "fetch('/api').then(r => r.text())" }
    }))
    .await;

    let response = take_response_by_id(&mut ctx, 200);
    assert_eq!(response["id"], 200);

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "ok"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 201,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        201,
        json!({
            "body": "runtime fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_call_function_on_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "ok"),
            ],
            "runtime fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let execution_context_id =
        enable_runtime_and_take_execution_context_id_async(&mut ctx, 1980).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 1981,
        "method": "Runtime.callFunctionOn",
        "sessionId": "SID-1",
        "params": {
            "functionDeclaration": "() => fetch('/api').then(r => r.text())",
            "executionContextId": execution_context_id
        }
    }))
    .await;

    let response = take_response_by_id(&mut ctx, 1981);
    assert_eq!(response["id"], 1981);

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime callFunctionOn fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_call_function_on_xhr_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn xhr() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-xhr", "ok"),
            ],
            "runtime xhr body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/xhr", get(xhr)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let execution_context_id =
        enable_runtime_and_take_execution_context_id_async(&mut ctx, 1982).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 1983,
        "method": "Runtime.callFunctionOn",
        "sessionId": "SID-1",
        "params": {
            "functionDeclaration": "() => { const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send(); return 'scheduled'; }",
            "executionContextId": execution_context_id
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 1983);
    assert_eq!(response["id"], 1983);
    assert_eq!(response["result"]["result"]["type"], json!("string"));
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .expect("runtime callFunctionOn xhr request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], xhr_url);

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_fetch_applies_extra_http_headers() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api(headers: axum::http::HeaderMap) -> impl IntoResponse {
        let received = headers
            .get("x-cdp-test")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "ok"),
            ],
            received.to_owned(),
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    ctx.process_async(json!({
        "id": 489,
        "method": "Network.enable",
        "sessionId": "SID-1"
    }))
    .await;
    let _ = take_response_by_id(&mut ctx, 489);
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 490).await;
    ctx.process_async(json!({
        "id": 491,
        "method": "Network.setExtraHTTPHeaders",
        "sessionId": "SID-1",
        "params": { "headers": { "x-cdp-test": "runtime-fetch" } }
    }))
    .await;
    let _ = take_response_by_id(&mut ctx, 491);
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 492,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": { "expression": "fetch('/api').then(r => r.text())" }
    }))
    .await;

    let response = take_response_by_id(&mut ctx, 492);
    assert_eq!(response["id"], 492);

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime fetch request event");
    assert_eq!(request["params"]["request"]["url"], api_url);
    assert_eq!(
        request["params"]["request"]["headers"]["x-cdp-test"],
        "runtime-fetch"
    );
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime fetch request id")
        .to_owned();

    ctx.process_async(json!({
        "id": 493,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        493,
        json!({
            "body": "runtime-fetch",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_xhr_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn xhr() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-xhr", "ok"),
            ],
            "runtime xhr body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/xhr", get(xhr)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 204).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 202,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send(); return xhr.responseText; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 202);
    assert_eq!(response["id"], 202);

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .expect("runtime xhr request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], xhr_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime xhr request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime xhr response event");
    assert_eq!(response_event["params"]["type"], "XHR");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-xhr"],
        "ok"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 203,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        203,
        json!({
            "body": "runtime xhr body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_xhr_applies_extra_http_headers() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn xhr(headers: axum::http::HeaderMap) -> impl IntoResponse {
        let received = headers
            .get("x-cdp-test")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-xhr", "ok"),
            ],
            received.to_owned(),
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/xhr", get(xhr)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    ctx.process_async(json!({
        "id": 493,
        "method": "Network.enable",
        "sessionId": "SID-1"
    }))
    .await;
    let _ = take_response_by_id(&mut ctx, 493);
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 494).await;
    ctx.process_async(json!({
        "id": 495,
        "method": "Network.setExtraHTTPHeaders",
        "sessionId": "SID-1",
        "params": { "headers": { "x-cdp-test": "runtime-xhr" } }
    }))
    .await;
    let _ = take_response_by_id(&mut ctx, 495);
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 496,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send(); return xhr.responseText; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 496);
    assert_eq!(response["id"], 496);

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .expect("runtime xhr request event");
    assert_eq!(request["params"]["request"]["url"], xhr_url);
    assert_eq!(
        request["params"]["request"]["headers"]["x-cdp-test"],
        "runtime-xhr"
    );
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime xhr request id")
        .to_owned();

    ctx.process_async(json!({
        "id": 497,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        497,
        json!({
            "body": "runtime-xhr",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_set_timeout_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "timeout"),
            ],
            "runtime timeout fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 206).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 207,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { setTimeout(() => { fetch('/api').then(r => r.text()); }, 0); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 207);
    assert_eq!(response["id"], 207);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime timeout fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime timeout fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime timeout fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "timeout"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 208,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        208,
        json!({
            "body": "runtime timeout fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_set_timeout_xhr_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn xhr() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-xhr", "timeout"),
            ],
            "runtime timeout xhr body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/xhr", get(xhr)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 209).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 210,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { setTimeout(() => { const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send(); }, 0); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 210);
    assert_eq!(response["id"], 210);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .expect("runtime timeout xhr request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], xhr_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime timeout xhr request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime timeout xhr response event");
    assert_eq!(response_event["params"]["type"], "XHR");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-xhr"],
        "timeout"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 211,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        211,
        json!({
            "body": "runtime timeout xhr body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_set_interval_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "interval"),
            ],
            "runtime interval fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 2110).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 2111,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { const id = setInterval(() => { clearInterval(id); fetch('/api').then(r => r.text()); }, 0); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 2111);
    assert_eq!(response["id"], 2111);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let requests = ctx
        .sent
        .iter()
        .filter(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        requests.len(),
        1,
        "setInterval fetch should fire once after clearInterval"
    );
    let request = requests[0].clone();
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime interval fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime interval fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "interval"
    );

    ctx.process_async(json!({
        "id": 2112,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        2112,
        json!({
            "body": "runtime interval fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_set_interval_xhr_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn xhr() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-xhr", "interval"),
            ],
            "runtime interval xhr body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/xhr", get(xhr)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 2113).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 2114,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { const id = setInterval(() => { clearInterval(id); const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send(); }, 0); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 2114);
    assert_eq!(response["id"], 2114);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let requests = ctx
        .sent
        .iter()
        .filter(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        requests.len(),
        1,
        "setInterval xhr should fire once after clearInterval"
    );
    let request = requests[0].clone();
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], xhr_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime interval xhr request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime interval xhr response event");
    assert_eq!(response_event["params"]["type"], "XHR");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-xhr"],
        "interval"
    );

    ctx.process_async(json!({
        "id": 2115,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        2115,
        json!({
            "body": "runtime interval xhr body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_request_animation_frame_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "raf"),
            ],
            "runtime raf fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 212).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 213,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { requestAnimationFrame(() => { fetch('/api').then(r => r.text()); }); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 213);
    assert_eq!(response["id"], 213);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime raf fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime raf fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime raf fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "raf"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 214,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        214,
        json!({
            "body": "runtime raf fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_request_animation_frame_xhr_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn xhr() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-xhr", "raf"),
            ],
            "runtime raf xhr body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/xhr", get(xhr)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 215).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 216,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { requestAnimationFrame(() => { const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send(); }); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 216);
    assert_eq!(response["id"], 216);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .expect("runtime raf xhr request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], xhr_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime raf xhr request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime raf xhr response event");
    assert_eq!(response_event["params"]["type"], "XHR");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-xhr"],
        "raf"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 217,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        217,
        json!({
            "body": "runtime raf xhr body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_request_idle_callback_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "idle"),
            ],
            "runtime idle fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 218).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 219,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { requestIdleCallback(deadline => { globalThis.__lm_idle_did_timeout = deadline.didTimeout; globalThis.__lm_idle_time_remaining = deadline.timeRemaining() > 0; fetch('/api'); }); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 219);
    assert_eq!(response["id"], 219);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime idle fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime idle fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime idle fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "idle"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 220,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "globalThis.__lm_idle_did_timeout === false && globalThis.__lm_idle_time_remaining === true"
        }
    })).await;
    let idle_meta = take_response_by_id(&mut ctx, 220);
    assert_eq!(idle_meta["result"]["result"]["value"], true);

    ctx.process_async(json!({
        "id": 221,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        221,
        json!({
            "body": "runtime idle fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_request_idle_callback_xhr_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn xhr() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-xhr", "idle"),
            ],
            "runtime idle xhr body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/xhr", get(xhr)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 222).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 223,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { requestIdleCallback(() => { const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send(); }); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 223);
    assert_eq!(response["id"], 223);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .expect("runtime idle xhr request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], xhr_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime idle xhr request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime idle xhr response event");
    assert_eq!(response_event["params"]["type"], "XHR");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-xhr"],
        "idle"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 224,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        224,
        json!({
            "body": "runtime idle xhr body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_promise_then_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "promise"),
            ],
            "runtime promise fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 225).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 226,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { Promise.resolve().then(() => { fetch('/api').then(r => r.text()); }); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 226);
    assert_eq!(response["id"], 226);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime promise fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime promise fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime promise fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "promise"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 227,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        227,
        json!({
            "body": "runtime promise fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_promise_then_xhr_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn xhr() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-xhr", "promise"),
            ],
            "runtime promise xhr body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/xhr", get(xhr)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 228).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 229,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": "(() => { Promise.resolve().then(() => { const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send(); }); return 'scheduled'; })()"
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 229);
    assert_eq!(response["id"], 229);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .expect("runtime promise xhr request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], xhr_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime promise xhr request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime promise xhr response event");
    assert_eq!(response_event["params"]["type"], "XHR");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-xhr"],
        "promise"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 230,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        230,
        json!({
            "body": "runtime promise xhr body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_window_post_message_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "postmessage"),
            ],
            "runtime postmessage fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 230).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 231,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": r#"(() => {
  window.addEventListener('message', () => {
fetch('/api').then(r => r.text());
  }, { once: true });
  window.postMessage('go', '*');
  return 'scheduled';
})()"#
        }
    }))
    .await;

    let response = take_response_by_id(&mut ctx, 231);
    assert_eq!(response["id"], 231);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime postMessage fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime postMessage fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime postMessage fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "postmessage"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 232,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        232,
        json!({
            "body": "runtime postmessage fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_mutation_observer_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "mutation"),
            ],
            "runtime mutation fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 233).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 234,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": r#"(() => {
  const observer = new MutationObserver(() => {
fetch('/api').then(r => r.text());
observer.disconnect();
  });
  observer.observe(document.body, { attributes: true });
  document.body.setAttribute('data-trigger', '1');
  return 'scheduled';
})()"#
        }
    }))
    .await;

    let response = take_response_by_id(&mut ctx, 234);
    assert_eq!(response["id"], 234);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime mutation fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime mutation fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime mutation fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "mutation"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 235,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        235,
        json!({
            "body": "runtime mutation fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_intersection_observer_fetch_emits_subresource_network_events() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body><div id='target'>ok</div></body></html>",
        )
    }

    async fn api() -> impl IntoResponse {
        (
            [
                (CONTENT_TYPE.as_str(), "text/plain"),
                ("x-runtime-fetch", "intersection"),
            ],
            "runtime intersection fetch body",
        )
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/page", get(page))
                .route("/api", get(api)),
        )
        .await
        .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 236).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 237,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": r#"(() => {
  const observer = new IntersectionObserver(() => {
fetch('/api').then(r => r.text());
observer.disconnect();
  });
  observer.observe(document.getElementById('target'));
  return 'scheduled';
})()"#
        }
    }))
    .await;

    let response = take_response_by_id(&mut ctx, 237);
    assert_eq!(response["id"], 237);
    assert_eq!(response["result"]["result"]["value"], json!("scheduled"));

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network completion",
        |message| message["method"] == json!("Network.loadingFinished"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime intersection fetch request event");
    assert_eq!(request["sessionId"], "SID-1");
    assert_eq!(request["params"]["documentURL"], page_url);
    assert_eq!(request["params"]["request"]["url"], api_url);
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime intersection fetch request id")
        .to_owned();

    let response_event = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.responseReceived")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime intersection fetch response event");
    assert_eq!(response_event["params"]["type"], "Fetch");
    assert_eq!(
        response_event["params"]["response"]["headers"]["x-runtime-fetch"],
        "intersection"
    );

    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Network.loadingFinished")
            && message["params"]["requestId"] == json!(request_id)
    }));

    ctx.process_async(json!({
        "id": 238,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_result(
        238,
        json!({
            "body": "runtime intersection fetch body",
            "base64Encoded": false
        }),
        Some("SID-1"),
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_fetch_failure_emits_loading_failed() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    let (failing_addr, failing_server) = spawn_connection_drop_server().await;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/page", get(page)))
            .await
            .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let api_url = format!("http://{failing_addr}/api");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 202).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 203,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": { "expression": format!("fetch('{api_url}').catch(() => 'failed')") }
    }))
    .await;

    let response = take_response_by_id(&mut ctx, 203);
    assert_eq!(response["id"], 203);

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network failure",
        |message| message["method"] == json!("Network.loadingFailed"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("Fetch")
        })
        .cloned()
        .expect("runtime fetch request event");
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime fetch request id")
        .to_owned();
    assert_eq!(request["params"]["request"]["url"], api_url);

    let failed = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.loadingFailed")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime fetch loadingFailed event");
    assert_eq!(failed["params"]["type"], "Fetch");
    assert_eq!(failed["params"]["canceled"], false);
    assert!(
        failed["params"]["errorText"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );

    ctx.process_async(json!({
        "id": 204,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_error(
        204,
        -32000,
        "No data found for resource with given identifier",
    );

    failing_server.abort();
    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_evaluate_xhr_failure_emits_loading_failed() {
    async fn page() -> impl IntoResponse {
        (
            [(CONTENT_TYPE.as_str(), "text/html")],
            "<!doctype html><html><body>ok</body></html>",
        )
    }

    let (failing_addr, failing_server) = spawn_connection_drop_server().await;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/page", get(page)))
            .await
            .unwrap();
    });

    let page_url = format!("http://{addr}/page");
    let xhr_url = format!("http://{failing_addr}/xhr");
    let mut ctx = TestContext::new();
    with_loaded_http_document_async(&mut ctx, &page_url, "SID-1", "TID-1").await;
    let _ = enable_runtime_and_take_execution_context_id_async(&mut ctx, 205).await;
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 206,
        "method": "Runtime.evaluate",
        "sessionId": "SID-1",
        "params": {
            "expression": format!("new Promise(resolve => {{ const xhr = new XMLHttpRequest(); xhr.onerror = () => resolve('failed'); xhr.open('GET', '{xhr_url}'); xhr.send(); }})")
        }
    })).await;

    let response = take_response_by_id(&mut ctx, 206);
    assert_eq!(response["id"], 206);

    wait_until_message(
        &mut ctx,
        Some("SID-1"),
        "runtime subresource network failure",
        |message| message["method"] == json!("Network.loadingFailed"),
    )
    .await;

    let request = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.requestWillBeSent")
                && message["params"]["type"] == json!("XHR")
        })
        .cloned()
        .expect("runtime xhr request event");
    let request_id = request["params"]["requestId"]
        .as_str()
        .expect("runtime xhr request id")
        .to_owned();
    assert_eq!(request["params"]["request"]["url"], xhr_url);

    let failed = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Network.loadingFailed")
                && message["params"]["requestId"] == json!(request_id)
        })
        .cloned()
        .expect("runtime xhr loadingFailed event");
    assert_eq!(failed["params"]["type"], "XHR");
    assert_eq!(failed["params"]["canceled"], false);
    assert!(
        failed["params"]["errorText"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );

    ctx.process_async(json!({
        "id": 207,
        "method": "Network.getResponseBody",
        "sessionId": "SID-1",
        "params": { "requestId": request_id }
    }))
    .await;
    ctx.expect_error(
        207,
        -32000,
        "No data found for resource with given identifier",
    );

    failing_server.abort();
    server.abort();
}
