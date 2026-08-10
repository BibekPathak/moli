use super::super::tests_cdp_smoke_fixture::SmokeFixtureServer;
use super::super::*;
use super::support::{attached_smoke_session, navigate_and_take_response};
use crate::testing::wait_until_messages;
use serde_json::json;

// Chromium source:
// third_party/blink/web_tests/inspector-protocol/runtime/runtime-evaluate-without-enabling.js
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_chromium_import_runtime_default_context_object_cleans_on_reload() {
    let fixture = SmokeFixtureServer::start().await;
    let mut ctx = TestContext::new_with_target_discovery(false);
    let attached = attached_smoke_session(&mut ctx, 92_000).await;
    navigate_and_take_response(
        &mut ctx,
        &attached.session_id,
        92_005,
        fixture.url("/plain"),
    )
    .await;

    ctx.process_async(json!({
        "id": 92_006,
        "method": "Runtime.evaluate",
        "sessionId": attached.session_id,
        "params": { "expression": "window.dummyObject = { a: 1 }; window.dummyObject" }
    }))
    .await;
    let evaluated = take_response_by_id(&mut ctx, 92_006);
    let object_id = evaluated["result"]["result"]["objectId"]
        .as_str()
        .unwrap_or_else(|| panic!("Runtime.evaluate should return objectId: {evaluated}"))
        .to_owned();

    ctx.process_async(json!({
        "id": 92_007,
        "method": "Page.enable",
        "sessionId": attached.session_id
    }))
    .await;
    ctx.expect_result(92_007, json!({}), Some(&attached.session_id));
    ctx.process_async(json!({
        "id": 92_008,
        "method": "Page.reload",
        "sessionId": attached.session_id
    }))
    .await;
    ctx.expect_result(92_008, json!({}), Some(&attached.session_id));
    wait_until_messages(
        &mut ctx,
        Some(attached.session_id.as_str()),
        "load event after reload",
        |messages| {
            messages.iter().any(|message| {
                message["method"] == json!("Page.loadEventFired")
                    && message["sessionId"] == json!(attached.session_id)
            })
        },
    )
    .await;

    ctx.process_async(json!({
        "id": 92_009,
        "method": "Runtime.getProperties",
        "sessionId": attached.session_id,
        "params": { "objectId": object_id, "ownProperties": true }
    }))
    .await;
    let get_properties = take_response_by_id(&mut ctx, 92_009);
    assert_eq!(get_properties["error"]["code"], -32000, "{get_properties}");
    assert!(
        get_properties["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("context") || message.contains("object")),
        "{get_properties}"
    );
}

// Chromium source:
// third_party/blink/web_tests/inspector-protocol/runtime/runtime-enable-forces-contexts.js
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_chromium_import_runtime_enable_emits_default_context() {
    let mut ctx = TestContext::new_with_target_discovery(false);
    let attached = attached_smoke_session(&mut ctx, 105_000).await;

    ctx.process_async(json!({
        "id": 105_005,
        "method": "Runtime.enable",
        "sessionId": attached.session_id
    }))
    .await;
    ctx.expect_result(105_005, json!({}), Some(&attached.session_id));
    let context = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.executionContextCreated")
                && message["sessionId"] == json!(attached.session_id)
        })
        .cloned()
        .unwrap_or_else(|| panic!("missing executionContextCreated: {:?}", ctx.sent));
    assert_eq!(context["params"]["context"]["auxData"]["isDefault"], true);
    assert_eq!(
        context["params"]["context"]["auxData"]["frameId"],
        attached.target_id
    );
}

// Capability source: docs/WEB_CAPABILITIES.md JavaScript execution.
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_capability_runtime_await_promise_contract() {
    let mut ctx = TestContext::new_with_target_discovery(false);
    let attached = attached_smoke_session(&mut ctx, 106_000).await;

    ctx.process_async(json!({
        "id": 106_005,
        "method": "Runtime.evaluate",
        "sessionId": attached.session_id,
        "params": {
            "expression": "Promise.resolve({ ok: true, answer: 42 })",
            "awaitPromise": true,
            "returnByValue": true
        }
    }))
    .await;
    let response = take_response_by_id(&mut ctx, 106_005);
    assert_eq!(
        response["result"]["result"]["value"],
        json!({ "ok": true, "answer": 42 })
    );
}

