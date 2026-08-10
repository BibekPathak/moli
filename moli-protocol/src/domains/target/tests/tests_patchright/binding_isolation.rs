use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_dom_cleanup_removes_marker_nodes_via_dom_domain_calls() {
    let mut ctx = TestContext::new();
    let attached = create_attached_page_session_async(&mut ctx, 292, 293, 294, 295, 296).await;

    ctx.process_async(json!({
            "id": 297,
            "method": "Page.navigate",
            "sessionId": attached.session_id,
            "params": {
                "url": "data:text/html,<!doctype html><html><body><script class='__pw_init'></script><script class='__pw_init'></script><div id='keep'>ok</div></body></html>"
            }
        })).await;
    let navigation = take_response_by_id(&mut ctx, 297);
    assert_eq!(navigation["sessionId"], json!(attached.session_id));
    let loader_id = navigation["result"]["loaderId"]
        .as_str()
        .expect("navigation loader id");
    crate::testing::wait_until_renderer_document_load(
        &mut ctx,
        Some(&attached.session_id),
        &attached.target_id,
        loader_id,
    )
    .await;
    ctx.take_all();

    ctx.process_async(json!({
        "id": 298,
        "method": "DOM.getDocument",
        "sessionId": attached.session_id,
        "params": { "depth": 1 }
    }))
    .await;
    let root_id = take_response_by_id(&mut ctx, 298)["result"]["root"]["nodeId"]
        .as_u64()
        .expect("document node id") as u32;

    ctx.process_async(json!({
        "id": 299,
        "method": "DOM.querySelectorAll",
        "sessionId": attached.session_id,
        "params": {
            "nodeId": root_id,
            "selector": "[class=__pw_init]"
        }
    }))
    .await;
    let marker_node_ids = take_response_by_id(&mut ctx, 299)["result"]["nodeIds"]
        .as_array()
        .expect("marker node ids")
        .iter()
        .filter_map(|value| value.as_u64())
        .map(|value| value as u32)
        .collect::<Vec<_>>();
    assert_eq!(marker_node_ids.len(), 2);

    for (offset, node_id) in marker_node_ids.iter().copied().enumerate() {
        let command_id = 300 + offset as u64;
        ctx.process_async(json!({
            "id": command_id,
            "method": "DOM.removeNode",
            "sessionId": attached.session_id,
            "params": { "nodeId": node_id }
        }))
        .await;
        ctx.expect_result(command_id, json!({}), Some(&attached.session_id));
    }

    ctx.process_async(json!({
        "id": 302,
        "method": "Runtime.runIfWaitingForDebugger",
        "sessionId": attached.session_id
    }))
    .await;
    ctx.expect_result(302, json!({}), Some(&attached.session_id));

    ctx.process_async(json!({
            "id": 303,
            "method": "Runtime.evaluate",
            "sessionId": attached.session_id,
            "params": {
                "expression": "JSON.stringify({ markers: document.querySelectorAll('[class=__pw_init]').length, keep: document.getElementById('keep')?.textContent ?? null })"
            }
        })).await;
    let cleanup_state = take_response_by_id(&mut ctx, 303);
    let cleanup_state = cleanup_state["result"]["result"]["value"]
        .as_str()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .expect("cleanup state payload");
    assert_eq!(cleanup_state["markers"], json!(0));
    assert_eq!(cleanup_state["keep"], json!("ok"));
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_page_binding_round_trip_resolves_utility_world_promise_without_runtime_enable()
 {
    let mut ctx = TestContext::new();
    ctx.process_async(json!({
        "id": 292,
        "method": "Target.createBrowserContext"
    }))
    .await;
    let browser_context_id = take_response_by_id(&mut ctx, 292)["result"]["browserContextId"]
        .as_str()
        .expect("browser context id")
        .to_owned();

    ctx.process_async(json!({
        "id": 293,
        "method": "Target.createTarget",
        "params": {
            "browserContextId": browser_context_id,
            "url": "about:blank"
        }
    }))
    .await;
    let target_id = take_response_by_id(&mut ctx, 293)["result"]["targetId"]
        .as_str()
        .expect("target id")
        .to_owned();
    ctx.expect_event(
        "Target.targetCreated",
        Some(&json!({
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 294,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": target_id,
            "flatten": true
        }
    }))
    .await;
    let session_id = ctx
        .conn
        .browser_context
        .as_ref()
        .and_then(|bc| bc.active_session_id_owned())
        .expect("session id should exist");
    ctx.expect_result(294, json!({ "sessionId": session_id }), None);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": session_id,
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 295,
        "method": "Page.navigate",
        "sessionId": session_id,
        "params": {
            "url": "data:text/html,<body>page-binding-round-trip</body>"
        }
    }))
    .await;
    let navigation = take_response_by_id(&mut ctx, 295);
    assert_eq!(navigation["sessionId"], json!(session_id));
    ctx.take_all();

    ctx.process_async(json!({
        "id": 296,
        "method": "Page.createIsolatedWorld",
        "sessionId": session_id,
        "params": {
            "frameId": target_id,
            "worldName": "utility"
        }
    }))
    .await;
    let utility_context_id = take_response_by_id(&mut ctx, 296)["result"]["executionContextId"]
        .as_i64()
        .expect("utility context id");
    assert!(
        !ctx.sent.iter().any(|message| {
            message["method"] == json!("Runtime.executionContextCreated")
                || message["method"] == json!("Runtime.executionContextsCleared")
        }),
        "utility world creation should stay off Runtime.enable surfaces: {:?}",
        ctx.sent
    );
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 297,
        "method": "Runtime.addBinding",
        "sessionId": session_id,
        "params": {
            "name": "patchedBinding",
            "executionContextId": utility_context_id
        }
    }))
    .await;
    let add_binding = take_response_by_id(&mut ctx, 297);
    assert_eq!(add_binding["result"], json!({}));

    ctx.process_async(json!({
            "id": 298,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": r#"
                    (() => {
                        function addPageBinding(bindingName) {
                            const binding = globalThis[bindingName];
                            globalThis[bindingName] = (...args) => {
                                const me = globalThis[bindingName];
                                let callbacks = me.callbacks;
                                if (!callbacks) {
                                    callbacks = new Map();
                                    me.callbacks = callbacks;
                                }
                                const seq = (me.lastSeq || 0) + 1;
                                me.lastSeq = seq;
                                const payload = { name: bindingName, seq, serializedArgs: args };
                                const promise = new Promise((resolve, reject) => callbacks.set(seq, { resolve, reject }));
                                binding(JSON.stringify(payload));
                                return promise;
                            };
                        }
                        function deliverBindingResult(arg) {
                            const callbacks = globalThis[arg.name].callbacks;
                            if ('error' in arg)
                                callbacks.get(arg.seq).reject(arg.error);
                            else
                                callbacks.get(arg.seq).resolve(arg.result);
                            callbacks.delete(arg.seq);
                        }
                        addPageBinding('patchedBinding');
                        globalThis.__lm_deliverBindingResult = deliverBindingResult;
                        return typeof globalThis.patchedBinding;
                    })()
                "#
            }
        })).await;
    let install_wrapper = take_response_by_id(&mut ctx, 298);
    assert_eq!(
        install_wrapper["result"]["result"]["value"],
        json!("function")
    );

    ctx.process_async(json!({
            "id": 299,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": "globalThis.__lm_bindingPromise = patchedBinding('payload-from-page'); 'scheduled'"
            }
        })).await;
    let scheduled = take_response_by_id(&mut ctx, 299);
    assert_eq!(scheduled["result"]["result"]["value"], json!("scheduled"));
    let binding_called = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["params"]["name"] == json!("patchedBinding")
        })
        .cloned()
        .expect("page binding wrapper should emit Runtime.bindingCalled");
    assert_eq!(
        binding_called["params"]["executionContextId"],
        json!(utility_context_id)
    );
    let binding_payload = binding_called["params"]["payload"]
        .as_str()
        .expect("binding payload should be a json string");
    let binding_payload: serde_json::Value =
        serde_json::from_str(binding_payload).expect("binding payload should be valid json");
    assert_eq!(binding_payload["name"], json!("patchedBinding"));
    assert_eq!(
        binding_payload["serializedArgs"],
        json!(["payload-from-page"])
    );
    let seq = binding_payload["seq"]
        .as_i64()
        .expect("binding payload seq should be an integer");
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 300,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": format!("globalThis.__lm_deliverBindingResult({{ name: 'patchedBinding', seq: {seq}, result: 'resolved-from-cdp' }}); 'delivered'")
            }
        })).await;
    let delivered = take_response_by_id(&mut ctx, 300);
    assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

    ctx.process_async(json!({
        "id": 301,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "contextId": utility_context_id,
            "expression": "globalThis.__lm_bindingPromise",
            "awaitPromise": true
        }
    }))
    .await;
    let resolved = take_response_by_id(&mut ctx, 301);
    assert_eq!(
        resolved["result"]["result"]["value"],
        json!("resolved-from-cdp")
    );
    assert!(
        !ctx.sent.iter().any(|message| {
            message["method"] == json!("Runtime.executionContextCreated")
                || message["method"] == json!("Runtime.executionContextsCleared")
        }),
        "page-binding round trip should stay off Runtime.enable surfaces: {:?}",
        ctx.sent
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_page_binding_round_trip_rejects_utility_world_promise_without_runtime_enable()
 {
    let mut ctx = TestContext::new();

    ctx.process_async(json!({
        "id": 302,
        "method": "Target.createBrowserContext"
    }))
    .await;
    let browser_context_id = take_response_by_id(&mut ctx, 302)["result"]["browserContextId"]
        .as_str()
        .expect("browser context id")
        .to_owned();

    ctx.process_async(json!({
        "id": 303,
        "method": "Target.createTarget",
        "params": {
            "browserContextId": browser_context_id,
            "url": "about:blank"
        }
    }))
    .await;
    let target_id = take_response_by_id(&mut ctx, 303)["result"]["targetId"]
        .as_str()
        .expect("target id")
        .to_owned();
    ctx.expect_event(
        "Target.targetCreated",
        Some(&json!({
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 304,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": target_id,
            "flatten": true
        }
    }))
    .await;
    let session_id = ctx
        .conn
        .browser_context
        .as_ref()
        .and_then(|bc| bc.active_session_id_owned())
        .expect("session id should exist");
    ctx.expect_result(304, json!({ "sessionId": session_id }), None);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": session_id,
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 305,
        "method": "Page.navigate",
        "sessionId": session_id,
        "params": {
            "url": "data:text/html,<body>page-binding-rejection</body>"
        }
    }))
    .await;
    let navigation = take_response_by_id(&mut ctx, 305);
    assert_eq!(navigation["sessionId"], json!(session_id));
    ctx.take_all();

    ctx.process_async(json!({
        "id": 306,
        "method": "Page.createIsolatedWorld",
        "sessionId": session_id,
        "params": {
            "frameId": target_id,
            "worldName": "utility"
        }
    }))
    .await;
    let utility_context_id = take_response_by_id(&mut ctx, 306)["result"]["executionContextId"]
        .as_i64()
        .expect("utility context id");
    assert!(
        !ctx.sent.iter().any(|message| {
            message["method"] == json!("Runtime.executionContextCreated")
                || message["method"] == json!("Runtime.executionContextsCleared")
        }),
        "utility world creation should stay off Runtime.enable surfaces: {:?}",
        ctx.sent
    );
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 307,
        "method": "Runtime.addBinding",
        "sessionId": session_id,
        "params": {
            "name": "patchedRejectingBinding",
            "executionContextId": utility_context_id
        }
    }))
    .await;
    let add_binding = take_response_by_id(&mut ctx, 307);
    assert_eq!(add_binding["result"], json!({}));

    ctx.process_async(json!({
            "id": 308,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": r#"
                    (() => {
                        function addPageBinding(bindingName) {
                            const binding = globalThis[bindingName];
                            globalThis[bindingName] = (...args) => {
                                const me = globalThis[bindingName];
                                let callbacks = me.callbacks;
                                if (!callbacks) {
                                    callbacks = new Map();
                                    me.callbacks = callbacks;
                                }
                                const seq = (me.lastSeq || 0) + 1;
                                me.lastSeq = seq;
                                const payload = { name: bindingName, seq, serializedArgs: args };
                                const promise = new Promise((resolve, reject) => callbacks.set(seq, { resolve, reject }));
                                binding(JSON.stringify(payload));
                                return promise;
                            };
                        }
                        function deliverBindingResult(arg) {
                            const callbacks = globalThis[arg.name].callbacks;
                            if ('error' in arg)
                                callbacks.get(arg.seq).reject(arg.error);
                            else
                                callbacks.get(arg.seq).resolve(arg.result);
                            callbacks.delete(arg.seq);
                        }
                        addPageBinding('patchedRejectingBinding');
                        globalThis.__lm_deliverRejectingBindingResult = deliverBindingResult;
                        return typeof globalThis.patchedRejectingBinding;
                    })()
                "#
            }
        })).await;
    let install_wrapper = take_response_by_id(&mut ctx, 308);
    assert_eq!(
        install_wrapper["result"]["result"]["value"],
        json!("function")
    );

    ctx.process_async(json!({
            "id": 309,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": "globalThis.__lm_bindingPromise = patchedRejectingBinding('payload-from-page').then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled'"
            }
        })).await;
    let scheduled = take_response_by_id(&mut ctx, 309);
    assert_eq!(scheduled["result"]["result"]["value"], json!("scheduled"));
    let binding_called = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["params"]["name"] == json!("patchedRejectingBinding")
        })
        .cloned()
        .expect("page binding wrapper should emit Runtime.bindingCalled");
    assert_eq!(
        binding_called["params"]["executionContextId"],
        json!(utility_context_id)
    );
    let binding_payload = binding_called["params"]["payload"]
        .as_str()
        .expect("binding payload should be a json string");
    let binding_payload: serde_json::Value =
        serde_json::from_str(binding_payload).expect("binding payload should be valid json");
    assert_eq!(binding_payload["name"], json!("patchedRejectingBinding"));
    assert_eq!(
        binding_payload["serializedArgs"],
        json!(["payload-from-page"])
    );
    let seq = binding_payload["seq"]
        .as_i64()
        .expect("binding payload seq should be an integer");
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 310,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": format!("globalThis.__lm_deliverRejectingBindingResult({{ name: 'patchedRejectingBinding', seq: {seq}, error: 'rejected-from-cdp' }}); 'delivered'")
            }
        })).await;
    let delivered = take_response_by_id(&mut ctx, 310);
    assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

    ctx.process_async(json!({
        "id": 311,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "contextId": utility_context_id,
            "expression": "globalThis.__lm_bindingPromise",
            "awaitPromise": true
        }
    }))
    .await;
    let rejected = take_response_by_id(&mut ctx, 311);
    assert_eq!(
        rejected["result"]["result"]["value"],
        json!("rejected:rejected-from-cdp")
    );
    assert!(
        !ctx.sent.iter().any(|message| {
            message["method"] == json!("Runtime.executionContextCreated")
                || message["method"] == json!("Runtime.executionContextsCleared")
        }),
        "page-binding rejection round trip should stay off Runtime.enable surfaces: {:?}",
        ctx.sent
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_same_named_page_binding_in_main_and_utility_worlds_keeps_same_seq_isolated_by_execution_context()
 {
    let mut ctx = TestContext::new();

    ctx.process_async(json!({
        "id": 331,
        "method": "Target.createBrowserContext"
    }))
    .await;
    let browser_context_id = take_response_by_id(&mut ctx, 331)["result"]["browserContextId"]
        .as_str()
        .expect("browser context id")
        .to_owned();

    ctx.process_async(json!({
        "id": 332,
        "method": "Target.createTarget",
        "params": {
            "browserContextId": browser_context_id,
            "url": "about:blank"
        }
    }))
    .await;
    let target_id = take_response_by_id(&mut ctx, 332)["result"]["targetId"]
        .as_str()
        .expect("target id")
        .to_owned();
    ctx.expect_event(
        "Target.targetCreated",
        Some(&json!({
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 333,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": target_id,
            "flatten": true
        }
    }))
    .await;
    let session_id = ctx
        .conn
        .browser_context
        .as_ref()
        .and_then(|bc| bc.active_session_id_owned())
        .expect("session id should exist");
    ctx.expect_result(333, json!({ "sessionId": session_id }), None);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": session_id,
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 334,
        "method": "Page.navigate",
        "sessionId": session_id,
        "params": {
            "url": "data:text/html,<body>dual-world-page-binding</body>"
        }
    }))
    .await;
    let navigation = take_response_by_id(&mut ctx, 334);
    assert_eq!(navigation["sessionId"], json!(session_id));
    ctx.take_all();

    ctx.process_async(json!({
        "id": 335,
        "method": "Runtime.addBinding",
        "sessionId": session_id,
        "params": {
            "name": "dualWorldBinding"
        }
    }))
    .await;
    let add_main_binding = take_response_by_id(&mut ctx, 335);
    assert_eq!(add_main_binding["result"], json!({}));

    ctx.process_async(json!({
        "id": 336,
        "method": "Page.createIsolatedWorld",
        "sessionId": session_id,
        "params": {
            "frameId": target_id,
            "worldName": "utility"
        }
    }))
    .await;
    let utility_context_id = take_response_by_id(&mut ctx, 336)["result"]["executionContextId"]
        .as_i64()
        .expect("utility context id");
    assert!(
        !ctx.sent.iter().any(|message| {
            message["method"] == json!("Runtime.executionContextCreated")
                || message["method"] == json!("Runtime.executionContextsCleared")
        }),
        "dual-world page-binding setup should stay off Runtime.enable surfaces: {:?}",
        ctx.sent
    );
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 337,
        "method": "Runtime.addBinding",
        "sessionId": session_id,
        "params": {
            "name": "dualWorldBinding",
            "executionContextId": utility_context_id
        }
    }))
    .await;
    let add_utility_binding = take_response_by_id(&mut ctx, 337);
    assert_eq!(add_utility_binding["result"], json!({}));

    let wrapper_source = patchright_page_binding_wrapper_source(
        "dualWorldBinding",
        "__lm_dual_world_deliver",
        None,
        false,
    );

    ctx.process_async(json!({
        "id": 338,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "expression": wrapper_source
        }
    }))
    .await;
    let install_main_wrapper = take_response_by_id(&mut ctx, 338);
    assert_eq!(
        install_main_wrapper["result"]["result"]["value"],
        json!("function")
    );

    ctx.process_async(json!({
        "id": 339,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "contextId": utility_context_id,
            "expression": wrapper_source
        }
    }))
    .await;
    let install_utility_wrapper = take_response_by_id(&mut ctx, 339);
    assert_eq!(
        install_utility_wrapper["result"]["result"]["value"],
        json!("function")
    );

    ctx.process_async(json!({
            "id": 340,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": "globalThis.__lm_main_binding_promise = dualWorldBinding('from-main'); 'scheduled-main'"
            }
        })).await;
    let scheduled_main = take_response_by_id(&mut ctx, 340);
    assert_eq!(
        scheduled_main["result"]["result"]["value"],
        json!("scheduled-main")
    );
    let main_binding_called = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["params"]["name"] == json!("dualWorldBinding")
        })
        .cloned()
        .expect("main world should emit Runtime.bindingCalled");
    let main_execution_context_id = main_binding_called["params"]["executionContextId"]
        .as_i64()
        .expect("main execution context id");
    assert_ne!(
        main_execution_context_id, utility_context_id,
        "main and utility worlds must use distinct execution contexts"
    );
    let main_payload = main_binding_called["params"]["payload"]
        .as_str()
        .expect("main binding payload should be a json string");
    let main_payload: serde_json::Value =
        serde_json::from_str(main_payload).expect("main payload should be valid json");
    assert_eq!(main_payload["name"], json!("dualWorldBinding"));
    assert_eq!(main_payload["serializedArgs"], json!(["from-main"]));
    let main_seq = main_payload["seq"]
        .as_i64()
        .expect("main payload seq should be an integer");
    assert_eq!(main_seq, 1);
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 341,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": "globalThis.__lm_utility_binding_promise = dualWorldBinding('from-utility'); 'scheduled-utility'"
            }
        })).await;
    let scheduled_utility = take_response_by_id(&mut ctx, 341);
    assert_eq!(
        scheduled_utility["result"]["result"]["value"],
        json!("scheduled-utility")
    );
    let utility_binding_called = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["params"]["name"] == json!("dualWorldBinding")
        })
        .cloned()
        .expect("utility world should emit Runtime.bindingCalled");
    assert_eq!(
        utility_binding_called["params"]["executionContextId"],
        json!(utility_context_id)
    );
    let utility_payload = utility_binding_called["params"]["payload"]
        .as_str()
        .expect("utility binding payload should be a json string");
    let utility_payload: serde_json::Value =
        serde_json::from_str(utility_payload).expect("utility payload should be valid json");
    assert_eq!(utility_payload["name"], json!("dualWorldBinding"));
    assert_eq!(utility_payload["serializedArgs"], json!(["from-utility"]));
    let utility_seq = utility_payload["seq"]
        .as_i64()
        .expect("utility payload seq should be an integer");
    assert_eq!(
        utility_seq, 1,
        "utility world should keep its own callback sequence even when the main world already used seq=1"
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 342,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": format!("globalThis.__lm_dual_world_deliver({{ name: 'dualWorldBinding', seq: {utility_seq}, result: 'utility-resolved' }}); 'delivered-utility'")
            }
        })).await;
    let delivered_utility = take_response_by_id(&mut ctx, 342);
    assert_eq!(
        delivered_utility["result"]["result"]["value"],
        json!("delivered-utility")
    );

    ctx.process_async(json!({
            "id": 343,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": format!("globalThis.__lm_dual_world_deliver({{ name: 'dualWorldBinding', seq: {main_seq}, result: 'main-resolved' }}); 'delivered-main'")
            }
        })).await;
    let delivered_main = take_response_by_id(&mut ctx, 343);
    assert_eq!(
        delivered_main["result"]["result"]["value"],
        json!("delivered-main")
    );

    ctx.process_async(json!({
        "id": 344,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "expression": "globalThis.__lm_main_binding_promise",
            "awaitPromise": true
        }
    }))
    .await;
    let resolved_main = take_response_by_id(&mut ctx, 344);
    assert_eq!(
        resolved_main["result"]["result"]["value"],
        json!("main-resolved")
    );

    ctx.process_async(json!({
        "id": 345,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "contextId": utility_context_id,
            "expression": "globalThis.__lm_utility_binding_promise",
            "awaitPromise": true
        }
    }))
    .await;
    let resolved_utility = take_response_by_id(&mut ctx, 345);
    assert_eq!(
        resolved_utility["result"]["result"]["value"],
        json!("utility-resolved")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_same_named_page_binding_in_main_and_utility_worlds_keeps_serialized_object_args_isolated_by_execution_context()
 {
    let mut ctx = TestContext::new();

    ctx.process_async(json!({
        "id": 3451,
        "method": "Target.createBrowserContext"
    }))
    .await;
    let browser_context_id = take_response_by_id(&mut ctx, 3451)["result"]["browserContextId"]
        .as_str()
        .expect("browser context id")
        .to_owned();

    ctx.process_async(json!({
        "id": 3452,
        "method": "Target.createTarget",
        "params": {
            "browserContextId": browser_context_id,
            "url": "about:blank"
        }
    }))
    .await;
    let target_id = take_response_by_id(&mut ctx, 3452)["result"]["targetId"]
        .as_str()
        .expect("target id")
        .to_owned();
    ctx.expect_event(
        "Target.targetCreated",
        Some(&json!({
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 3453,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": target_id,
            "flatten": true
        }
    }))
    .await;
    let session_id = ctx
        .conn
        .browser_context
        .as_ref()
        .and_then(|bc| bc.active_session_id_owned())
        .expect("session id should exist");
    ctx.expect_result(3453, json!({ "sessionId": session_id }), None);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": session_id,
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 3454,
        "method": "Page.navigate",
        "sessionId": session_id,
        "params": {
            "url": "data:text/html,<body><div id='page'>dual-world-object-args</div></body>"
        }
    }))
    .await;
    let navigation = take_response_by_id(&mut ctx, 3454);
    assert_eq!(navigation["sessionId"], json!(session_id));
    ctx.take_all();

    ctx.process_async(json!({
        "id": 3455,
        "method": "Runtime.addBinding",
        "sessionId": session_id,
        "params": {
            "name": "dualWorldObjectBinding"
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 3455)["result"], json!({}));

    ctx.process_async(json!({
        "id": 3456,
        "method": "Page.createIsolatedWorld",
        "sessionId": session_id,
        "params": {
            "frameId": target_id,
            "worldName": "utility"
        }
    }))
    .await;
    let utility_context_id = take_response_by_id(&mut ctx, 3456)["result"]["executionContextId"]
        .as_i64()
        .expect("utility context id");
    ctx.take_all();

    ctx.process_async(json!({
        "id": 3457,
        "method": "Runtime.addBinding",
        "sessionId": session_id,
        "params": {
            "name": "dualWorldObjectBinding",
            "executionContextId": utility_context_id
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 3457)["result"], json!({}));

    let wrapper_source = patchright_page_binding_wrapper_source(
        "dualWorldObjectBinding",
        "__lm_dual_world_object_deliver",
        None,
        false,
    );

    for (id, context_id) in [(3458_u64, None), (3459_u64, Some(utility_context_id))] {
        let mut params = json!({
            "expression": &wrapper_source,
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        let install_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    ctx.process_async(json!({
            "id": 3460,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": "globalThis.__lm_main_object_binding_promise = dualWorldObjectBinding({ source: 'main', nested: { count: 1, values: ['a', 2, true] } }); 'scheduled-main'"
            }
        })).await;
    let scheduled_main = take_response_by_id(&mut ctx, 3460);
    assert_eq!(
        scheduled_main["result"]["result"]["value"],
        json!("scheduled-main")
    );
    let main_binding_called = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["params"]["name"] == json!("dualWorldObjectBinding")
        })
        .cloned()
        .expect("main world should emit Runtime.bindingCalled");
    let main_execution_context_id = main_binding_called["params"]["executionContextId"]
        .as_i64()
        .expect("main execution context id");
    assert_ne!(main_execution_context_id, utility_context_id);
    let main_payload = main_binding_called["params"]["payload"]
        .as_str()
        .expect("main binding payload should be a json string");
    let main_payload: serde_json::Value =
        serde_json::from_str(main_payload).expect("main payload should be valid json");
    assert_eq!(main_payload["name"], json!("dualWorldObjectBinding"));
    assert_eq!(main_payload["seq"], json!(1));
    assert_eq!(
        main_payload["serializedArgs"],
        json!([{
            "source": "main",
            "nested": {
                "count": 1,
                "values": ["a", 2, true]
            }
        }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 3461,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": "globalThis.__lm_utility_object_binding_promise = dualWorldObjectBinding({ source: 'utility', nested: { count: 2, values: ['b', 3, false] } }); 'scheduled-utility'"
            }
        })).await;
    let scheduled_utility = take_response_by_id(&mut ctx, 3461);
    assert_eq!(
        scheduled_utility["result"]["result"]["value"],
        json!("scheduled-utility")
    );
    let utility_binding_called = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["params"]["name"] == json!("dualWorldObjectBinding")
                && message["params"]["executionContextId"] == json!(utility_context_id)
        })
        .cloned()
        .expect("utility world should emit Runtime.bindingCalled");
    let utility_payload = utility_binding_called["params"]["payload"]
        .as_str()
        .expect("utility binding payload should be a json string");
    let utility_payload: serde_json::Value =
        serde_json::from_str(utility_payload).expect("utility payload should be valid json");
    assert_eq!(utility_payload["name"], json!("dualWorldObjectBinding"));
    assert_eq!(utility_payload["seq"], json!(1));
    assert_eq!(
        utility_payload["serializedArgs"],
        json!([{
            "source": "utility",
            "nested": {
                "count": 2,
                "values": ["b", 3, false]
            }
        }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 3462,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": "globalThis.__lm_dual_world_object_deliver({ name: 'dualWorldObjectBinding', seq: 1, result: 'utility-object-resolved' }); 'delivered-utility'"
            }
        })).await;
    let delivered_utility = take_response_by_id(&mut ctx, 3462);
    assert_eq!(
        delivered_utility["result"]["result"]["value"],
        json!("delivered-utility")
    );

    ctx.process_async(json!({
            "id": 3463,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": "globalThis.__lm_dual_world_object_deliver({ name: 'dualWorldObjectBinding', seq: 1, result: 'main-object-resolved' }); 'delivered-main'"
            }
        })).await;
    let delivered_main = take_response_by_id(&mut ctx, 3463);
    assert_eq!(
        delivered_main["result"]["result"]["value"],
        json!("delivered-main")
    );

    for (id, context_id, promise_name, expected) in [
        (
            3464_u64,
            None,
            "__lm_main_object_binding_promise",
            "main-object-resolved",
        ),
        (
            3465_u64,
            Some(utility_context_id),
            "__lm_utility_object_binding_promise",
            "utility-object-resolved",
        ),
    ] {
        let mut params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id);
        assert_eq!(resolved["result"]["result"]["value"], json!(expected));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_same_named_handle_binding_in_main_and_utility_worlds_keeps_handles_isolated_by_execution_context()
 {
    super::patchright_8mb_stack("patchright-binding-isolation-dual-world-handles", || async {
    let mut ctx = TestContext::new();

    ctx.process_async(json!({
        "id": 346,
        "method": "Target.createBrowserContext"
    }))
    .await;
    let browser_context_id = take_response_by_id(&mut ctx, 346)["result"]["browserContextId"]
        .as_str()
        .expect("browser context id")
        .to_owned();

    ctx.process_async(json!({
        "id": 347,
        "method": "Target.createTarget",
        "params": {
            "browserContextId": browser_context_id,
            "url": "about:blank"
        }
    }))
    .await;
    let target_id = take_response_by_id(&mut ctx, 347)["result"]["targetId"]
        .as_str()
        .expect("target id")
        .to_owned();
    ctx.expect_event(
        "Target.targetCreated",
        Some(&json!({
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
        "id": 348,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": target_id,
            "flatten": true
        }
    }))
    .await;
    let session_id = ctx
        .conn
        .browser_context
        .as_ref()
        .and_then(|bc| bc.active_session_id_owned())
        .expect("session id should exist");
    ctx.expect_result(348, json!({ "sessionId": session_id }), None);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": session_id,
            "targetInfo": {
                "targetId": target_id,
                "browserContextId": browser_context_id,
            }
        })),
    );

    ctx.process_async(json!({
            "id": 349,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": "data:text/html,<body><div id='main-handle'>main</div><div id='utility-handle'>utility</div></body>"
            }
        })).await;
    let navigation = take_response_by_id(&mut ctx, 349);
    assert_eq!(navigation["sessionId"], json!(session_id));
    ctx.take_all();

    ctx.process_async(json!({
        "id": 350,
        "method": "Runtime.addBinding",
        "sessionId": session_id,
        "params": {
            "name": "dualWorldHandleBinding"
        }
    }))
    .await;
    let add_main_binding = take_response_by_id(&mut ctx, 350);
    assert_eq!(add_main_binding["result"], json!({}));

    ctx.process_async(json!({
        "id": 351,
        "method": "Page.createIsolatedWorld",
        "sessionId": session_id,
        "params": {
            "frameId": target_id,
            "worldName": "utility"
        }
    }))
    .await;
    let utility_context_id = take_response_by_id(&mut ctx, 351)["result"]["executionContextId"]
        .as_i64()
        .expect("utility context id");
    ctx.sent.clear();

    ctx.process_async(json!({
        "id": 352,
        "method": "Runtime.addBinding",
        "sessionId": session_id,
        "params": {
            "name": "dualWorldHandleBinding",
            "executionContextId": utility_context_id
        }
    }))
    .await;
    let add_utility_binding = take_response_by_id(&mut ctx, 352);
    assert_eq!(add_utility_binding["result"], json!({}));

    let handle_wrapper_source = r#"
            (() => {
                function addHandleBinding(bindingName) {
                    const binding = globalThis[bindingName];
                    globalThis[bindingName] = (...args) => {
                        const me = globalThis[bindingName];
                        let callbacks = me.callbacks;
                        if (!callbacks) {
                            callbacks = new Map();
                            me.callbacks = callbacks;
                        }
                        let handles = me.handles;
                        if (!handles) {
                            handles = new Map();
                            me.handles = handles;
                        }
                        const seq = (me.lastSeq || 0) + 1;
                        me.lastSeq = seq;
                        handles.set(seq, args[0]);
                        const promise = new Promise((resolve, reject) => callbacks.set(seq, { resolve, reject }));
                        binding(JSON.stringify({ name: bindingName, seq }));
                        return promise;
                    };
                }
                function takeBindingHandle(arg) {
                    const handles = globalThis[arg.name].handles;
                    const handle = handles.get(arg.seq);
                    handles.delete(arg.seq);
                    return handle;
                }
                function deliverBindingResult(arg) {
                    const callbacks = globalThis[arg.name].callbacks;
                    if ('error' in arg)
                        callbacks.get(arg.seq).reject(arg.error);
                    else
                        callbacks.get(arg.seq).resolve(arg.result);
                    callbacks.delete(arg.seq);
                }
                addHandleBinding('dualWorldHandleBinding');
                globalThis.__lm_dual_world_take_handle = takeBindingHandle;
                globalThis.__lm_dual_world_deliver_handle = deliverBindingResult;
                return typeof globalThis.dualWorldHandleBinding;
            })()
        "#;

    ctx.process_async(json!({
        "id": 353,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "expression": handle_wrapper_source
        }
    }))
    .await;
    let install_main_wrapper = take_response_by_id(&mut ctx, 353);
    assert_eq!(
        install_main_wrapper["result"]["result"]["value"],
        json!("function")
    );

    ctx.process_async(json!({
        "id": 354,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "contextId": utility_context_id,
            "expression": handle_wrapper_source
        }
    }))
    .await;
    let install_utility_wrapper = take_response_by_id(&mut ctx, 354);
    assert_eq!(
        install_utility_wrapper["result"]["result"]["value"],
        json!("function")
    );

    ctx.process_async(json!({
            "id": 355,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": "globalThis.__lm_main_handle_promise = dualWorldHandleBinding(document.getElementById('main-handle')); 'scheduled-main'"
            }
        })).await;
    let scheduled_main = take_response_by_id(&mut ctx, 355);
    assert_eq!(
        scheduled_main["result"]["result"]["value"],
        json!("scheduled-main")
    );
    let main_binding_called = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["params"]["name"] == json!("dualWorldHandleBinding")
        })
        .cloned()
        .expect("main world handle binding should emit Runtime.bindingCalled");
    let main_execution_context_id = main_binding_called["params"]["executionContextId"]
        .as_i64()
        .expect("main execution context id");
    assert_ne!(main_execution_context_id, utility_context_id);
    let main_payload = main_binding_called["params"]["payload"]
        .as_str()
        .expect("main handle payload should be a json string");
    let main_payload: serde_json::Value =
        serde_json::from_str(main_payload).expect("main handle payload should be valid json");
    assert_eq!(main_payload["name"], json!("dualWorldHandleBinding"));
    let main_seq = main_payload["seq"]
        .as_i64()
        .expect("main handle seq should be an integer");
    assert_eq!(main_seq, 1);
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 356,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": "globalThis.__lm_utility_handle_promise = dualWorldHandleBinding(document.getElementById('utility-handle')); 'scheduled-utility'"
            }
        })).await;
    let scheduled_utility = take_response_by_id(&mut ctx, 356);
    assert_eq!(
        scheduled_utility["result"]["result"]["value"],
        json!("scheduled-utility")
    );
    let utility_binding_called = ctx
        .sent
        .iter()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["params"]["name"] == json!("dualWorldHandleBinding")
        })
        .cloned()
        .expect("utility world handle binding should emit Runtime.bindingCalled");
    assert_eq!(
        utility_binding_called["params"]["executionContextId"],
        json!(utility_context_id)
    );
    let utility_payload = utility_binding_called["params"]["payload"]
        .as_str()
        .expect("utility handle payload should be a json string");
    let utility_payload: serde_json::Value =
        serde_json::from_str(utility_payload).expect("utility handle payload should be valid json");
    assert_eq!(utility_payload["name"], json!("dualWorldHandleBinding"));
    let utility_seq = utility_payload["seq"]
        .as_i64()
        .expect("utility handle seq should be an integer");
    assert_eq!(utility_seq, 1);
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 357,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": format!("(() => {{ const handle = globalThis.__lm_dual_world_take_handle({{ name: 'dualWorldHandleBinding', seq: {utility_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_dual_world_take_handle({{ name: 'dualWorldHandleBinding', seq: {utility_seq} }})]); }})()")
            }
        })).await;
    let utility_taken = take_response_by_id(&mut ctx, 357);
    let utility_taken_value = utility_taken["result"]["result"]["value"]
        .as_str()
        .expect("utility handle take should serialize");
    let utility_taken_value: serde_json::Value = serde_json::from_str(utility_taken_value)
        .expect("utility handle take should be valid json");
    assert_eq!(
        utility_taken_value,
        json!(["utility-handle", "utility", "undefined"])
    );

    ctx.process_async(json!({
            "id": 358,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": format!("(() => {{ const handle = globalThis.__lm_dual_world_take_handle({{ name: 'dualWorldHandleBinding', seq: {main_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_dual_world_take_handle({{ name: 'dualWorldHandleBinding', seq: {main_seq} }})]); }})()")
            }
        })).await;
    let main_taken = take_response_by_id(&mut ctx, 358);
    let main_taken_value = main_taken["result"]["result"]["value"]
        .as_str()
        .expect("main handle take should serialize");
    let main_taken_value: serde_json::Value =
        serde_json::from_str(main_taken_value).expect("main handle take should be valid json");
    assert_eq!(
        main_taken_value,
        json!(["main-handle", "main", "undefined"])
    );

    ctx.process_async(json!({
            "id": 359,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context_id,
                "expression": format!("globalThis.__lm_dual_world_deliver_handle({{ name: 'dualWorldHandleBinding', seq: {utility_seq}, result: 'utility-handle-resolved' }}); 'delivered-utility'")
            }
        })).await;
    let delivered_utility = take_response_by_id(&mut ctx, 359);
    assert_eq!(
        delivered_utility["result"]["result"]["value"],
        json!("delivered-utility")
    );

    ctx.process_async(json!({
            "id": 360,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": format!("globalThis.__lm_dual_world_deliver_handle({{ name: 'dualWorldHandleBinding', seq: {main_seq}, result: 'main-handle-resolved' }}); 'delivered-main'")
            }
        })).await;
    let delivered_main = take_response_by_id(&mut ctx, 360);
    assert_eq!(
        delivered_main["result"]["result"]["value"],
        json!("delivered-main")
    );

    ctx.process_async(json!({
        "id": 361,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "expression": "globalThis.__lm_main_handle_promise",
            "awaitPromise": true
        }
    }))
    .await;
    let resolved_main = take_response_by_id(&mut ctx, 361);
    assert_eq!(
        resolved_main["result"]["result"]["value"],
        json!("main-handle-resolved")
    );

    ctx.process_async(json!({
        "id": 362,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "contextId": utility_context_id,
            "expression": "globalThis.__lm_utility_handle_promise",
            "awaitPromise": true
        }
    }))
    .await;
    let resolved_utility = take_response_by_id(&mut ctx, 362);
    assert_eq!(
        resolved_utility["result"]["result"]["value"],
        json!("utility-handle-resolved")
    );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_same_named_dual_world_handle_binding_keeps_same_seq_isolated_by_execution_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 363, 364, 365).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 366, 367, 368).await;

    for (id, session_id, label, main_handle_id, utility_handle_id) in [
        (
            369_u64,
            first.session_id.as_str(),
            "first-page",
            "first-main-handle",
            "first-utility-handle",
        ),
        (
            372_u64,
            second.session_id.as_str(),
            "second-page",
            "second-main-handle",
            "second-utility-handle",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedDualWorldHandleBinding"
            }
        }))
        .await;
        let add_main_binding = take_response_by_id(&mut ctx, id);
        assert_eq!(add_main_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 1,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedDualWorldHandleBinding",
                "executionContextName": "utility"
            }
        }))
        .await;
        let add_utility_binding = take_response_by_id(&mut ctx, id + 1);
        assert_eq!(add_utility_binding["result"], json!({}));

        ctx.process_async(json!({
                "id": id + 2,
                "method": "Page.navigate",
                "sessionId": session_id,
                "params": {
                    "url": format!(
                        "data:text/html,<body><div id='page'>{label}</div><div id='{main_handle_id}'>main-{label}</div><div id='{utility_handle_id}'>utility-{label}</div></body>"
                    )
                }
            })).await;
        let navigation = take_response_by_id(&mut ctx, id + 2);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (375_u64, first.target_id.as_str(), first.session_id.as_str()),
        (
            376_u64,
            second.target_id.as_str(),
            second.session_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": {
                "targetId": target_id,
                "sessionId": session_id
            }
        }))
        .await;
        ctx.expect_result(id, json!({}), None);
        ctx.expect_event(
            "Target.detachedFromTarget",
            Some(&json!({
                "targetId": target_id,
                "sessionId": session_id,
            })),
        );
    }

    ctx.process_async(json!({
        "id": 377,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(377, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        attached_events.len(),
        2,
        "auto-attach sweep should attach both targets"
    );
    let first_auto_session = attached_events
        .iter()
        .find(|event| event["params"]["targetInfo"]["targetId"] == json!(first.target_id))
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("first auto-attached session id")
        .to_owned();
    let second_auto_session = attached_events
        .iter()
        .find(|event| event["params"]["targetInfo"]["targetId"] == json!(second.target_id))
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("second auto-attached session id")
        .to_owned();

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            378_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            379_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.createIsolatedWorld",
            "sessionId": session_id,
            "params": {
                "frameId": target_id,
                "worldName": "utility"
            }
        }))
        .await;
        *utility_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("utility context id after auto-attach");
        ctx.take_all();
    }

    let handle_wrapper_source = r#"
            (() => {
                function addHandleBinding(bindingName) {
                    const binding = globalThis[bindingName];
                    globalThis[bindingName] = (...args) => {
                        const me = globalThis[bindingName];
                        let callbacks = me.callbacks;
                        if (!callbacks) {
                            callbacks = new Map();
                            me.callbacks = callbacks;
                        }
                        let handles = me.handles;
                        if (!handles) {
                            handles = new Map();
                            me.handles = handles;
                        }
                        const seq = (me.lastSeq || 0) + 1;
                        me.lastSeq = seq;
                        handles.set(seq, args[0]);
                        const promise = new Promise((resolve, reject) => callbacks.set(seq, { resolve, reject }));
                        binding(JSON.stringify({ name: bindingName, seq }));
                        return promise;
                    };
                }
                function takeBindingHandle(arg) {
                    const handles = globalThis[arg.name].handles;
                    const handle = handles.get(arg.seq);
                    handles.delete(arg.seq);
                    return handle;
                }
                function deliverBindingResult(arg) {
                    const callbacks = globalThis[arg.name].callbacks;
                    if ('error' in arg)
                        callbacks.get(arg.seq).reject(arg.error);
                    else
                        callbacks.get(arg.seq).resolve(arg.result);
                    callbacks.delete(arg.seq);
                }
                addHandleBinding('sharedDualWorldHandleBinding');
                globalThis.__lm_shared_dual_world_take_handle = takeBindingHandle;
                globalThis.__lm_shared_dual_world_deliver_handle = deliverBindingResult;
                return typeof globalThis.sharedDualWorldHandleBinding;
            })()
        "#;

    for (id, session_id) in [
        (380_u64, first_auto_session.as_str()),
        (382_u64, second_auto_session.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": handle_wrapper_source
            }
        }))
        .await;
        let install_main_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_main_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    for (id, session_id, utility_context) in [
        (381_u64, first_auto_session.as_str(), first_utility_context),
        (
            383_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": handle_wrapper_source
            }
        }))
        .await;
        let install_utility_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_utility_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    let mut first_main_seq = 0_i64;
    let mut first_main_context = 0_i64;
    let mut first_utility_seq = 0_i64;
    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (
        id,
        session_id,
        context_id,
        expression,
        expected_name,
        expected_text,
        seq_out,
        main_context_out,
    ) in [
        (
            384_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_main_handle_promise = sharedDualWorldHandleBinding(document.getElementById('first-main-handle')); 'scheduled-first-main'",
            "first-main-handle",
            "main-first-page",
            &mut first_main_seq,
            Some(&mut first_main_context),
        ),
        (
            385_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "globalThis.__lm_first_utility_handle_promise = sharedDualWorldHandleBinding(document.getElementById('first-utility-handle')); 'scheduled-first-utility'",
            "first-utility-handle",
            "utility-first-page",
            &mut first_utility_seq,
            None,
        ),
        (
            386_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_main_handle_promise = sharedDualWorldHandleBinding(document.getElementById('second-main-handle')); 'scheduled-second-main'",
            "second-main-handle",
            "main-second-page",
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            387_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "globalThis.__lm_second_utility_handle_promise = sharedDualWorldHandleBinding(document.getElementById('second-utility-handle')); 'scheduled-second-utility'",
            "second-utility-handle",
            "utility-second-page",
            &mut second_utility_seq,
            None,
        ),
    ] {
        let mut params = json!({ "expression": expression });
        if let Some(context_id) = context_id {
            params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("sharedDualWorldHandleBinding")
            })
            .cloned()
            .expect("dual-world handle binding should emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(
                execution_context_id,
                context_id.expect("utility context id")
            );
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("sharedDualWorldHandleBinding"));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();

        let mut take_params = json!({
            "expression": format!(
                "(() => {{ const handle = globalThis.__lm_shared_dual_world_take_handle({{ name: 'sharedDualWorldHandleBinding', seq: {} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_shared_dual_world_take_handle({{ name: 'sharedDualWorldHandleBinding', seq: {} }})]); }})()",
                *seq_out, *seq_out
            )
        });
        if let Some(context_id) = context_id {
            take_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": take_params
        }))
        .await;
        let taken = take_response_by_id(&mut ctx, id + 10);
        let taken_value = taken["result"]["result"]["value"]
            .as_str()
            .expect("handle take should serialize");
        let taken_value: serde_json::Value =
            serde_json::from_str(taken_value).expect("taken handle json");
        assert_eq!(
            taken_value,
            json!([expected_name, expected_text, "undefined"])
        );
    }

    assert_ne!(first_main_context, first_utility_context);
    assert_ne!(second_main_context, second_utility_context);

    for (id, session_id, context_id, seq, result, promise_name) in [
        (
            398_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-resolved",
            "__lm_second_utility_handle_promise",
        ),
        (
            399_u64,
            first_auto_session.as_str(),
            None,
            first_main_seq,
            "first-main-resolved",
            "__lm_first_main_handle_promise",
        ),
        (
            400_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            first_utility_seq,
            "first-utility-resolved",
            "__lm_first_utility_handle_promise",
        ),
        (
            401_u64,
            second_auto_session.as_str(),
            None,
            second_main_seq,
            "second-main-resolved",
            "__lm_second_main_handle_promise",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_dual_world_deliver_handle({{ name: 'sharedDualWorldHandleBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            )
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_same_named_dual_world_page_binding_keeps_same_seq_isolated_by_execution_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 402, 403, 404).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 405, 406, 407).await;

    for (id, session_id, label) in [
        (408_u64, first.session_id.as_str(), "first-page"),
        (411_u64, second.session_id.as_str(), "second-page"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedDualWorldBinding"
            }
        }))
        .await;
        let add_main_binding = take_response_by_id(&mut ctx, id);
        assert_eq!(add_main_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 1,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedDualWorldBinding",
                "executionContextName": "utility"
            }
        }))
        .await;
        let add_utility_binding = take_response_by_id(&mut ctx, id + 1);
        assert_eq!(add_utility_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 2,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,<body><div id='page'>{label}</div></body>")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id + 2);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (414_u64, first.target_id.as_str(), first.session_id.as_str()),
        (
            415_u64,
            second.target_id.as_str(),
            second.session_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": {
                "targetId": target_id,
                "sessionId": session_id
            }
        }))
        .await;
        ctx.expect_result(id, json!({}), None);
        ctx.expect_event(
            "Target.detachedFromTarget",
            Some(&json!({
                "targetId": target_id,
                "sessionId": session_id,
            })),
        );
    }

    ctx.process_async(json!({
        "id": 416,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(416, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        attached_events.len(),
        2,
        "auto-attach sweep should attach both targets"
    );
    let first_auto_session = attached_events
        .iter()
        .find(|event| event["params"]["targetInfo"]["targetId"] == json!(first.target_id))
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("first auto-attached session id")
        .to_owned();
    let second_auto_session = attached_events
        .iter()
        .find(|event| event["params"]["targetInfo"]["targetId"] == json!(second.target_id))
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("second auto-attached session id")
        .to_owned();

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            417_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            418_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.createIsolatedWorld",
            "sessionId": session_id,
            "params": {
                "frameId": target_id,
                "worldName": "utility"
            }
        }))
        .await;
        *utility_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("utility context id after auto-attach");
        ctx.take_all();
    }

    let wrapper_source = r#"
            (() => {
                function addPageBinding(bindingName) {
                    const binding = globalThis[bindingName];
                    globalThis[bindingName] = (...args) => {
                        const me = globalThis[bindingName];
                        let callbacks = me.callbacks;
                        if (!callbacks) {
                            callbacks = new Map();
                            me.callbacks = callbacks;
                        }
                        const seq = (me.lastSeq || 0) + 1;
                        me.lastSeq = seq;
                        const payload = { name: bindingName, seq, serializedArgs: args };
                        const promise = new Promise((resolve, reject) => callbacks.set(seq, { resolve, reject }));
                        binding(JSON.stringify(payload));
                        return promise;
                    };
                }
                function deliverBindingResult(arg) {
                    const callbacks = globalThis[arg.name].callbacks;
                    if ('error' in arg)
                        callbacks.get(arg.seq).reject(arg.error);
                    else
                        callbacks.get(arg.seq).resolve(arg.result);
                    callbacks.delete(arg.seq);
                }
                addPageBinding('sharedDualWorldBinding');
                globalThis.__lm_shared_dual_world_deliver = deliverBindingResult;
                return typeof globalThis.sharedDualWorldBinding;
            })()
        "#;

    for (id, session_id) in [
        (419_u64, first_auto_session.as_str()),
        (421_u64, second_auto_session.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": wrapper_source
            }
        }))
        .await;
        let install_main_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_main_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    for (id, session_id, utility_context) in [
        (420_u64, first_auto_session.as_str(), first_utility_context),
        (
            422_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": wrapper_source
            }
        }))
        .await;
        let install_utility_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_utility_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    let mut first_main_seq = 0_i64;
    let mut first_main_context = 0_i64;
    let mut first_utility_seq = 0_i64;
    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (id, session_id, context_id, expression, serialized_arg, seq_out, main_context_out) in [
        (
            423_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_main_binding_promise = sharedDualWorldBinding('from-first-main'); 'scheduled-first-main'",
            "from-first-main",
            &mut first_main_seq,
            Some(&mut first_main_context),
        ),
        (
            424_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "globalThis.__lm_first_utility_binding_promise = sharedDualWorldBinding('from-first-utility'); 'scheduled-first-utility'",
            "from-first-utility",
            &mut first_utility_seq,
            None,
        ),
        (
            425_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_main_binding_promise = sharedDualWorldBinding('from-second-main'); 'scheduled-second-main'",
            "from-second-main",
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            426_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "globalThis.__lm_second_utility_binding_promise = sharedDualWorldBinding('from-second-utility'); 'scheduled-second-utility'",
            "from-second-utility",
            &mut second_utility_seq,
            None,
        ),
    ] {
        let mut params = json!({ "expression": expression });
        if let Some(context_id) = context_id {
            params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("sharedDualWorldBinding")
            })
            .cloned()
            .expect("dual-world page binding should emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(
                execution_context_id,
                context_id.expect("utility context id")
            );
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value = serde_json::from_str(payload).unwrap_or_else(|error| {
                panic!(
                    "binding payload should be valid json: {error}; payload={payload:?}; event={binding_called:?}"
                )
            });
        assert_eq!(payload["name"], json!("sharedDualWorldBinding"));
        assert_eq!(payload["serializedArgs"], json!([serialized_arg]));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }

    assert_ne!(first_main_context, first_utility_context);
    assert_ne!(second_main_context, second_utility_context);

    for (id, session_id, context_id, seq, result, promise_name) in [
        (
            427_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-resolved",
            "__lm_second_utility_binding_promise",
        ),
        (
            428_u64,
            first_auto_session.as_str(),
            None,
            first_main_seq,
            "first-main-resolved",
            "__lm_first_main_binding_promise",
        ),
        (
            429_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            first_utility_seq,
            "first-utility-resolved",
            "__lm_first_utility_binding_promise",
        ),
        (
            430_u64,
            second_auto_session.as_str(),
            None,
            second_main_seq,
            "second-main-resolved",
            "__lm_second_main_binding_promise",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_dual_world_deliver({{ name: 'sharedDualWorldBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            )
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_same_named_dual_world_page_binding_keeps_serialized_object_args_isolated_by_execution_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 3431, 3432, 3433).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 3434, 3435, 3436).await;

    for (id, session_id, label) in [
        (3437_u64, first.session_id.as_str(), "first-object-page"),
        (3440_u64, second.session_id.as_str(), "second-object-page"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedDualWorldObjectBinding"
            }
        }))
        .await;
        assert_eq!(take_response_by_id(&mut ctx, id)["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 1,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedDualWorldObjectBinding",
                "executionContextName": "utility"
            }
        }))
        .await;
        assert_eq!(take_response_by_id(&mut ctx, id + 1)["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 2,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,<body><div id='page'>{label}</div></body>")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id + 2);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            3443_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            3444_u64,
            second.target_id.as_str(),
            second.session_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": {
                "targetId": target_id,
                "sessionId": session_id
            }
        }))
        .await;
        ctx.expect_result(id, json!({}), None);
        ctx.expect_event(
            "Target.detachedFromTarget",
            Some(&json!({
                "targetId": target_id,
                "sessionId": session_id,
            })),
        );
    }

    ctx.process_async(json!({
        "id": 3445,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(3445, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        attached_events.len(),
        2,
        "auto-attach sweep should attach both targets"
    );
    let first_auto_session = attached_events
        .iter()
        .find(|event| event["params"]["targetInfo"]["targetId"] == json!(first.target_id))
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("first auto-attached session id")
        .to_owned();
    let second_auto_session = attached_events
        .iter()
        .find(|event| event["params"]["targetInfo"]["targetId"] == json!(second.target_id))
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("second auto-attached session id")
        .to_owned();

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            3446_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            3447_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.createIsolatedWorld",
            "sessionId": session_id,
            "params": {
                "frameId": target_id,
                "worldName": "utility"
            }
        }))
        .await;
        *utility_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("utility context id after auto-attach");
        ctx.take_all();
    }

    let wrapper_source = patchright_page_binding_wrapper_source(
        "sharedDualWorldObjectBinding",
        "__lm_shared_dual_world_object_deliver",
        None,
        false,
    );

    for (id, session_id) in [
        (3448_u64, first_auto_session.as_str()),
        (3450_u64, second_auto_session.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": &wrapper_source
            }
        }))
        .await;
        let install_main_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_main_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    for (id, session_id, utility_context) in [
        (3449_u64, first_auto_session.as_str(), first_utility_context),
        (
            3451_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": &wrapper_source
            }
        }))
        .await;
        let install_utility_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_utility_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    let mut first_main_seq = 0_i64;
    let mut first_utility_seq = 0_i64;
    let mut second_main_seq = 0_i64;
    let mut second_utility_seq = 0_i64;
    let mut first_main_context = 0_i64;
    let mut second_main_context = 0_i64;
    for (id, session_id, context_id, expression, serialized_arg, seq_out, main_context_out) in [
        (
            3452_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_main_object_promise = sharedDualWorldObjectBinding({ source: 'first-main', nested: { count: 1, values: ['a', 2, true] } }); 'scheduled-first-main'",
            json!([{
                "source": "first-main",
                "nested": { "count": 1, "values": ["a", 2, true] }
            }]),
            &mut first_main_seq,
            Some(&mut first_main_context),
        ),
        (
            3453_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "globalThis.__lm_first_utility_object_promise = sharedDualWorldObjectBinding({ source: 'first-utility', nested: { count: 2, values: ['b', 3, false] } }); 'scheduled-first-utility'",
            json!([{
                "source": "first-utility",
                "nested": { "count": 2, "values": ["b", 3, false] }
            }]),
            &mut first_utility_seq,
            None,
        ),
        (
            3454_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_main_object_promise = sharedDualWorldObjectBinding({ source: 'second-main', nested: { count: 3, values: ['c', 4, true] } }); 'scheduled-second-main'",
            json!([{
                "source": "second-main",
                "nested": { "count": 3, "values": ["c", 4, true] }
            }]),
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            3455_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "globalThis.__lm_second_utility_object_promise = sharedDualWorldObjectBinding({ source: 'second-utility', nested: { count: 4, values: ['d', 5, false] } }); 'scheduled-second-utility'",
            json!([{
                "source": "second-utility",
                "nested": { "count": 4, "values": ["d", 5, false] }
            }]),
            &mut second_utility_seq,
            None,
        ),
    ] {
        let mut params = json!({ "expression": expression });
        if let Some(context_id) = context_id {
            params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("sharedDualWorldObjectBinding")
            })
            .cloned()
            .expect("dual-world object binding should emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(
                execution_context_id,
                context_id.expect("utility context id")
            );
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value = serde_json::from_str(payload).unwrap_or_else(|error| {
                panic!(
                    "binding payload should be valid json: {error}; payload={payload:?}; event={binding_called:?}"
                )
            });
        assert_eq!(payload["name"], json!("sharedDualWorldObjectBinding"));
        assert_eq!(payload["serializedArgs"], serialized_arg);
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }

    assert_ne!(first_main_context, first_utility_context);
    assert_ne!(second_main_context, second_utility_context);

    for (id, session_id, context_id, seq, result, promise_name) in [
        (
            3456_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-object-resolved",
            "__lm_second_utility_object_promise",
        ),
        (
            3457_u64,
            first_auto_session.as_str(),
            None,
            first_main_seq,
            "first-main-object-resolved",
            "__lm_first_main_object_promise",
        ),
        (
            3458_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            first_utility_seq,
            "first-utility-object-resolved",
            "__lm_first_utility_object_promise",
        ),
        (
            3459_u64,
            second_auto_session.as_str(),
            None,
            second_main_seq,
            "second-main-object-resolved",
            "__lm_second_main_object_promise",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_dual_world_object_deliver({{ name: 'sharedDualWorldObjectBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            )
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_same_named_dual_world_page_binding_rejection_keeps_same_seq_isolated_by_execution_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 431, 432, 433).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 434, 435, 436).await;

    for (id, session_id, label) in [
        (437_u64, first.session_id.as_str(), "first-page"),
        (440_u64, second.session_id.as_str(), "second-page"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedRejectingDualWorldBinding"
            }
        }))
        .await;
        let add_main_binding = take_response_by_id(&mut ctx, id);
        assert_eq!(add_main_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 1,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedRejectingDualWorldBinding",
                "executionContextName": "utility"
            }
        }))
        .await;
        let add_utility_binding = take_response_by_id(&mut ctx, id + 1);
        assert_eq!(add_utility_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 2,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,<body><div id='page'>{label}</div></body>")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id + 2);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (443_u64, first.target_id.as_str(), first.session_id.as_str()),
        (
            444_u64,
            second.target_id.as_str(),
            second.session_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": {
                "targetId": target_id,
                "sessionId": session_id
            }
        }))
        .await;
        ctx.expect_result(id, json!({}), None);
        ctx.expect_event(
            "Target.detachedFromTarget",
            Some(&json!({
                "targetId": target_id,
                "sessionId": session_id,
            })),
        );
    }

    ctx.process_async(json!({
        "id": 445,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(445, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        attached_events.len(),
        2,
        "auto-attach sweep should attach both targets"
    );
    let first_auto_session = attached_events
        .iter()
        .find(|event| event["params"]["targetInfo"]["targetId"] == json!(first.target_id))
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("first auto-attached session id")
        .to_owned();
    let second_auto_session = attached_events
        .iter()
        .find(|event| event["params"]["targetInfo"]["targetId"] == json!(second.target_id))
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("second auto-attached session id")
        .to_owned();

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            446_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            447_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.createIsolatedWorld",
            "sessionId": session_id,
            "params": {
                "frameId": target_id,
                "worldName": "utility"
            }
        }))
        .await;
        *utility_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("utility context id after auto-attach");
        ctx.take_all();
    }

    let wrapper_source = r#"
            (() => {
                function addPageBinding(bindingName) {
                    const binding = globalThis[bindingName];
                    globalThis[bindingName] = (...args) => {
                        const me = globalThis[bindingName];
                        let callbacks = me.callbacks;
                        if (!callbacks) {
                            callbacks = new Map();
                            me.callbacks = callbacks;
                        }
                        const seq = (me.lastSeq || 0) + 1;
                        me.lastSeq = seq;
                        const payload = { name: bindingName, seq, serializedArgs: args };
                        const promise = new Promise((resolve, reject) => callbacks.set(seq, { resolve, reject }));
                        binding(JSON.stringify(payload));
                        return promise;
                    };
                }
                function deliverBindingResult(arg) {
                    const callbacks = globalThis[arg.name].callbacks;
                    if ('error' in arg)
                        callbacks.get(arg.seq).reject(arg.error);
                    else
                        callbacks.get(arg.seq).resolve(arg.result);
                    callbacks.delete(arg.seq);
                }
                addPageBinding('sharedRejectingDualWorldBinding');
                globalThis.__lm_shared_rejecting_dual_world_deliver = deliverBindingResult;
                return typeof globalThis.sharedRejectingDualWorldBinding;
            })()
        "#;

    for (id, session_id) in [
        (448_u64, first_auto_session.as_str()),
        (450_u64, second_auto_session.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": wrapper_source
            }
        }))
        .await;
        let install_main_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_main_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    for (id, session_id, utility_context) in [
        (449_u64, first_auto_session.as_str(), first_utility_context),
        (
            451_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": wrapper_source
            }
        }))
        .await;
        let install_utility_wrapper = take_response_by_id(&mut ctx, id);
        assert_eq!(
            install_utility_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    let mut first_main_seq = 0_i64;
    let mut first_main_context = 0_i64;
    let mut first_utility_seq = 0_i64;
    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (id, session_id, context_id, expression, serialized_arg, seq_out, main_context_out) in [
        (
            452_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_main_binding_promise = sharedRejectingDualWorldBinding('from-first-main').then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-first-main'",
            "from-first-main",
            &mut first_main_seq,
            Some(&mut first_main_context),
        ),
        (
            453_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "globalThis.__lm_first_utility_binding_promise = sharedRejectingDualWorldBinding('from-first-utility').then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-first-utility'",
            "from-first-utility",
            &mut first_utility_seq,
            None,
        ),
        (
            454_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_main_binding_promise = sharedRejectingDualWorldBinding('from-second-main').then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-second-main'",
            "from-second-main",
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            455_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "globalThis.__lm_second_utility_binding_promise = sharedRejectingDualWorldBinding('from-second-utility').then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-second-utility'",
            "from-second-utility",
            &mut second_utility_seq,
            None,
        ),
    ] {
        let mut params = json!({ "expression": expression });
        if let Some(context_id) = context_id {
            params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("sharedRejectingDualWorldBinding")
            })
            .cloned()
            .expect("rejecting dual-world page binding should emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(
                execution_context_id,
                context_id.expect("utility context id")
            );
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("sharedRejectingDualWorldBinding"));
        assert_eq!(payload["serializedArgs"], json!([serialized_arg]));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }

    assert_ne!(first_main_context, first_utility_context);
    assert_ne!(second_main_context, second_utility_context);

    for (id, session_id, context_id, seq, error, promise_name) in [
        (
            456_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-rejected",
            "__lm_second_utility_binding_promise",
        ),
        (
            457_u64,
            first_auto_session.as_str(),
            None,
            first_main_seq,
            "first-main-rejected",
            "__lm_first_main_binding_promise",
        ),
        (
            458_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            first_utility_seq,
            "first-utility-rejected",
            "__lm_first_utility_binding_promise",
        ),
        (
            459_u64,
            second_auto_session.as_str(),
            None,
            second_main_seq,
            "second-main-rejected",
            "__lm_second_main_binding_promise",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_rejecting_dual_world_deliver({{ name: 'sharedRejectingDualWorldBinding', seq: {seq}, error: '{error}' }}); 'delivered'"
            )
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let rejected = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(
            rejected["result"]["result"]["value"],
            json!(format!("rejected:{error}"))
        );
    }
}
