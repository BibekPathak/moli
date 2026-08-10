use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_remove_binding_sequence_stays_isolated_per_browser_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2603, 2604, 2605).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2606, 2607, 2608).await;

    for (id, session_id, label) in [
        (2609_u64, first.session_id.as_str(), "first-crpage-remove"),
        (2610_u64, second.session_id.as_str(), "second-crpage-remove"),
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

    for (id, target_id, session_id) in [
        (
            2611_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            2612_u64,
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
        "id": 2613,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(2613, json!({}), None);
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
            2614_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            2615_u64,
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
        "sharedCrPageCleanupBinding",
        "__lm_shared_crpage_cleanup_deliver",
        None,
        false,
    );
    for (id, session_id, utility_context) in [
        (2616_u64, first_auto_session.as_str(), first_utility_context),
        (
            2620_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedCrPageCleanupBinding"
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
                "name": "sharedCrPageCleanupBinding",
                "executionContextId": utility_context
            }
        }))
        .await;
        let add_utility_binding = take_response_by_id(&mut ctx, id + 1);
        assert_eq!(add_utility_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 2,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": wrapper_source,
                "awaitPromise": true
            }
        }))
        .await;
        let install_main_wrapper = take_response_by_id(&mut ctx, id + 2);
        assert_eq!(
            install_main_wrapper["result"]["result"]["value"],
            json!("function")
        );

        ctx.process_async(json!({
            "id": id + 3,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": wrapper_source,
                "awaitPromise": true
            }
        }))
        .await;
        let install_utility_wrapper = take_response_by_id(&mut ctx, id + 3);
        assert_eq!(
            install_utility_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    ctx.process_async(json!({
        "id": 2624,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageCleanupBinding"
        }
    }))
    .await;
    let remove_binding = take_response_by_id(&mut ctx, 2624);
    assert_eq!(remove_binding["result"], json!({}));

    for (id, session_id, context_id, expected_type) in [
        (2625_u64, first_auto_session.as_str(), None, "undefined"),
        (
            2626_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "undefined",
        ),
        (2627_u64, second_auto_session.as_str(), None, "function"),
        (
            2628_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "function",
        ),
    ] {
        let mut params = json!({
            "expression": "typeof globalThis.sharedCrPageCleanupBinding"
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
        assert_eq!(state["result"]["result"]["value"], json!(expected_type));
    }

    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (id, context_id, expression, serialized_arg, seq_out, main_context_out) in [
        (
            2629_u64,
            None,
            "globalThis.__lm_second_cleanup_main = sharedCrPageCleanupBinding('second-main'); 'scheduled-second-main'",
            "second-main",
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            2630_u64,
            Some(second_utility_context),
            "globalThis.__lm_second_cleanup_utility = sharedCrPageCleanupBinding('second-utility'); 'scheduled-second-utility'",
            "second-utility",
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
            "sessionId": second_auto_session,
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
                    && message["params"]["name"] == json!("sharedCrPageCleanupBinding")
            })
            .cloned()
            .expect("remaining context should still emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(execution_context_id, second_utility_context);
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("sharedCrPageCleanupBinding"));
        assert_eq!(payload["serializedArgs"], json!([serialized_arg]));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }
    assert_ne!(second_main_context, second_utility_context);

    for (id, context_id, seq, result, promise_name) in [
        (
            2631_u64,
            None,
            second_main_seq,
            "second-main-kept",
            "__lm_second_cleanup_main",
        ),
        (
            2632_u64,
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-kept",
            "__lm_second_cleanup_utility",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_crpage_cleanup_deliver({{ name: 'sharedCrPageCleanupBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": second_auto_session,
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
            "sessionId": second_auto_session,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_remove_binding_sequence_keeps_serialized_object_args_isolated_per_browser_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 26667, 26668, 26669)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 26670, 26671, 26672)
            .await;

    for (id, session_id, label) in [
        (
            26673_u64,
            first.session_id.as_str(),
            "first-crpage-object-cleanup",
        ),
        (
            26674_u64,
            second.session_id.as_str(),
            "second-crpage-object-cleanup",
        ),
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

    for (id, target_id, session_id) in [
        (
            26675_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            26676_u64,
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
        "id": 26677,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(26677, json!({}), None);
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
            26678_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            26679_u64,
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
        "sharedCrPageObjectCleanupBinding",
        "__lm_shared_crpage_object_cleanup_deliver",
        None,
        false,
    );
    for (id, session_id, utility_context) in [
        (
            26680_u64,
            first_auto_session.as_str(),
            first_utility_context,
        ),
        (
            26684_u64,
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
            "sharedCrPageObjectCleanupBinding",
            &wrapper_source,
        )
        .await;
    }

    ctx.process_async(json!({
        "id": 26688,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageObjectCleanupBinding"
        }
    }))
    .await;
    let remove_binding = take_response_by_id(&mut ctx, 26688);
    assert_eq!(remove_binding["result"], json!({}));

    for (id, session_id, context_id, expected_type) in [
        (26689_u64, first_auto_session.as_str(), None, "undefined"),
        (
            26690_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "undefined",
        ),
        (26691_u64, second_auto_session.as_str(), None, "function"),
        (
            26692_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "function",
        ),
    ] {
        let mut params = json!({
            "expression": "typeof globalThis.sharedCrPageObjectCleanupBinding"
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
        assert_eq!(state["result"]["result"]["value"], json!(expected_type));
    }

    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (id, context_id, expression, serialized_arg, seq_out, main_context_out) in [
        (
            26693_u64,
            None,
            "globalThis.__lm_second_cleanup_object_main = sharedCrPageObjectCleanupBinding({ source: 'second-main', nested: { count: 3, values: ['c', 4, true] } }); 'scheduled-second-main'",
            json!([{
                "source": "second-main",
                "nested": { "count": 3, "values": ["c", 4, true] }
            }]),
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            26694_u64,
            Some(second_utility_context),
            "globalThis.__lm_second_cleanup_object_utility = sharedCrPageObjectCleanupBinding({ source: 'second-utility', nested: { count: 4, values: ['d', 5, false] } }); 'scheduled-second-utility'",
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
            "sessionId": second_auto_session,
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
                    && message["params"]["name"] == json!("sharedCrPageObjectCleanupBinding")
            })
            .cloned()
            .expect("remaining context should still emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(execution_context_id, second_utility_context);
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("sharedCrPageObjectCleanupBinding"));
        assert_eq!(payload["serializedArgs"], serialized_arg);
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }
    assert_ne!(second_main_context, second_utility_context);

    for (id, context_id, seq, result, promise_name) in [
        (
            26695_u64,
            None,
            second_main_seq,
            "second-main-object-kept",
            "__lm_second_cleanup_object_main",
        ),
        (
            26696_u64,
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-object-kept",
            "__lm_second_cleanup_object_utility",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_crpage_object_cleanup_deliver({{ name: 'sharedCrPageObjectCleanupBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": second_auto_session,
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
            "sessionId": second_auto_session,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_remove_binding_rejection_sequence_stays_isolated_per_browser_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2633, 2634, 2635).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2636, 2637, 2638).await;

    for (id, session_id, label) in [
        (
            2639_u64,
            first.session_id.as_str(),
            "first-crpage-cleanup-reject",
        ),
        (
            2640_u64,
            second.session_id.as_str(),
            "second-crpage-cleanup-reject",
        ),
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

    for (id, target_id, session_id) in [
        (
            2641_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            2642_u64,
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
        "id": 2643,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(2643, json!({}), None);
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
            2644_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            2645_u64,
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
        "sharedCrPageRejectingCleanupBinding",
        "__lm_shared_crpage_rejecting_cleanup_deliver",
        None,
        false,
    );
    for (id, session_id, utility_context) in [
        (2646_u64, first_auto_session.as_str(), first_utility_context),
        (
            2650_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedCrPageRejectingCleanupBinding"
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
                "name": "sharedCrPageRejectingCleanupBinding",
                "executionContextId": utility_context
            }
        }))
        .await;
        let add_utility_binding = take_response_by_id(&mut ctx, id + 1);
        assert_eq!(add_utility_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 2,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": wrapper_source,
                "awaitPromise": true
            }
        }))
        .await;
        let install_main_wrapper = take_response_by_id(&mut ctx, id + 2);
        assert_eq!(
            install_main_wrapper["result"]["result"]["value"],
            json!("function")
        );

        ctx.process_async(json!({
            "id": id + 3,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": wrapper_source,
                "awaitPromise": true
            }
        }))
        .await;
        let install_utility_wrapper = take_response_by_id(&mut ctx, id + 3);
        assert_eq!(
            install_utility_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    ctx.process_async(json!({
        "id": 2654,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageRejectingCleanupBinding"
        }
    }))
    .await;
    let remove_binding = take_response_by_id(&mut ctx, 2654);
    assert_eq!(remove_binding["result"], json!({}));

    for (id, session_id, context_id, expected_type) in [
        (2655_u64, first_auto_session.as_str(), None, "undefined"),
        (
            2656_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "undefined",
        ),
        (2657_u64, second_auto_session.as_str(), None, "function"),
        (
            2658_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "function",
        ),
    ] {
        let mut params = json!({
            "expression": "typeof globalThis.sharedCrPageRejectingCleanupBinding"
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
        assert_eq!(state["result"]["result"]["value"], json!(expected_type));
    }

    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (id, context_id, expression, serialized_arg, seq_out, main_context_out) in [
        (
            2659_u64,
            None,
            "globalThis.__lm_second_cleanup_reject_main = sharedCrPageRejectingCleanupBinding('second-main').then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-second-main'",
            "second-main",
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            2660_u64,
            Some(second_utility_context),
            "globalThis.__lm_second_cleanup_reject_utility = sharedCrPageRejectingCleanupBinding('second-utility').then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-second-utility'",
            "second-utility",
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
            "sessionId": second_auto_session,
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
                    && message["params"]["name"] == json!("sharedCrPageRejectingCleanupBinding")
            })
            .cloned()
            .expect("remaining context should still emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(execution_context_id, second_utility_context);
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(
            payload["name"],
            json!("sharedCrPageRejectingCleanupBinding")
        );
        assert_eq!(payload["serializedArgs"], json!([serialized_arg]));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();
    }
    assert_ne!(second_main_context, second_utility_context);

    for (id, context_id, seq, error, promise_name) in [
        (
            2661_u64,
            None,
            second_main_seq,
            "second-main-rejected",
            "__lm_second_cleanup_reject_main",
        ),
        (
            2662_u64,
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-rejected",
            "__lm_second_cleanup_reject_utility",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_crpage_rejecting_cleanup_deliver({{ name: 'sharedCrPageRejectingCleanupBinding', seq: {seq}, error: '{error}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": second_auto_session,
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
            "sessionId": second_auto_session,
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

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_remove_handle_binding_sequence_stays_isolated_per_browser_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2663, 2664, 2665).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2666, 2667, 2668).await;

    for (id, session_id, html) in [
        (
            2669_u64,
            first.session_id.as_str(),
            "<body><div id='first-main-handle'>first-main</div><div id='first-utility-handle'>first-utility</div></body>",
        ),
        (
            2670_u64,
            second.session_id.as_str(),
            "<body><div id='second-main-handle'>second-main</div><div id='second-utility-handle'>second-utility</div></body>",
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
            2671_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            2672_u64,
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
        "id": 2673,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(2673, json!({}), None);
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
            2674_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            2675_u64,
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
        "sharedCrPageHandleCleanupBinding",
        "__lm_shared_crpage_handle_cleanup_deliver",
        Some("__lm_shared_crpage_handle_cleanup_take"),
        true,
    );
    for (id, session_id, utility_context) in [
        (2676_u64, first_auto_session.as_str(), first_utility_context),
        (
            2680_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedCrPageHandleCleanupBinding"
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
                "name": "sharedCrPageHandleCleanupBinding",
                "executionContextId": utility_context
            }
        }))
        .await;
        let add_utility_binding = take_response_by_id(&mut ctx, id + 1);
        assert_eq!(add_utility_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 2,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": wrapper_source,
                "awaitPromise": true
            }
        }))
        .await;
        let install_main_wrapper = take_response_by_id(&mut ctx, id + 2);
        assert_eq!(
            install_main_wrapper["result"]["result"]["value"],
            json!("function")
        );

        ctx.process_async(json!({
            "id": id + 3,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": wrapper_source,
                "awaitPromise": true
            }
        }))
        .await;
        let install_utility_wrapper = take_response_by_id(&mut ctx, id + 3);
        assert_eq!(
            install_utility_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    ctx.process_async(json!({
        "id": 2684,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageHandleCleanupBinding"
        }
    }))
    .await;
    let remove_binding = take_response_by_id(&mut ctx, 2684);
    assert_eq!(remove_binding["result"], json!({}));

    for (id, session_id, context_id, expected_type) in [
        (2685_u64, first_auto_session.as_str(), None, "undefined"),
        (
            2686_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "undefined",
        ),
        (2687_u64, second_auto_session.as_str(), None, "function"),
        (
            2688_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "function",
        ),
    ] {
        let mut params = json!({
            "expression": "typeof globalThis.sharedCrPageHandleCleanupBinding"
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
        assert_eq!(state["result"]["result"]["value"], json!(expected_type));
    }

    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (id, context_id, expression, handle_id, expected_text, seq_out, main_context_out) in [
        (
            2689_u64,
            None,
            "globalThis.__lm_second_cleanup_handle_main = sharedCrPageHandleCleanupBinding(document.getElementById('second-main-handle')); 'scheduled-second-main'",
            "second-main-handle",
            "second-main",
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            2690_u64,
            Some(second_utility_context),
            "globalThis.__lm_second_cleanup_handle_utility = sharedCrPageHandleCleanupBinding(document.getElementById('second-utility-handle')); 'scheduled-second-utility'",
            "second-utility-handle",
            "second-utility",
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
            "sessionId": second_auto_session,
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
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"] == json!("sharedCrPageHandleCleanupBinding")
            })
            .cloned()
            .expect("remaining context should still emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(execution_context_id, second_utility_context);
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!("sharedCrPageHandleCleanupBinding"));
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();

        let mut take_params = json!({
            "expression": format!(
                "(() => {{ const handle = globalThis.__lm_shared_crpage_handle_cleanup_take({{ name: 'sharedCrPageHandleCleanupBinding', seq: {} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_shared_crpage_handle_cleanup_take({{ name: 'sharedCrPageHandleCleanupBinding', seq: {} }})]); }})()",
                *seq_out, *seq_out
            )
        });
        if let Some(context_id) = context_id {
            take_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": second_auto_session,
            "params": take_params
        }))
        .await;
        let taken = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(
            taken["result"]["result"]["value"],
            json!(format!(
                "[\"{handle_id}\",\"{expected_text}\",\"undefined\"]"
            ))
        );
    }
    assert_ne!(second_main_context, second_utility_context);

    for (id, context_id, seq, result, promise_name) in [
        (
            2691_u64,
            None,
            second_main_seq,
            "second-main-kept",
            "__lm_second_cleanup_handle_main",
        ),
        (
            2692_u64,
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-kept",
            "__lm_second_cleanup_handle_utility",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_crpage_handle_cleanup_deliver({{ name: 'sharedCrPageHandleCleanupBinding', seq: {seq}, result: '{result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": second_auto_session,
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
            "sessionId": second_auto_session,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(resolved["result"]["result"]["value"], json!(result));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_remove_handle_binding_rejection_sequence_stays_isolated_per_browser_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2693, 2694, 2695).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 2696, 2697, 2698).await;

    for (id, session_id, html) in [
        (
            2699_u64,
            first.session_id.as_str(),
            "<body><div id='first-main-handle'>first-main</div><div id='first-utility-handle'>first-utility</div></body>",
        ),
        (
            2700_u64,
            second.session_id.as_str(),
            "<body><div id='second-main-handle'>second-main</div><div id='second-utility-handle'>second-utility</div></body>",
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
            2701_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            2702_u64,
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
        "id": 2703,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(2703, json!({}), None);
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
            2704_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            2705_u64,
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
        "sharedCrPageRejectingHandleCleanupBinding",
        "__lm_shared_crpage_rejecting_handle_cleanup_deliver",
        Some("__lm_shared_crpage_rejecting_handle_cleanup_take"),
        true,
    );
    for (id, session_id, utility_context) in [
        (2706_u64, first_auto_session.as_str(), first_utility_context),
        (
            2710_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.addBinding",
            "sessionId": session_id,
            "params": {
                "name": "sharedCrPageRejectingHandleCleanupBinding"
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
                "name": "sharedCrPageRejectingHandleCleanupBinding",
                "executionContextId": utility_context
            }
        }))
        .await;
        let add_utility_binding = take_response_by_id(&mut ctx, id + 1);
        assert_eq!(add_utility_binding["result"], json!({}));

        ctx.process_async(json!({
            "id": id + 2,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": wrapper_source,
                "awaitPromise": true
            }
        }))
        .await;
        let install_main_wrapper = take_response_by_id(&mut ctx, id + 2);
        assert_eq!(
            install_main_wrapper["result"]["result"]["value"],
            json!("function")
        );

        ctx.process_async(json!({
            "id": id + 3,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": wrapper_source,
                "awaitPromise": true
            }
        }))
        .await;
        let install_utility_wrapper = take_response_by_id(&mut ctx, id + 3);
        assert_eq!(
            install_utility_wrapper["result"]["result"]["value"],
            json!("function")
        );
    }

    ctx.process_async(json!({
        "id": 2714,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "sharedCrPageRejectingHandleCleanupBinding"
        }
    }))
    .await;
    let remove_binding = take_response_by_id(&mut ctx, 2714);
    assert_eq!(remove_binding["result"], json!({}));

    for (id, session_id, context_id, expected_type) in [
        (2715_u64, first_auto_session.as_str(), None, "undefined"),
        (
            2716_u64,
            first_auto_session.as_str(),
            Some(first_utility_context),
            "undefined",
        ),
        (2717_u64, second_auto_session.as_str(), None, "function"),
        (
            2718_u64,
            second_auto_session.as_str(),
            Some(second_utility_context),
            "function",
        ),
    ] {
        let mut params = json!({
            "expression": "typeof globalThis.sharedCrPageRejectingHandleCleanupBinding"
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
        assert_eq!(state["result"]["result"]["value"], json!(expected_type));
    }

    let mut second_main_seq = 0_i64;
    let mut second_main_context = 0_i64;
    let mut second_utility_seq = 0_i64;
    for (id, context_id, expression, handle_id, expected_text, seq_out, main_context_out) in [
        (
            2719_u64,
            None,
            "globalThis.__lm_second_cleanup_reject_handle_main = sharedCrPageRejectingHandleCleanupBinding(document.getElementById('second-main-handle')).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-second-main'",
            "second-main-handle",
            "second-main",
            &mut second_main_seq,
            Some(&mut second_main_context),
        ),
        (
            2720_u64,
            Some(second_utility_context),
            "globalThis.__lm_second_cleanup_reject_handle_utility = sharedCrPageRejectingHandleCleanupBinding(document.getElementById('second-utility-handle')).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-second-utility'",
            "second-utility-handle",
            "second-utility",
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
            "sessionId": second_auto_session,
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
            .find(|message| {
                message["method"] == json!("Runtime.bindingCalled")
                    && message["params"]["name"]
                        == json!("sharedCrPageRejectingHandleCleanupBinding")
            })
            .cloned()
            .expect("remaining context should still emit Runtime.bindingCalled");
        let execution_context_id = binding_called["params"]["executionContextId"]
            .as_i64()
            .expect("execution context id");
        if let Some(main_context_out) = main_context_out {
            *main_context_out = execution_context_id;
        } else {
            assert_eq!(execution_context_id, second_utility_context);
        }
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(
            payload["name"],
            json!("sharedCrPageRejectingHandleCleanupBinding")
        );
        *seq_out = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(*seq_out, 1);
        ctx.sent.clear();

        let mut take_params = json!({
            "expression": format!(
                "(() => {{ const handle = globalThis.__lm_shared_crpage_rejecting_handle_cleanup_take({{ name: 'sharedCrPageRejectingHandleCleanupBinding', seq: {} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_shared_crpage_rejecting_handle_cleanup_take({{ name: 'sharedCrPageRejectingHandleCleanupBinding', seq: {} }})]); }})()",
                *seq_out, *seq_out
            )
        });
        if let Some(context_id) = context_id {
            take_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": second_auto_session,
            "params": take_params
        }))
        .await;
        let taken = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(
            taken["result"]["result"]["value"],
            json!(format!(
                "[\"{handle_id}\",\"{expected_text}\",\"undefined\"]"
            ))
        );
    }
    assert_ne!(second_main_context, second_utility_context);

    for (id, context_id, seq, error, promise_name) in [
        (
            2721_u64,
            None,
            second_main_seq,
            "second-main-rejected",
            "__lm_second_cleanup_reject_handle_main",
        ),
        (
            2722_u64,
            Some(second_utility_context),
            second_utility_seq,
            "second-utility-rejected",
            "__lm_second_cleanup_reject_handle_utility",
        ),
    ] {
        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.__lm_shared_crpage_rejecting_handle_cleanup_deliver({{ name: 'sharedCrPageRejectingHandleCleanupBinding', seq: {seq}, error: '{error}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": second_auto_session,
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
            "sessionId": second_auto_session,
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