// Capability source: docs/WEB_CAPABILITIES.md page structure/value inspection.
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_capability_runtime_get_properties_contract() {
    let mut ctx = TestContext::new_with_target_discovery(false);
    let attached = attached_smoke_session(&mut ctx, 107_000).await;

    ctx.process_async(json!({
        "id": 107_005,
        "method": "Runtime.evaluate",
        "sessionId": attached.session_id,
        "params": { "expression": "({ alpha: 1, beta: 'two' })" }
    }))
    .await;
    let evaluated = take_response_by_id(&mut ctx, 107_005);
    let object_id = evaluated["result"]["result"]["objectId"]
        .as_str()
        .unwrap_or_else(|| panic!("objectId: {evaluated}"))
        .to_owned();

    ctx.process_async(json!({
        "id": 107_006,
        "method": "Runtime.getProperties",
        "sessionId": attached.session_id,
        "params": { "objectId": object_id, "ownProperties": true }
    }))
    .await;
    let properties = take_response_by_id(&mut ctx, 107_006);
    let names = properties["result"]["result"]
        .as_array()
        .expect("properties")
        .iter()
        .filter_map(|property| property["name"].as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"alpha"), "{properties}");
    assert!(names.contains(&"beta"), "{properties}");
}

// Capability source: docs/WEB_CAPABILITIES.md JavaScript execution.
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_capability_runtime_call_function_on_contract() {
    let mut ctx = TestContext::new_with_target_discovery(false);
    let attached = attached_smoke_session(&mut ctx, 108_000).await;

    ctx.process_async(json!({
        "id": 108_005,
        "method": "Runtime.evaluate",
        "sessionId": attached.session_id,
        "params": { "expression": "({ count: 41 })" }
    }))
    .await;
    let evaluated = take_response_by_id(&mut ctx, 108_005);
    let object_id = evaluated["result"]["result"]["objectId"]
        .as_str()
        .unwrap_or_else(|| panic!("objectId: {evaluated}"))
        .to_owned();
    ctx.process_async(json!({
        "id": 108_006,
        "method": "Runtime.callFunctionOn",
        "sessionId": attached.session_id,
        "params": {
            "objectId": object_id,
            "functionDeclaration": "function(delta) { return this.count + delta; }",
            "arguments": [{ "value": 1 }],
            "returnByValue": true
        }
    }))
    .await;
    let called = take_response_by_id(&mut ctx, 108_006);
    assert_eq!(called["result"]["result"]["value"], 42);
}

// Capability source: docs/WEB_CAPABILITIES.md page/client JavaScript bridge.
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_capability_runtime_binding_called_contract() {
    let mut ctx = TestContext::new_with_target_discovery(false);
    let attached = attached_smoke_session(&mut ctx, 109_000).await;

    ctx.process_async(json!({
        "id": 109_005,
        "method": "Runtime.enable",
        "sessionId": attached.session_id
    }))
    .await;
    ctx.expect_result(109_005, json!({}), Some(&attached.session_id));
    ctx.process_async(json!({
        "id": 109_006,
        "method": "Runtime.addBinding",
        "sessionId": attached.session_id,
        "params": { "name": "chromiumImportBinding" }
    }))
    .await;
    ctx.expect_result(109_006, json!({}), Some(&attached.session_id));
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 109_007,
        "method": "Runtime.evaluate",
        "sessionId": attached.session_id,
        "params": { "expression": "globalThis.chromiumImportBinding('payload')" }
    }))
    .await;
    let _ = take_response_by_id(&mut ctx, 109_007);
    assert!(ctx.sent.iter().any(|message| {
        message["method"] == json!("Runtime.bindingCalled")
            && message["sessionId"] == json!(attached.session_id)
            && message["params"]["name"] == json!("chromiumImportBinding")
            && message["params"]["payload"] == json!("payload")
    }));
}

// Chromium source:
// third_party/blink/web_tests/inspector-protocol/runtime/runtime-evaluate-return-by-value.js
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_chromium_import_runtime_return_by_value_non_serializable_errors() {
    let mut ctx = TestContext::new_with_target_discovery(false);
    let attached = attached_smoke_session(&mut ctx, 97_000).await;

    for (offset, expression) in [
        "Symbol(239)",
        "(() => { const a = {}; a.b = a; return a; })()",
    ]
    .into_iter()
    .enumerate()
    {
        let id = 97_005 + offset as u64;
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": attached.session_id,
            "params": { "expression": expression, "returnByValue": true }
        }))
        .await;
        let evaluated = take_response_by_id(&mut ctx, id);
        assert!(
            evaluated["result"]["exceptionDetails"].is_object() || evaluated["error"].is_object(),
            "{expression}: {evaluated}"
        );
    }
}
