use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_cleanup_retains_pw_bindings_while_removing_custom_ones()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2723, 2724, 2725).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2726, 2727, 2728).await;

    for (id, session_id, html) in [
        (
            2729_u64,
            first.session_id.as_str(),
            "<body><div id='first-main'>first-main</div><div id='first-utility'>first-utility</div></body>",
        ),
        (
            2730_u64,
            second.session_id.as_str(),
            "<body><div id='second-main'>second-main</div><div id='second-utility'>second-utility</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,{html}")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            2731_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            2732_u64,
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
        "id": 2733,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(2733, json!({}), None);
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
            2734_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            2735_u64,
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

    let custom_wrapper_source = patchright_page_binding_wrapper_source(
        "sharedCrPageCleanupBinding",
        "__lm_shared_crpage_cleanup_deliver",
        None,
        false,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptBinding",
        "__lm_pw_kept_binding_deliver",
        None,
        false,
    );
    for (id, session_id, utility_context) in [
        (2736_u64, first_auto_session.as_str(), first_utility_context),
        (
            2744_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        for (offset, binding_name) in [
            (0_u64, "sharedCrPageCleanupBinding"),
            (4_u64, "__pw_keptBinding"),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"],
                json!({})
            );

            ctx.process_async(json!({
                "id": id + offset + 1,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name,
                    "executionContextId": utility_context
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset + 1)["result"],
                json!({})
            );
        }

        for (offset, source) in [
            (2_u64, custom_wrapper_source.as_str()),
            (6_u64, retained_wrapper_source.as_str()),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"]["result"]["value"],
                json!("function")
            );

            ctx.process_async(json!({
                "id": id + offset + 1,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset + 1)["result"]["result"]["value"],
                json!("function")
            );
        }
    }

    ctx.process_async(json!({
        "id": 2752,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageCleanupBinding"
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 2752)["result"], json!({}));

    for (id, session_id, context_id, custom_type, kept_type) in [
        (
            2753_u64,
            first_auto_session.as_str(),
            None,
            "undefined",
            "function",
        ),
        (
            2754_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "undefined",
            "function",
        ),
        (
            2755_u64,
            second_auto_session.as_str(),
            None,
            "function",
            "function",
        ),
        (
            2756_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "function",
            "function",
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.sharedCrPageCleanupBinding, typeof globalThis.__pw_keptBinding])"
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
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(
            state["result"]["result"]["value"],
            json!(format!("[\"{custom_type}\",\"{kept_type}\"]"))
        );
    }

    let mut first_main_seq = 0_i64;
    let mut first_utility_seq = 0_i64;
    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    for (id, session_id, context_id, expression, serialized_arg, seq_out, main_context_out) in [
        (
            2757_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_pw_kept_main = __pw_keptBinding('first-main'); 'scheduled-first-main'",
            "first-main",
            &mut first_main_seq,
            None,
        ),
        (
            2758_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "globalThis.__lm_first_pw_kept_utility = __pw_keptBinding('first-utility'); 'scheduled-first-utility'",
            "first-utility",
            &mut first_utility_seq,
            None,
        ),
        (
            2759_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_pw_kept_main = __pw_keptBinding('second-main'); 'scheduled-second-main'",
            "second-main",
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
    ] {
        let mut params = json!({
            "expression": expression,
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
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled kept binding wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("__pw_keptBinding")
            })
            .cloned()
            .expect("retained __pw_ binding should still emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else if let Some(context_id) = context_id {
            assert_eq!(execution_context_id, context_id);
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("__pw_keptBinding"));
        assert_eq!(payload["serializedArgs"], json!([serialized_arg]));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }
    assert_ne!(
        second_main_context, second_utility_context,
        "main and utility worlds must remain distinct within the same target session"
    );

    for (id, session_id, context_id, seq, result, promise_name) in [
        (
            2760_u64,
            first_auto_session.as_str(),
            None,
            first_main_seq,
            "first-main-kept",
            "__lm_first_pw_kept_main",
        ),
        (
            2761_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            first_utility_seq,
            "first-utility-kept",
            "__lm_first_pw_kept_utility",
        ),
        (
            2762_u64,
            second_auto_session.as_str(),
            None,
            second_main_seq,
            "second-main-kept",
            "__lm_second_pw_kept_main",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            ),
            "awaitPromise": true
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
async fn patchright_over_cdp_auto_attach_sweep_crpage_cleanup_retains_pw_handle_bindings_while_removing_custom_ones()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2951, 2952, 2953).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2954, 2955, 2956).await;

    for (id, session_id, html) in [
        (
            2957_u64,
            first.session_id.as_str(),
            "<body><div id='first-main-handle'>first-main-handle</div><div id='first-utility-handle'>first-utility-handle</div></body>",
        ),
        (
            2958_u64,
            second.session_id.as_str(),
            "<body><div id='second-main-handle'>second-main-handle</div><div id='second-utility-handle'>second-utility-handle</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,{html}")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            2959_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            2960_u64,
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
        "id": 2961,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(2961, json!({}), None);
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
            2962_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            2963_u64,
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

    let custom_wrapper_source = patchright_page_binding_wrapper_source(
        "sharedCrPageHandleCleanupBinding",
        "__lm_shared_crpage_handle_cleanup_deliver",
        Some("__lm_shared_crpage_handle_cleanup_take"),
        true,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptHandleBinding",
        "__lm_pw_kept_handle_binding_deliver",
        Some("__lm_pw_kept_handle_binding_take"),
        true,
    );
    for (id, session_id, utility_context) in [
        (2964_u64, first_auto_session.as_str(), first_utility_context),
        (
            2972_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        for (offset, binding_name) in [
            (0_u64, "sharedCrPageHandleCleanupBinding"),
            (4_u64, "__pw_keptHandleBinding"),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"],
                json!({})
            );

            ctx.process_async(json!({
                "id": id + offset + 1,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name,
                    "executionContextId": utility_context
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset + 1)["result"],
                json!({})
            );
        }

        for (offset, source, world_name) in [
            (2_u64, custom_wrapper_source.as_str(), None),
            (3_u64, custom_wrapper_source.as_str(), Some(utility_context)),
            (6_u64, retained_wrapper_source.as_str(), None),
            (
                7_u64,
                retained_wrapper_source.as_str(),
                Some(utility_context),
            ),
        ] {
            let mut params = json!({
                "expression": source,
                "awaitPromise": true
            });
            if let Some(world_name) = world_name {
                params["contextId"] = json!(world_name);
            }
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": params
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"]["result"]["value"],
                json!("function")
            );
        }
    }

    ctx.process_async(json!({
        "id": 2980,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageHandleCleanupBinding"
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 2980)["result"], json!({}));

    for (id, session_id, context_id, custom_type, kept_type) in [
        (
            2981_u64,
            first_auto_session.as_str(),
            None,
            "undefined",
            "function",
        ),
        (
            2982_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "undefined",
            "function",
        ),
        (
            2983_u64,
            second_auto_session.as_str(),
            None,
            "function",
            "function",
        ),
        (
            2984_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "function",
            "function",
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.sharedCrPageHandleCleanupBinding, typeof globalThis.__pw_keptHandleBinding])"
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
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(
            state["result"]["result"]["value"],
            json!(format!("[\"{custom_type}\",\"{kept_type}\"]"))
        );
    }

    let mut first_main_seq = 0_i64;
    let mut first_utility_seq = 0_i64;
    let mut second_main_seq = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (id, session_id, context_id, expression, binding_name, expected_text, seq_out) in [
        (
            2985_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_pw_kept_handle_main = __pw_keptHandleBinding(document.getElementById('first-main-handle')); 'scheduled-first-main'",
            "__pw_keptHandleBinding",
            "first-main-handle",
            &mut first_main_seq,
        ),
        (
            2986_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "globalThis.__lm_first_pw_kept_handle_utility = __pw_keptHandleBinding(document.getElementById('first-utility-handle')); 'scheduled-first-utility'",
            "__pw_keptHandleBinding",
            "first-utility-handle",
            &mut first_utility_seq,
        ),
        (
            2987_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_custom_handle_main = sharedCrPageHandleCleanupBinding(document.getElementById('second-main-handle')); 'scheduled-second-main'",
            "sharedCrPageHandleCleanupBinding",
            "second-main-handle",
            &mut second_main_seq,
        ),
        (
            2988_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "globalThis.__lm_second_pw_kept_handle_utility = __pw_keptHandleBinding(document.getElementById('second-utility-handle')); 'scheduled-second-utility'",
            "__pw_keptHandleBinding",
            "second-utility-handle",
            &mut second_utility_seq,
        ),
    ] {
        let mut params = json!({
            "expression": expression,
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
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled handle wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .rev()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["sessionId"] == json!(session_id)
                    && message["params"]["name"] == json!(binding_name)
                    && match context_id {
                        Some(context_id) => {
                            message["params"]["executionContextId"] == json!(context_id)
                        }
                        None => true,
                    }
            })
            .cloned()
            .expect("expected binding call after retained/custom handle invocation");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();

        let mut take_params = json!({
            "expression": format!(
                "(() => {{ const handle = globalThis.{}({{ name: '{}', seq: {} }}); return JSON.stringify([handle.textContent, typeof globalThis.{}({{ name: '{}', seq: {} }})]); }})()",
                if binding_name == "__pw_keptHandleBinding" {
                    "__lm_pw_kept_handle_binding_take"
                } else {
                    "__lm_shared_crpage_handle_cleanup_take"
                },
                binding_name,
                *seq_out,
                if binding_name == "__pw_keptHandleBinding" {
                    "__lm_pw_kept_handle_binding_take"
                } else {
                    "__lm_shared_crpage_handle_cleanup_take"
                },
                binding_name,
                *seq_out
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
        assert_eq!(
            taken["result"]["result"]["value"],
            json!(format!("[\"{expected_text}\",\"undefined\"]"))
        );
    }

    for (id, session_id, context_id, binding_name, seq, result, promise_name) in [
        (
            2989_u64,
            first_auto_session.as_str(),
            None,
            "__pw_keptHandleBinding",
            first_main_seq,
            "first-main-kept-handle",
            "__lm_first_pw_kept_handle_main",
        ),
        (
            2990_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "__pw_keptHandleBinding",
            first_utility_seq,
            "first-utility-kept-handle",
            "__lm_first_pw_kept_handle_utility",
        ),
        (
            2991_u64,
            second_auto_session.as_str(),
            None,
            "sharedCrPageHandleCleanupBinding",
            second_main_seq,
            "second-main-custom-handle",
            "__lm_second_custom_handle_main",
        ),
        (
            2992_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "__pw_keptHandleBinding",
            second_utility_seq,
            "second-utility-kept-handle",
            "__lm_second_pw_kept_handle_utility",
        ),
    ] {
        let helper_name = if binding_name == "__pw_keptHandleBinding" {
            "__lm_pw_kept_handle_binding_deliver"
        } else {
            "__lm_shared_crpage_handle_cleanup_deliver"
        };
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.{helper_name}({{ name: '{binding_name}', seq: {seq}, result: '{result}' }}); 'delivered'"
            ),
            "awaitPromise": true
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
async fn patchright_over_cdp_auto_attach_sweep_crpage_cleanup_retains_pw_bindings_with_serialized_object_args_while_removing_custom_ones()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 26697, 26698, 26699)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 26700, 26701, 26702)
            .await;

    for (id, session_id, html) in [
        (
            26703_u64,
            first.session_id.as_str(),
            "<body><div id='first-main'>first-main</div><div id='first-utility'>first-utility</div></body>",
        ),
        (
            26704_u64,
            second.session_id.as_str(),
            "<body><div id='second-main'>second-main</div><div id='second-utility'>second-utility</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,{html}")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            26705_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            26706_u64,
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
        "id": 26707,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(26707, json!({}), None);
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
            26708_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            26709_u64,
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

    let custom_wrapper_source = patchright_page_binding_wrapper_source(
        "sharedCrPageCleanupBinding",
        "__lm_shared_crpage_cleanup_deliver",
        None,
        false,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptBinding",
        "__lm_pw_kept_binding_deliver",
        None,
        false,
    );
    for (id, session_id, utility_context) in [
        (
            26710_u64,
            first_auto_session.as_str(),
            first_utility_context,
        ),
        (
            26718_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context,
            id,
            id + 1,
            id + 2,
            id + 3,
            "sharedCrPageCleanupBinding",
            &custom_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context,
            id + 4,
            id + 5,
            id + 6,
            id + 7,
            "__pw_keptBinding",
            &retained_wrapper_source,
        )
        .await;
    }

    ctx.process_async(json!({
        "id": 26726,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageCleanupBinding"
        }
    }))
    .await;
    let remove_binding = take_response_by_id(&mut ctx, 26726);
    assert_eq!(remove_binding["result"], json!({}));

    for (id, session_id, context_id, expected_state) in [
        (
            26727_u64,
            first_auto_session.as_str(),
            None,
            json!(["undefined", "function"]),
        ),
        (
            26728_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            json!(["undefined", "function"]),
        ),
        (
            26729_u64,
            second_auto_session.as_str(),
            None,
            json!(["function", "function"]),
        ),
        (
            26730_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            json!(["function", "function"]),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.sharedCrPageCleanupBinding, typeof globalThis.__pw_keptBinding])"
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
        let state = take_response_by_id(&mut ctx, id);
        let state_json = state["result"]["result"]["value"]
            .as_str()
            .expect("state should serialize as string");
        let state_json: serde_json::Value =
            serde_json::from_str(state_json).expect("state should be valid json");
        assert_eq!(state_json, expected_state);
    }

    let mut first_main_seq = 0_i64;
    let mut first_utility_seq = 0_i64;
    let mut second_main_seq = 0_i64;
    let mut second_utility_seq = 0_i64;
    let mut first_main_context = 0_i64;
    let mut second_main_context = 0_i64;
    for (id, session_id, context_id, expression, serialized_arg, seq_out, main_context_out) in [
        (
            26731_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_pw_kept_object_main = __pw_keptBinding({ source: 'first-main', nested: { count: 1, values: ['a', 2, true] } }); 'scheduled-first-main'",
            json!([{
                "source": "first-main",
                "nested": { "count": 1, "values": ["a", 2, true] }
            }]),
            &mut first_main_seq,
            Some(&mut first_main_context),
        ),
        (
            26732_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "globalThis.__lm_first_pw_kept_object_utility = __pw_keptBinding({ source: 'first-utility', nested: { count: 2, values: ['b', 3, false] } }); 'scheduled-first-utility'",
            json!([{
                "source": "first-utility",
                "nested": { "count": 2, "values": ["b", 3, false] }
            }]),
            &mut first_utility_seq,
            None,
        ),
        (
            26733_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_pw_kept_object_main = __pw_keptBinding({ source: 'second-main', nested: { count: 3, values: ['c', 4, true] } }); 'scheduled-second-main'",
            json!([{
                "source": "second-main",
                "nested": { "count": 3, "values": ["c", 4, true] }
            }]),
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            26734_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "globalThis.__lm_second_pw_kept_object_utility = __pw_keptBinding({ source: 'second-utility', nested: { count: 4, values: ['d', 5, false] } }); 'scheduled-second-utility'",
            json!([{
                "source": "second-utility",
                "nested": { "count": 4, "values": ["d", 5, false] }
            }]),
            &mut second_utility_seq,
            None,
        ),
    ] {
        let mut params = json!({
            "expression": expression,
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
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled binding wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("__pw_keptBinding")
            })
            .cloned()
            .expect("retained binding should emit Runtime.bindingCalled");
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
        assert_eq!(payload["name"], json!("__pw_keptBinding"));
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
            26735_u64,
            first_auto_session.as_str(),
            None,
            first_main_seq,
            "first-main-object-kept",
            "__lm_first_pw_kept_object_main",
        ),
        (
            26736_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            first_utility_seq,
            "first-utility-object-kept",
            "__lm_first_pw_kept_object_utility",
        ),
        (
            26737_u64,
            second_auto_session.as_str(),
            None,
            second_main_seq,
            "second-main-object-kept",
            "__lm_second_pw_kept_object_main",
        ),
        (
            26738_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-object-kept",
            "__lm_second_pw_kept_object_utility",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            ),
            "awaitPromise": true
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
async fn patchright_over_cdp_crpage_cleanup_replay_keeps_pw_bindings_but_not_removed_custom_bindings_after_navigation()
 {
    super::super::patchright_8mb_stack(
        "patchright-retained-pw-cleanup-navigation",
        || async {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2763, 2764, 2765).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2766, 2767, 2768).await;

    for (id, session_id, html) in [
        (
            2769_u64,
            first.session_id.as_str(),
            "<body><div id='first-main'>first-main</div><div id='first-utility'>first-utility</div></body>",
        ),
        (
            2770_u64,
            second.session_id.as_str(),
            "<body><div id='second-main'>second-main</div><div id='second-utility'>second-utility</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,{html}")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            2771_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            2772_u64,
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
        "id": 2773,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(2773, json!({}), None);
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
            2774_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            2775_u64,
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

    let custom_wrapper_source = patchright_page_binding_wrapper_source(
        "sharedCrPageCleanupBinding",
        "__lm_shared_crpage_cleanup_deliver",
        None,
        false,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptBinding",
        "__lm_pw_kept_binding_deliver",
        None,
        false,
    );
    for (id, session_id, utility_context) in [
        (2776_u64, first_auto_session.as_str(), first_utility_context),
        (
            2784_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        for (offset, binding_name) in [
            (0_u64, "sharedCrPageCleanupBinding"),
            (4_u64, "__pw_keptBinding"),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"],
                json!({})
            );

            ctx.process_async(json!({
                "id": id + offset + 1,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name,
                    "executionContextId": utility_context
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset + 1)["result"],
                json!({})
            );
        }

        for (offset, source, world_name) in [
            (2_u64, custom_wrapper_source.as_str(), None),
            (3_u64, custom_wrapper_source.as_str(), Some("utility")),
            (6_u64, retained_wrapper_source.as_str(), None),
            (7_u64, retained_wrapper_source.as_str(), Some("utility")),
        ] {
            let mut params = json!({
                "source": source,
                "runImmediately": true
            });
            if let Some(world_name) = world_name {
                params["worldName"] = json!(world_name);
            }
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Page.addScriptToEvaluateOnNewDocument",
                "sessionId": session_id,
                "params": params
            }))
            .await;
            assert!(
                take_response_by_id(&mut ctx, id + offset)["result"]["identifier"]
                    .as_str()
                    .is_some()
            );
        }
    }

    ctx.process_async(json!({
        "id": 2792,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageCleanupBinding"
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 2792)["result"], json!({}));

    for (id, session_id, label) in [
        (2793_u64, first_auto_session.as_str(), "first-replay"),
        (2794_u64, second_auto_session.as_str(), "second-replay"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,<body><div id='page'>{label}</div></body>")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, session_id, custom_binding_type, retained_binding_type) in [
        (
            2795_u64,
            first_auto_session.as_str(),
            "undefined",
            "function",
        ),
        (
            2797_u64,
            second_auto_session.as_str(),
            "function",
            "function",
        ),
    ] {
        for (offset, source, expected_type) in [
            (0_u64, custom_wrapper_source.as_str(), custom_binding_type),
            (
                1_u64,
                retained_wrapper_source.as_str(),
                retained_binding_type,
            ),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"]["result"]["value"],
                json!(expected_type)
            );
        }
    }

    for (id, session_id, custom_type, kept_type) in [
        (
            2799_u64,
            first_auto_session.as_str(),
            "undefined",
            "function",
        ),
        (
            2800_u64,
            second_auto_session.as_str(),
            "function",
            "function",
        ),
    ] {
        let params = json!({
            "expression": "JSON.stringify([typeof globalThis.sharedCrPageCleanupBinding, typeof globalThis.__pw_keptBinding])"
        });
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(
            state["result"]["result"]["value"],
            json!(format!("[\"{custom_type}\",\"{kept_type}\"]"))
        );
    }

    let mut first_replay_seq = 0_i64;
    let mut second_replay_seq = 0_i64;
    for (id, session_id, expression, serialized_arg, seq_out) in [
        (
            2801_u64,
            first_auto_session.as_str(),
            "globalThis.__lm_first_replay_pw_kept = __pw_keptBinding('first-replay-main'); 'scheduled-first-replay'",
            "first-replay-main",
            &mut first_replay_seq,
        ),
        (
            2802_u64,
            second_auto_session.as_str(),
            "globalThis.__lm_second_replay_pw_kept = __pw_keptBinding('second-replay-main'); 'scheduled-second-replay'",
            "second-replay-main",
            &mut second_replay_seq,
        ),
    ] {
        let params = json!({
            "expression": expression,
            "awaitPromise": true
        });
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
                .expect("scheduled replay kept binding wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("__pw_keptBinding")
            })
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "retained __pw_ binding should replay after navigation; events were: {:?}",
                    ctx.sent
                )
            });
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("__pw_keptBinding"));
        assert_eq!(payload["serializedArgs"], json!([serialized_arg]));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }

    for (id, session_id, seq, result, promise_name) in [
        (
            2803_u64,
            first_auto_session.as_str(),
            first_replay_seq,
            "first-replay-kept",
            "__lm_first_replay_pw_kept",
        ),
        (
            2804_u64,
            second_auto_session.as_str(),
            second_replay_seq,
            "second-replay-kept",
            "__lm_second_replay_pw_kept",
        ),
    ] {
        let deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
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

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            2815_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            2816_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_replay_utility_context,
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
            .expect("utility context after cleanup replay navigation");
        ctx.take_all();
    }

    for (
        id,
        session_id,
        utility_context,
        rehydrated_names,
        custom_binding_type,
        retained_binding_type,
    ) in [
        (
            2817_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            vec!["__pw_keptBinding"],
            "undefined",
            "function",
        ),
        (
            2821_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            vec!["sharedCrPageCleanupBinding", "__pw_keptBinding"],
            "function",
            "function",
        ),
    ] {
        for (offset, binding_name) in rehydrated_names.iter().enumerate() {
            ctx.process_async(json!({
                "id": id + offset as u64,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name,
                    "executionContextId": utility_context
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset as u64)["result"],
                json!({})
            );
        }

        let source_base = id + rehydrated_names.len() as u64;
        for (offset, source, expected_type) in [
            (0_u64, custom_wrapper_source.as_str(), custom_binding_type),
            (
                1_u64,
                retained_wrapper_source.as_str(),
                retained_binding_type,
            ),
        ] {
            ctx.process_async(json!({
                "id": source_base + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            let replayed = take_response_by_id(&mut ctx, source_base + offset);
            assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
        }
    }

    let mut first_replay_utility_seq = 0_i64;
    let mut second_replay_utility_seq = 0_i64;
    for (id, session_id, utility_context, expression, serialized_arg, seq_out) in [
        (
            2825_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            "globalThis.__lm_first_replay_pw_kept_utility = __pw_keptBinding('first-replay-utility'); 'scheduled-first-utility'",
            "first-replay-utility",
            &mut first_replay_utility_seq,
        ),
        (
            2826_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            "globalThis.__lm_second_replay_pw_kept_utility = __pw_keptBinding('second-replay-utility'); 'scheduled-second-utility'",
            "second-replay-utility",
            &mut second_replay_utility_seq,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": expression,
                "awaitPromise": true
            }
        }))
        .await;
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled utility replay kept binding wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
                .sent
                .iter()
                .find(|message| {
                    message["method"] == json!("Runtime.bindingCalled")
                        && message["params"]["name"] == json!("__pw_keptBinding")
                })
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "retained __pw_ binding should replay into utility world after navigation; events were: {:?}",
                        ctx.sent
                    )
                });
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("__pw_keptBinding"));
        assert_eq!(payload["serializedArgs"], json!([serialized_arg]));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }

    for (id, session_id, utility_context, seq, result, promise_name) in [
        (
            2827_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            first_replay_utility_seq,
            "first-replay-utility-kept",
            "__lm_first_replay_pw_kept_utility",
        ),
        (
            2828_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            second_replay_utility_seq,
            "second-replay-utility-kept",
            "__lm_second_replay_pw_kept_utility",
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": format!(
                        "globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
                    ),
                    "awaitPromise": true
                }
            })).await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": format!("globalThis.{promise_name}"),
                "awaitPromise": true
            }
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
        },
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_crpage_cleanup_replay_keeps_pw_bindings_with_serialized_object_args_after_navigation()
 {
    super::super::patchright_8mb_stack(
        "patchright-retained-pw-serialized-object-args",
        || async {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34001, 34002, 34003)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34004, 34005, 34006)
            .await;

    for (id, session_id, html) in [
        (
            34007_u64,
            first.session_id.as_str(),
            "<body><div id='first-main'>first-main</div><div id='first-utility'>first-utility</div></body>",
        ),
        (
            34008_u64,
            second.session_id.as_str(),
            "<body><div id='second-main'>second-main</div><div id='second-utility'>second-utility</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,{html}")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            34009_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            34010_u64,
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
        "id": 34011,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(34011, json!({}), None);
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
            34012_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            34013_u64,
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

    let custom_wrapper_source = patchright_page_binding_wrapper_source(
        "sharedCrPageCleanupBinding",
        "__lm_shared_crpage_cleanup_deliver",
        None,
        false,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptBinding",
        "__lm_pw_kept_binding_deliver",
        None,
        false,
    );
    for (id, session_id, utility_context) in [
        (
            34014_u64,
            first_auto_session.as_str(),
            first_utility_context,
        ),
        (
            34022_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        for (offset, binding_name) in [
            (0_u64, "sharedCrPageCleanupBinding"),
            (4_u64, "__pw_keptBinding"),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"],
                json!({})
            );

            ctx.process_async(json!({
                "id": id + offset + 1,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name,
                    "executionContextId": utility_context
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset + 1)["result"],
                json!({})
            );
        }

        for (offset, source, world_name) in [
            (2_u64, custom_wrapper_source.as_str(), None),
            (3_u64, custom_wrapper_source.as_str(), Some("utility")),
            (6_u64, retained_wrapper_source.as_str(), None),
            (7_u64, retained_wrapper_source.as_str(), Some("utility")),
        ] {
            let mut params = json!({
                "source": source,
                "runImmediately": true
            });
            if let Some(world_name) = world_name {
                params["worldName"] = json!(world_name);
            }
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Page.addScriptToEvaluateOnNewDocument",
                "sessionId": session_id,
                "params": params
            }))
            .await;
            assert!(
                take_response_by_id(&mut ctx, id + offset)["result"]["identifier"]
                    .as_str()
                    .is_some()
            );
        }
    }

    ctx.process_async(json!({
        "id": 34030,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageCleanupBinding"
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 34030)["result"], json!({}));

    for (id, session_id, label) in [
        (34031_u64, first_auto_session.as_str(), "first-replay"),
        (34032_u64, second_auto_session.as_str(), "second-replay"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,<body><div id='page'>{label}</div></body>")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, session_id, custom_binding_type, retained_binding_type) in [
        (
            34033_u64,
            first_auto_session.as_str(),
            "undefined",
            "function",
        ),
        (
            34035_u64,
            second_auto_session.as_str(),
            "function",
            "function",
        ),
    ] {
        for (offset, source, expected_type) in [
            (0_u64, custom_wrapper_source.as_str(), custom_binding_type),
            (
                1_u64,
                retained_wrapper_source.as_str(),
                retained_binding_type,
            ),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"]["result"]["value"],
                json!(expected_type)
            );
        }
    }

    for (id, session_id, custom_type, kept_type) in [
        (
            34037_u64,
            first_auto_session.as_str(),
            "undefined",
            "function",
        ),
        (
            34038_u64,
            second_auto_session.as_str(),
            "function",
            "function",
        ),
    ] {
        let params = json!({
            "expression": "JSON.stringify([typeof globalThis.sharedCrPageCleanupBinding, typeof globalThis.__pw_keptBinding])"
        });
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(
            state["result"]["result"]["value"],
            json!(format!("[\"{custom_type}\",\"{kept_type}\"]"))
        );
    }

    let mut first_replay_seq = 0_i64;
    let mut second_replay_seq = 0_i64;
    for (id, session_id, expression, serialized_arg, seq_out) in [
        (
            34039_u64,
            first_auto_session.as_str(),
            "globalThis.__lm_first_replay_pw_kept = __pw_keptBinding({ source: 'first-replay-main', nested: { count: 1, values: ['a', 2, true] } }); 'scheduled-first-replay'",
            json!([{
                "source": "first-replay-main",
                "nested": { "count": 1, "values": ["a", 2, true] }
            }]),
            &mut first_replay_seq,
        ),
        (
            34040_u64,
            second_auto_session.as_str(),
            "globalThis.__lm_second_replay_pw_kept = __pw_keptBinding({ source: 'second-replay-main', nested: { count: 2, values: ['b', 3, false] } }); 'scheduled-second-replay'",
            json!([{
                "source": "second-replay-main",
                "nested": { "count": 2, "values": ["b", 3, false] }
            }]),
            &mut second_replay_seq,
        ),
    ] {
        let params = json!({
            "expression": expression,
            "awaitPromise": true
        });
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
                .expect("scheduled replay kept binding wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("__pw_keptBinding")
            })
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "retained __pw_ binding should replay after navigation; events were: {:?}",
                    ctx.sent
                )
            });
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("__pw_keptBinding"));
        assert_eq!(payload["serializedArgs"], serialized_arg);
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }

    for (id, session_id, seq, result, promise_name) in [
        (
            34041_u64,
            first_auto_session.as_str(),
            first_replay_seq,
            "first-replay-kept",
            "__lm_first_replay_pw_kept",
        ),
        (
            34042_u64,
            second_auto_session.as_str(),
            second_replay_seq,
            "second-replay-kept",
            "__lm_second_replay_pw_kept",
        ),
    ] {
        let deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
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

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            34043_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            34044_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_replay_utility_context,
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
            .expect("utility context after cleanup replay navigation");
        ctx.take_all();
    }

    for (
        id,
        session_id,
        utility_context,
        rehydrated_names,
        custom_binding_type,
        retained_binding_type,
    ) in [
        (
            34045_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            vec!["__pw_keptBinding"],
            "undefined",
            "function",
        ),
        (
            34049_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            vec!["sharedCrPageCleanupBinding", "__pw_keptBinding"],
            "function",
            "function",
        ),
    ] {
        for (offset, binding_name) in rehydrated_names.iter().enumerate() {
            ctx.process_async(json!({
                "id": id + offset as u64,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name,
                    "executionContextId": utility_context
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset as u64)["result"],
                json!({})
            );
        }

        let source_base = id + rehydrated_names.len() as u64;
        for (offset, source, expected_type) in [
            (0_u64, custom_wrapper_source.as_str(), custom_binding_type),
            (
                1_u64,
                retained_wrapper_source.as_str(),
                retained_binding_type,
            ),
        ] {
            ctx.process_async(json!({
                "id": source_base + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            let replayed = take_response_by_id(&mut ctx, source_base + offset);
            assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
        }
    }

    let mut first_replay_utility_seq = 0_i64;
    let mut second_replay_utility_seq = 0_i64;
    for (id, session_id, utility_context, expression, serialized_arg, seq_out) in [
        (
            34053_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            "globalThis.__lm_first_replay_pw_kept_utility = __pw_keptBinding({ source: 'first-replay-utility', nested: { count: 3, values: ['c', 4, true] } }); 'scheduled-first-utility'",
            json!([{
                "source": "first-replay-utility",
                "nested": { "count": 3, "values": ["c", 4, true] }
            }]),
            &mut first_replay_utility_seq,
        ),
        (
            34054_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            "globalThis.__lm_second_replay_pw_kept_utility = __pw_keptBinding({ source: 'second-replay-utility', nested: { count: 4, values: ['d', 5, false] } }); 'scheduled-second-utility'",
            json!([{
                "source": "second-replay-utility",
                "nested": { "count": 4, "values": ["d", 5, false] }
            }]),
            &mut second_replay_utility_seq,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": expression,
                "awaitPromise": true
            }
        }))
        .await;
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled utility replay kept binding wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
                .sent
                .iter()
                .find(|message| {
                    message["method"] == json!("Runtime.bindingCalled")
                        && message["params"]["name"] == json!("__pw_keptBinding")
                })
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "retained __pw_ binding should replay into utility world after navigation; events were: {:?}",
                        ctx.sent
                    )
                });
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("__pw_keptBinding"));
        assert_eq!(payload["serializedArgs"], serialized_arg);
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }

    for (id, session_id, utility_context, seq, result, promise_name) in [
        (
            34055_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            first_replay_utility_seq,
            "first-replay-utility-kept",
            "__lm_first_replay_pw_kept_utility",
        ),
        (
            34056_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            second_replay_utility_seq,
            "second-replay-utility-kept",
            "__lm_second_replay_pw_kept_utility",
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": format!(
                        "globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
                    ),
                    "awaitPromise": true
                }
            })).await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": format!("globalThis.{promise_name}"),
                "awaitPromise": true
            }
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
        },
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_crpage_cleanup_replay_keeps_pw_handle_bindings_in_new_utility_contexts_after_navigation()
 {
    super::super::patchright_8mb_stack(
        "patchright-retained-pw-handle-bindings",
        || async {
            run_patchright_over_cdp_crpage_cleanup_replay_keeps_pw_handle_bindings_in_new_utility_contexts_after_navigation()
                .await;
        },
    )
    .await;
}

async fn run_patchright_over_cdp_crpage_cleanup_replay_keeps_pw_handle_bindings_in_new_utility_contexts_after_navigation()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2829, 2830, 2831).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2832, 2833, 2834).await;

    for (id, session_id, html) in [
        (
            2835_u64,
            first.session_id.as_str(),
            "<body><div id='first-initial-handle'>first-initial</div></body>",
        ),
        (
            2836_u64,
            second.session_id.as_str(),
            "<body><div id='second-initial-handle'>second-initial</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,{html}")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            2837_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            2838_u64,
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
        "id": 2839,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(2839, json!({}), None);
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
            2840_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            2841_u64,
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
            .expect("utility context after auto-attach");
        ctx.take_all();
    }

    let custom_wrapper_source = patchright_page_binding_wrapper_source(
        "sharedCrPageHandleCleanupBinding",
        "__lm_shared_crpage_handle_cleanup_deliver",
        Some("__lm_shared_crpage_handle_cleanup_take"),
        true,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptHandleBinding",
        "__lm_pw_kept_handle_binding_deliver",
        Some("__lm_pw_kept_handle_binding_take"),
        true,
    );
    for (id, session_id, utility_context) in [
        (2842_u64, first_auto_session.as_str(), first_utility_context),
        (
            2846_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        for (offset, binding_name) in [
            (0_u64, "sharedCrPageHandleCleanupBinding"),
            (2_u64, "__pw_keptHandleBinding"),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name,
                    "executionContextId": utility_context
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset)["result"],
                json!({})
            );
        }

        for (offset, source) in [
            (1_u64, custom_wrapper_source.as_str()),
            (3_u64, retained_wrapper_source.as_str()),
        ] {
            ctx.process_async(json!({
                "id": id + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            let installed = take_response_by_id(&mut ctx, id + offset);
            assert_eq!(installed["result"]["result"]["value"], json!("function"));
        }
    }

    ctx.process_async(json!({
        "id": 2850,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageHandleCleanupBinding"
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 2850)["result"], json!({}));

    for (id, session_id, html) in [
        (
            2851_u64,
            first_auto_session.as_str(),
            "<body><div id='first-replay-handle'>first-replay</div></body>",
        ),
        (
            2852_u64,
            second_auto_session.as_str(),
            "<body><div id='second-replay-handle'>second-replay</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": {
                "url": format!("data:text/html,{html}")
            }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, session_id, rehydrated_names, custom_binding_type, retained_binding_type) in [
        (
            2901_u64,
            first_auto_session.as_str(),
            vec!["__pw_keptHandleBinding"],
            "undefined",
            "function",
        ),
        (
            2905_u64,
            second_auto_session.as_str(),
            vec!["sharedCrPageHandleCleanupBinding", "__pw_keptHandleBinding"],
            "function",
            "function",
        ),
    ] {
        for (offset, binding_name) in rehydrated_names.iter().enumerate() {
            ctx.process_async(json!({
                "id": id + offset as u64,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset as u64)["result"],
                json!({})
            );
        }

        let source_base = id + rehydrated_names.len() as u64;
        for (offset, source, expected_type) in [
            (0_u64, custom_wrapper_source.as_str(), custom_binding_type),
            (
                1_u64,
                retained_wrapper_source.as_str(),
                retained_binding_type,
            ),
        ] {
            ctx.process_async(json!({
                "id": source_base + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            let replayed = take_response_by_id(&mut ctx, source_base + offset);
            assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
        }
    }

    let mut first_replay_main_handle_seq = 0_i64;
    let mut second_replay_main_handle_seq = 0_i64;
    for (id, session_id, expression, expected_text, seq_out) in [
        (
            2909_u64,
            first_auto_session.as_str(),
            "globalThis.__lm_first_replay_pw_kept_handle_main = __pw_keptHandleBinding(document.getElementById('first-replay-handle')); 'scheduled-first-replay-main-handle'",
            "first-replay",
            &mut first_replay_main_handle_seq,
        ),
        (
            2910_u64,
            second_auto_session.as_str(),
            "globalThis.__lm_second_replay_pw_kept_handle_main = __pw_keptHandleBinding(document.getElementById('second-replay-handle')); 'scheduled-second-replay-main-handle'",
            "second-replay",
            &mut second_replay_main_handle_seq,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": expression,
                "awaitPromise": true
            }
        }))
        .await;
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled retained main handle wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("__pw_keptHandleBinding")
            })
            .cloned()
            .expect(
                "retained __pw_ handle binding should emit Runtime.bindingCalled in main world",
            );
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("__pw_keptHandleBinding"));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();

        ctx.process_async(json!({
                "id": id + 10,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": format!(
                        "(() => {{ const handle = globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {seq} }}); return JSON.stringify([handle.textContent, typeof globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {seq} }})]); }})()",
                        seq = *seq_out
                    )
                }
            })).await;
        let taken = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(
            taken["result"]["result"]["value"],
            json!(format!("[\"{expected_text}\",\"undefined\"]"))
        );
    }

    for (id, session_id, seq, result, promise_name) in [
        (
            2911_u64,
            first_auto_session.as_str(),
            first_replay_main_handle_seq,
            "first-replay-main-handle-resolved",
            "__lm_first_replay_pw_kept_handle_main",
        ),
        (
            2912_u64,
            second_auto_session.as_str(),
            second_replay_main_handle_seq,
            "second-replay-main-handle-resolved",
            "__lm_second_replay_pw_kept_handle_main",
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": format!(
                        "globalThis.__lm_pw_kept_handle_binding_deliver({{ name: '__pw_keptHandleBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
                    ),
                    "awaitPromise": true
                }
            })).await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": format!("globalThis.{promise_name}"),
                "awaitPromise": true
            }
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            2853_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            2854_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_replay_utility_context,
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
            .expect("utility context after cleanup replay navigation");
        ctx.take_all();
    }

    for (
        id,
        session_id,
        utility_context,
        rehydrated_names,
        custom_binding_type,
        retained_binding_type,
    ) in [
        (
            2855_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            vec!["__pw_keptHandleBinding"],
            "undefined",
            "function",
        ),
        (
            2859_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            vec!["sharedCrPageHandleCleanupBinding", "__pw_keptHandleBinding"],
            "function",
            "function",
        ),
    ] {
        for (offset, binding_name) in rehydrated_names.iter().enumerate() {
            ctx.process_async(json!({
                "id": id + offset as u64,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": {
                    "name": binding_name,
                    "executionContextId": utility_context
                }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset as u64)["result"],
                json!({})
            );
        }

        let source_base = id + rehydrated_names.len() as u64;
        for (offset, source, expected_type) in [
            (0_u64, custom_wrapper_source.as_str(), custom_binding_type),
            (
                1_u64,
                retained_wrapper_source.as_str(),
                retained_binding_type,
            ),
        ] {
            ctx.process_async(json!({
                "id": source_base + offset,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": source,
                    "awaitPromise": true
                }
            }))
            .await;
            let replayed = take_response_by_id(&mut ctx, source_base + offset);
            assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
        }
    }

    let mut first_replay_handle_seq = 0_i64;
    let mut second_replay_handle_seq = 0_i64;
    for (id, session_id, utility_context, expression, expected_text, seq_out) in [
        (
            2863_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            "globalThis.__lm_first_replay_pw_kept_handle = __pw_keptHandleBinding(document.getElementById('first-replay-handle')); 'scheduled-first-replay-handle'",
            "first-replay",
            &mut first_replay_handle_seq,
        ),
        (
            2864_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            "globalThis.__lm_second_replay_pw_kept_handle = __pw_keptHandleBinding(document.getElementById('second-replay-handle')); 'scheduled-second-replay-handle'",
            "second-replay",
            &mut second_replay_handle_seq,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": expression,
                "awaitPromise": true
            }
        }))
        .await;
        let scheduled = take_response_by_id(&mut ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled retained handle wrapper value")
                .starts_with("scheduled-")
        );
        let binding_called = ctx
            .sent
            .iter()
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("__pw_keptHandleBinding")
                    && message["params"]["executionContextId"] == json!(utility_context)
            })
            .cloned()
            .expect("retained __pw_ handle binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("__pw_keptHandleBinding"));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();

        ctx.process_async(json!({
                "id": id + 10,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": format!(
                        "(() => {{ const handle = globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {seq} }}); return JSON.stringify([handle.textContent, typeof globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {seq} }})]); }})()",
                        seq = *seq_out
                    )
                }
            })).await;
        let taken = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(
            taken["result"]["result"]["value"],
            json!(format!("[\"{expected_text}\",\"undefined\"]"))
        );
    }

    for (id, session_id, utility_context, seq, result, promise_name) in [
        (
            2865_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            first_replay_handle_seq,
            "first-replay-handle-resolved",
            "__lm_first_replay_pw_kept_handle",
        ),
        (
            2866_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            second_replay_handle_seq,
            "second-replay-handle-resolved",
            "__lm_second_replay_pw_kept_handle",
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": format!(
                        "globalThis.__lm_pw_kept_handle_binding_deliver({{ name: '__pw_keptHandleBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
                    ),
                    "awaitPromise": true
                }
            })).await;
        let delivered = take_response_by_id(&mut ctx, id);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": format!("globalThis.{promise_name}"),
                "awaitPromise": true
            }
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
}
