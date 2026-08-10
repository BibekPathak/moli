use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_replacement_targets_keep_thin_handle_cleanup_isolated_per_browser_context_without_runtime_enable()
 {
    patchright_replacement_targets_large_stack(|| async {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36000, 36001, 36002)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36003, 36004, 36005)
            .await;

    for (id, session_id, html) in [
        (
            36006_u64,
            first.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-first-handle-a</div></body>",
        ),
        (
            36007_u64,
            second.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-second-handle-a</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36008_u64,
            first.session_id.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            36009_u64,
            second.session_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin handle initial utility context id");
        ctx.take_all();
    }

    let custom_handle_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingA",
        "__lm_custom_handle_binding_a_deliver",
        Some("__lm_custom_handle_binding_a_take"),
        true,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptBinding",
        "__lm_pw_kept_binding_deliver",
        None,
        false,
    );

    for (session_id, utility_context_id, id_base) in [
        (first.session_id.as_str(), first_utility_context, 36010_u64),
        (
            second.session_id.as_str(),
            second_utility_context,
            36020_u64,
        ),
    ] {
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base,
            id_base + 1,
            id_base + 2,
            id_base + 3,
            "customHandleBindingA",
            &custom_handle_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 4,
            id_base + 5,
            id_base + 6,
            id_base + 7,
            "__pw_keptBinding",
            &retained_wrapper_source,
        )
        .await;
    }

    for (session_id, id_base) in [
        (first.session_id.as_str(), 36030_u64),
        (second.session_id.as_str(), 36040_u64),
    ] {
        for (source, world_name, offset) in [
            (custom_handle_wrapper_source.as_str(), None, 0_u64),
            (retained_wrapper_source.as_str(), None, 1_u64),
            (
                custom_handle_wrapper_source.as_str(),
                Some("utility"),
                2_u64,
            ),
            (retained_wrapper_source.as_str(), Some("utility"), 3_u64),
        ] {
            let mut params = json!({
                "source": source,
                "runImmediately": true
            });
            if let Some(world_name) = world_name {
                params["worldName"] = json!(world_name);
            }
            ctx.process_async(json!({
                "id": id_base + offset,
                "method": "Page.addScriptToEvaluateOnNewDocument",
                "sessionId": session_id,
                "params": params
            }))
            .await;
            assert!(
                take_response_by_id(&mut ctx, id_base + offset)["result"]["identifier"]
                    .as_str()
                    .is_some()
            );
        }
    }

    ctx.process_async(json!({
        "id": 36050,
        "method": "Runtime.removeBinding",
        "sessionId": first.session_id,
        "params": { "name": "customHandleBindingA" }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 36050)["result"], json!({}));

    for (id, target_id) in [
        (36051_u64, first.target_id.as_str()),
        (36052_u64, second.target_id.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.closeTarget",
            "params": { "targetId": target_id }
        }))
        .await;
        ctx.expect_result(id, json!({ "success": true }), None);
        ctx.take_all();
    }

    let first_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &first.browser_context_id,
        36053,
        36054,
    )
    .await;
    let second_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &second.browser_context_id,
        36055,
        36056,
    )
    .await;

    for (id, session_id, html) in [
        (
            36057_u64,
            first_replacement.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-first-replacement-handle-a</div></body>",
        ),
        (
            36058_u64,
            second_replacement.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-second-replacement-handle-a</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            36059_u64,
            first_replacement.target_id.as_str(),
            first_replacement.session_id.as_str(),
        ),
        (
            36060_u64,
            second_replacement.target_id.as_str(),
            second_replacement.session_id.as_str(),
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
        "id": 36061,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(36061, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(attached_events.len(), 2);
    let first_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(first_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin handle first replacement re-auto-attached session")
        .to_owned();
    let second_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(second_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin handle second replacement re-auto-attached session")
        .to_owned();

    for (id, session_id, source, expected_type) in [
        (
            36062_u64,
            first_reauto_session.as_str(),
            custom_handle_wrapper_source.as_str(),
            "undefined",
        ),
        (
            36063_u64,
            first_reauto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            36064_u64,
            second_reauto_session.as_str(),
            custom_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            36065_u64,
            second_reauto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": source,
                "awaitPromise": true
            }
        }))
        .await;
        let installed = take_response_by_id(&mut ctx, id);
        assert_eq!(installed["result"]["result"]["value"], json!(expected_type));
    }

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36066_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            36067_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin handle replay utility context id");
        ctx.take_all();
    }

    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36068,
        36069,
        36070,
        36071,
        "__pw_keptBinding",
        &retained_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36072,
        36073,
        36074,
        36075,
        "customHandleBindingA",
        &custom_handle_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36076,
        36077,
        36078,
        36079,
        "__pw_keptBinding",
        &retained_wrapper_source,
    )
    .await;

    for (id, session_id, context_id, expected_state) in [
        (
            36080_u64,
            first_reauto_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"function\"]"),
        ),
        (
            36081_u64,
            first_reauto_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"function\"]"),
        ),
        (
            36082_u64,
            second_reauto_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\"]"),
        ),
        (
            36083_u64,
            second_reauto_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customHandleBindingA, typeof globalThis.__pw_keptBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36084,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": "globalThis.__lm_thin_second_custom_handle_a = customHandleBindingA(document.getElementById('utility-handle-a')); 'scheduled-thin-second-custom-handle-a'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_custom_handle = take_response_by_id(&mut ctx, 36084);
    assert!(
        scheduled_custom_handle["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin second custom handle")
            .starts_with("scheduled-")
    );
    let custom_handle_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reauto_session)
                && message["params"]["name"] == json!("customHandleBindingA")
                && message["params"]["executionContextId"] == json!(second_replay_utility_context)
        })
        .cloned()
        .expect("thin second custom handle bindingCalled");
    let custom_handle_payload: serde_json::Value = serde_json::from_str(
        custom_handle_called["params"]["payload"]
            .as_str()
            .expect("thin second custom handle payload string"),
    )
    .expect("thin second custom handle payload json");
    let custom_handle_seq = custom_handle_payload["seq"]
        .as_i64()
        .expect("thin second custom handle seq");
    assert_eq!(custom_handle_seq, 1);
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36085,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": format!("(() => {{ const handle = globalThis.__lm_custom_handle_binding_a_take({{ name: 'customHandleBindingA', seq: {custom_handle_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_custom_handle_binding_a_take({{ name: 'customHandleBindingA', seq: {custom_handle_seq} }})]); }})()")
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36085)["result"]["result"]["value"],
        json!("[\"utility-handle-a\",\"thin-second-replacement-handle-a\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 36086,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": format!("globalThis.__lm_custom_handle_binding_a_deliver({{ name: 'customHandleBindingA', seq: {custom_handle_seq}, result: 'thin-second-custom-handle-a-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36086)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36087,
        "method": "Runtime.evaluate",
        "sessionId": second_reauto_session,
        "params": {
            "contextId": second_replay_utility_context,
            "expression": "globalThis.__lm_thin_second_custom_handle_a",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36087)["result"]["result"]["value"],
        json!("thin-second-custom-handle-a-ok")
    );

    ctx.process_async(json!({
            "id": 36088,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "expression": "globalThis.__lm_thin_first_pw_binding = __pw_keptBinding({ source: 'thin-first-pw-binding', nested: { count: 16, values: ['thin', 17, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-first-pw-binding'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_pw_binding = take_response_by_id(&mut ctx, 36088);
    assert!(
        scheduled_pw_binding["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin first pw binding")
            .starts_with("scheduled-")
    );
    let pw_binding_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reauto_session)
                && message["params"]["name"] == json!("__pw_keptBinding")
        })
        .cloned()
        .expect("thin first pw binding bindingCalled");
    let pw_binding_payload: serde_json::Value = serde_json::from_str(
        pw_binding_called["params"]["payload"]
            .as_str()
            .expect("thin first pw binding payload string"),
    )
    .expect("thin first pw binding payload json");
    let pw_binding_seq = pw_binding_payload["seq"]
        .as_i64()
        .expect("thin first pw binding seq");
    assert_eq!(
        pw_binding_payload["serializedArgs"],
        json!([{ "source": "thin-first-pw-binding", "nested": { "count": 16, "values": ["thin", 17, false] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36089,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "expression": format!("globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {pw_binding_seq}, error: 'thin-first-pw-binding-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36089)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36090,
        "method": "Runtime.evaluate",
        "sessionId": first_reauto_session,
        "params": {
            "expression": "globalThis.__lm_thin_first_pw_binding",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36090)["result"]["result"]["value"],
        json!("rejected:thin-first-pw-binding-error")
    );

    for (id, session_id, target_id) in [
        (
            36091_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
        ),
        (
            36092_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": {
                "targetId": target_id,
                "sessionId": session_id,
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
        "id": 36093,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": first_replacement.target_id,
            "flatten": true
        }
    }))
    .await;
    let first_reattached_session = take_response_by_id(&mut ctx, 36093)["result"]["sessionId"]
        .as_str()
        .expect("thin handle first reattached session id")
        .to_owned();
    assert_ne!(first_reattached_session, first_reauto_session);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": first_reattached_session,
            "targetInfo": {
                "targetId": first_replacement.target_id,
                "browserContextId": first.browser_context_id,
            }
        })),
    );
    ctx.take_all();

    ctx.process_async(json!({
        "id": 36094,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": second_replacement.target_id,
            "flatten": true
        }
    }))
    .await;
    let second_reattached_session = take_response_by_id(&mut ctx, 36094)["result"]["sessionId"]
        .as_str()
        .expect("thin handle second reattached session id")
        .to_owned();
    assert_ne!(second_reattached_session, second_reauto_session);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": second_reattached_session,
            "targetInfo": {
                "targetId": second_replacement.target_id,
                "browserContextId": second.browser_context_id,
            }
        })),
    );
    ctx.take_all();

    for (id, session_id, context_id, expected_state) in [
        (
            36095_u64,
            first_reattached_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"function\"]"),
        ),
        (
            36096_u64,
            first_reattached_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"function\"]"),
        ),
        (
            36097_u64,
            second_reattached_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\"]"),
        ),
        (
            36098_u64,
            second_reattached_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customHandleBindingA, typeof globalThis.__pw_keptBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36099,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": "globalThis.__lm_thin_reattached_second_custom_handle_a = customHandleBindingA(document.getElementById('utility-handle-a')); 'scheduled-thin-reattached-second-custom-handle-a'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_reattached_custom_handle = take_response_by_id(&mut ctx, 36099);
    assert!(
        scheduled_reattached_custom_handle["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin reattached second custom handle")
            .starts_with("scheduled-")
    );
    let reattached_custom_handle_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reattached_session)
                && message["params"]["name"] == json!("customHandleBindingA")
                && message["params"]["executionContextId"] == json!(second_replay_utility_context)
        })
        .cloned()
        .expect("thin reattached second custom handle bindingCalled");
    let reattached_custom_handle_payload: serde_json::Value = serde_json::from_str(
        reattached_custom_handle_called["params"]["payload"]
            .as_str()
            .expect("thin reattached second custom handle payload string"),
    )
    .expect("thin reattached second custom handle payload json");
    let reattached_custom_handle_seq = reattached_custom_handle_payload["seq"]
        .as_i64()
        .expect("thin reattached second custom handle seq");
    assert_eq!(reattached_custom_handle_seq, 2);
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36100,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": format!("(() => {{ const handle = globalThis.__lm_custom_handle_binding_a_take({{ name: 'customHandleBindingA', seq: {reattached_custom_handle_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_custom_handle_binding_a_take({{ name: 'customHandleBindingA', seq: {reattached_custom_handle_seq} }})]); }})()")
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36100)["result"]["result"]["value"],
        json!("[\"utility-handle-a\",\"thin-second-replacement-handle-a\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 36101,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": format!("globalThis.__lm_custom_handle_binding_a_deliver({{ name: 'customHandleBindingA', seq: {reattached_custom_handle_seq}, result: 'thin-reattached-second-custom-handle-a-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36101)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36102,
        "method": "Runtime.evaluate",
        "sessionId": second_reattached_session,
        "params": {
            "contextId": second_replay_utility_context,
            "expression": "globalThis.__lm_thin_reattached_second_custom_handle_a",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36102)["result"]["result"]["value"],
        json!("thin-reattached-second-custom-handle-a-ok")
    );

    ctx.process_async(json!({
            "id": 36103,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "expression": "globalThis.__lm_thin_reattached_first_pw_binding = __pw_keptBinding({ source: 'thin-reattached-first-pw-binding', nested: { count: 18, values: ['reattach', 19, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-reattached-first-pw-binding'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_reattached_pw_binding = take_response_by_id(&mut ctx, 36103);
    assert!(
        scheduled_reattached_pw_binding["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin reattached first pw binding")
            .starts_with("scheduled-")
    );
    let reattached_pw_binding_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reattached_session)
                && message["params"]["name"] == json!("__pw_keptBinding")
        })
        .cloned()
        .expect("thin reattached first pw binding bindingCalled");
    let reattached_pw_binding_payload: serde_json::Value = serde_json::from_str(
        reattached_pw_binding_called["params"]["payload"]
            .as_str()
            .expect("thin reattached first pw binding payload string"),
    )
    .expect("thin reattached first pw binding payload json");
    let reattached_pw_binding_seq = reattached_pw_binding_payload["seq"]
        .as_i64()
        .expect("thin reattached first pw binding seq");
    assert_eq!(reattached_pw_binding_seq, 2);
    assert_eq!(
        reattached_pw_binding_payload["serializedArgs"],
        json!([{ "source": "thin-reattached-first-pw-binding", "nested": { "count": 18, "values": ["reattach", 19, false] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36104,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "expression": format!("globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {reattached_pw_binding_seq}, error: 'thin-reattached-first-pw-binding-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36104)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36105,
        "method": "Runtime.evaluate",
        "sessionId": first_reattached_session,
        "params": {
            "expression": "globalThis.__lm_thin_reattached_first_pw_binding",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36105)["result"]["result"]["value"],
        json!("rejected:thin-reattached-first-pw-binding-error")
    );
    })
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_replacement_targets_keep_thin_name_cleanup_isolated_per_browser_context_without_runtime_enable()
 {
    patchright_replacement_targets_large_stack(|| async {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36110, 36111, 36112)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36113, 36114, 36115)
            .await;

    for (id, session_id, html) in [
        (
            36116_u64,
            first.session_id.as_str(),
            "<body><div id='page'>thin-first-name</div></body>",
        ),
        (
            36117_u64,
            second.session_id.as_str(),
            "<body><div id='page'>thin-second-name</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36118_u64,
            first.session_id.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            36119_u64,
            second.session_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin name initial utility context id");
        ctx.take_all();
    }

    let custom_a_wrapper_source = patchright_page_binding_wrapper_source(
        "customBindingA",
        "__lm_custom_binding_a_deliver",
        None,
        false,
    );
    let custom_b_wrapper_source = patchright_page_binding_wrapper_source(
        "customBindingB",
        "__lm_custom_binding_b_deliver",
        None,
        false,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptBinding",
        "__lm_pw_kept_binding_deliver",
        None,
        false,
    );

    for (session_id, utility_context_id, id_base) in [
        (first.session_id.as_str(), first_utility_context, 36120_u64),
        (
            second.session_id.as_str(),
            second_utility_context,
            36132_u64,
        ),
    ] {
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base,
            id_base + 1,
            id_base + 2,
            id_base + 3,
            "customBindingA",
            &custom_a_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 4,
            id_base + 5,
            id_base + 6,
            id_base + 7,
            "customBindingB",
            &custom_b_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 8,
            id_base + 9,
            id_base + 10,
            id_base + 11,
            "__pw_keptBinding",
            &retained_wrapper_source,
        )
        .await;
    }

    for (session_id, id_base) in [
        (first.session_id.as_str(), 36144_u64),
        (second.session_id.as_str(), 36150_u64),
    ] {
        for (source, world_name, offset) in [
            (custom_a_wrapper_source.as_str(), None, 0_u64),
            (custom_b_wrapper_source.as_str(), None, 1_u64),
            (retained_wrapper_source.as_str(), None, 2_u64),
            (custom_a_wrapper_source.as_str(), Some("utility"), 3_u64),
            (custom_b_wrapper_source.as_str(), Some("utility"), 4_u64),
            (retained_wrapper_source.as_str(), Some("utility"), 5_u64),
        ] {
            let mut params = json!({
                "source": source,
                "runImmediately": true
            });
            if let Some(world_name) = world_name {
                params["worldName"] = json!(world_name);
            }
            ctx.process_async(json!({
                "id": id_base + offset,
                "method": "Page.addScriptToEvaluateOnNewDocument",
                "sessionId": session_id,
                "params": params
            }))
            .await;
            assert!(
                take_response_by_id(&mut ctx, id_base + offset)["result"]["identifier"]
                    .as_str()
                    .is_some()
            );
        }
    }

    ctx.process_async(json!({
        "id": 36156,
        "method": "Runtime.removeBinding",
        "sessionId": first.session_id,
        "params": { "name": "customBindingA" }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 36156)["result"], json!({}));

    for (id, target_id) in [
        (36157_u64, first.target_id.as_str()),
        (36158_u64, second.target_id.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.closeTarget",
            "params": { "targetId": target_id }
        }))
        .await;
        ctx.expect_result(id, json!({ "success": true }), None);
        ctx.take_all();
    }

    let first_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &first.browser_context_id,
        36159,
        36160,
    )
    .await;
    let second_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &second.browser_context_id,
        36161,
        36162,
    )
    .await;

    for (id, session_id, html) in [
        (
            36163_u64,
            first_replacement.session_id.as_str(),
            "<body><div id='page'>thin-first-name-replacement</div></body>",
        ),
        (
            36164_u64,
            second_replacement.session_id.as_str(),
            "<body><div id='page'>thin-second-name-replacement</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            36165_u64,
            first_replacement.target_id.as_str(),
            first_replacement.session_id.as_str(),
        ),
        (
            36166_u64,
            second_replacement.target_id.as_str(),
            second_replacement.session_id.as_str(),
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
        "id": 36167,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(36167, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(attached_events.len(), 2);
    let first_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(first_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin name first replacement re-auto-attached session")
        .to_owned();
    let second_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(second_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin name second replacement re-auto-attached session")
        .to_owned();

    for (id, session_id, source, expected_type) in [
        (
            36168_u64,
            first_reauto_session.as_str(),
            custom_a_wrapper_source.as_str(),
            "undefined",
        ),
        (
            36169_u64,
            first_reauto_session.as_str(),
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            36170_u64,
            first_reauto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            36171_u64,
            second_reauto_session.as_str(),
            custom_a_wrapper_source.as_str(),
            "function",
        ),
        (
            36172_u64,
            second_reauto_session.as_str(),
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            36173_u64,
            second_reauto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": source,
                "awaitPromise": true
            }
        }))
        .await;
        let installed = take_response_by_id(&mut ctx, id);
        assert_eq!(installed["result"]["result"]["value"], json!(expected_type));
    }

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36174_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            36175_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin name replay utility context id");
        ctx.take_all();
    }

    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36176,
        36177,
        36178,
        36179,
        "customBindingB",
        &custom_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36180,
        36181,
        36182,
        36183,
        "__pw_keptBinding",
        &retained_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36184,
        36185,
        36186,
        36187,
        "customBindingA",
        &custom_a_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36188,
        36189,
        36190,
        36191,
        "customBindingB",
        &custom_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36192,
        36193,
        36194,
        36195,
        "__pw_keptBinding",
        &retained_wrapper_source,
    )
    .await;

    for (id, session_id, context_id, expected_state) in [
        (
            36196_u64,
            first_reauto_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36197_u64,
            first_reauto_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36198_u64,
            second_reauto_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\",\"function\"]"),
        ),
        (
            36199_u64,
            second_reauto_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customBindingB, typeof globalThis.__pw_keptBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36200,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": "globalThis.__lm_thin_first_custom_b = customBindingB({ source: 'thin-first-custom-b', nested: { count: 20, values: ['first', 21, true] } }); 'scheduled-thin-first-custom-b'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_first_custom_b = take_response_by_id(&mut ctx, 36200);
    assert!(
        scheduled_first_custom_b["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin first custom b")
            .starts_with("scheduled-")
    );
    let first_custom_b_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reauto_session)
                && message["params"]["name"] == json!("customBindingB")
                && message["params"]["executionContextId"] == json!(first_replay_utility_context)
        })
        .cloned()
        .expect("thin first custom b bindingCalled");
    let first_custom_b_payload: serde_json::Value = serde_json::from_str(
        first_custom_b_called["params"]["payload"]
            .as_str()
            .expect("thin first custom b payload string"),
    )
    .expect("thin first custom b payload json");
    let first_custom_b_seq = first_custom_b_payload["seq"]
        .as_i64()
        .expect("thin first custom b seq");
    assert_eq!(
        first_custom_b_payload["serializedArgs"],
        json!([{ "source": "thin-first-custom-b", "nested": { "count": 20, "values": ["first", 21, true] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36201,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("globalThis.__lm_custom_binding_b_deliver({{ name: 'customBindingB', seq: {first_custom_b_seq}, result: 'thin-first-custom-b-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36201)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36202,
        "method": "Runtime.evaluate",
        "sessionId": first_reauto_session,
        "params": {
            "contextId": first_replay_utility_context,
            "expression": "globalThis.__lm_thin_first_custom_b",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36202)["result"]["result"]["value"],
        json!("thin-first-custom-b-ok")
    );

    ctx.process_async(json!({
            "id": 36203,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "expression": "globalThis.__lm_thin_second_custom_a_reject = customBindingA({ source: 'thin-second-custom-a-reject', nested: { count: 22, values: ['second', 23, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-second-custom-a-reject'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_second_custom_a = take_response_by_id(&mut ctx, 36203);
    assert!(
        scheduled_second_custom_a["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin second custom a reject")
            .starts_with("scheduled-")
    );
    let second_custom_a_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reauto_session)
                && message["params"]["name"] == json!("customBindingA")
        })
        .cloned()
        .expect("thin second custom a bindingCalled");
    let second_custom_a_payload: serde_json::Value = serde_json::from_str(
        second_custom_a_called["params"]["payload"]
            .as_str()
            .expect("thin second custom a payload string"),
    )
    .expect("thin second custom a payload json");
    let second_custom_a_seq = second_custom_a_payload["seq"]
        .as_i64()
        .expect("thin second custom a seq");
    assert_eq!(
        second_custom_a_payload["serializedArgs"],
        json!([{ "source": "thin-second-custom-a-reject", "nested": { "count": 22, "values": ["second", 23, false] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36204,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "expression": format!("globalThis.__lm_custom_binding_a_deliver({{ name: 'customBindingA', seq: {second_custom_a_seq}, error: 'thin-second-custom-a-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36204)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36205,
        "method": "Runtime.evaluate",
        "sessionId": second_reauto_session,
        "params": {
            "expression": "globalThis.__lm_thin_second_custom_a_reject",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36205)["result"]["result"]["value"],
        json!("rejected:thin-second-custom-a-error")
    );

    for (id, session_id, target_id) in [
        (
            36206_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
        ),
        (
            36207_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": {
                "targetId": target_id,
                "sessionId": session_id,
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
        "id": 36208,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": first_replacement.target_id,
            "flatten": true
        }
    }))
    .await;
    let first_reattached_session = take_response_by_id(&mut ctx, 36208)["result"]["sessionId"]
        .as_str()
        .expect("thin name first reattached session id")
        .to_owned();
    assert_ne!(first_reattached_session, first_reauto_session);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": first_reattached_session,
            "targetInfo": {
                "targetId": first_replacement.target_id,
                "browserContextId": first.browser_context_id,
            }
        })),
    );
    ctx.take_all();

    ctx.process_async(json!({
        "id": 36209,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": second_replacement.target_id,
            "flatten": true
        }
    }))
    .await;
    let second_reattached_session = take_response_by_id(&mut ctx, 36209)["result"]["sessionId"]
        .as_str()
        .expect("thin name second reattached session id")
        .to_owned();
    assert_ne!(second_reattached_session, second_reauto_session);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": second_reattached_session,
            "targetInfo": {
                "targetId": second_replacement.target_id,
                "browserContextId": second.browser_context_id,
            }
        })),
    );
    ctx.take_all();

    for (id, session_id, context_id, expected_state) in [
        (
            36210_u64,
            first_reattached_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36211_u64,
            first_reattached_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36212_u64,
            second_reattached_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\",\"function\"]"),
        ),
        (
            36213_u64,
            second_reattached_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customBindingB, typeof globalThis.__pw_keptBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36214,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": "globalThis.__lm_thin_reattached_first_custom_b = customBindingB({ source: 'thin-reattached-first-custom-b', nested: { count: 24, values: ['reattach', 25, true] } }); 'scheduled-thin-reattached-first-custom-b'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_reattached_first_custom_b = take_response_by_id(&mut ctx, 36214);
    assert!(
        scheduled_reattached_first_custom_b["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin reattached first custom b")
            .starts_with("scheduled-")
    );
    let reattached_first_custom_b_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reattached_session)
                && message["params"]["name"] == json!("customBindingB")
                && message["params"]["executionContextId"] == json!(first_replay_utility_context)
        })
        .cloned()
        .expect("thin reattached first custom b bindingCalled");
    let reattached_first_custom_b_payload: serde_json::Value = serde_json::from_str(
        reattached_first_custom_b_called["params"]["payload"]
            .as_str()
            .expect("thin reattached first custom b payload string"),
    )
    .expect("thin reattached first custom b payload json");
    let reattached_first_custom_b_seq = reattached_first_custom_b_payload["seq"]
        .as_i64()
        .expect("thin reattached first custom b seq");
    assert_eq!(reattached_first_custom_b_seq, 2);
    assert_eq!(
        reattached_first_custom_b_payload["serializedArgs"],
        json!([{ "source": "thin-reattached-first-custom-b", "nested": { "count": 24, "values": ["reattach", 25, true] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36215,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("globalThis.__lm_custom_binding_b_deliver({{ name: 'customBindingB', seq: {reattached_first_custom_b_seq}, result: 'thin-reattached-first-custom-b-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36215)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36216,
        "method": "Runtime.evaluate",
        "sessionId": first_reattached_session,
        "params": {
            "contextId": first_replay_utility_context,
            "expression": "globalThis.__lm_thin_reattached_first_custom_b",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36216)["result"]["result"]["value"],
        json!("thin-reattached-first-custom-b-ok")
    );

    ctx.process_async(json!({
            "id": 36217,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "expression": "globalThis.__lm_thin_reattached_second_custom_a = customBindingA({ source: 'thin-reattached-second-custom-a', nested: { count: 26, values: ['reattach', 27, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-reattached-second-custom-a'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_reattached_second_custom_a = take_response_by_id(&mut ctx, 36217);
    assert!(
        scheduled_reattached_second_custom_a["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin reattached second custom a")
            .starts_with("scheduled-")
    );
    let reattached_second_custom_a_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reattached_session)
                && message["params"]["name"] == json!("customBindingA")
        })
        .cloned()
        .expect("thin reattached second custom a bindingCalled");
    let reattached_second_custom_a_payload: serde_json::Value = serde_json::from_str(
        reattached_second_custom_a_called["params"]["payload"]
            .as_str()
            .expect("thin reattached second custom a payload string"),
    )
    .expect("thin reattached second custom a payload json");
    let reattached_second_custom_a_seq = reattached_second_custom_a_payload["seq"]
        .as_i64()
        .expect("thin reattached second custom a seq");
    assert_eq!(reattached_second_custom_a_seq, 2);
    assert_eq!(
        reattached_second_custom_a_payload["serializedArgs"],
        json!([{ "source": "thin-reattached-second-custom-a", "nested": { "count": 26, "values": ["reattach", 27, false] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36218,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "expression": format!("globalThis.__lm_custom_binding_a_deliver({{ name: 'customBindingA', seq: {reattached_second_custom_a_seq}, error: 'thin-reattached-second-custom-a-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36218)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36219,
        "method": "Runtime.evaluate",
        "sessionId": second_reattached_session,
        "params": {
            "expression": "globalThis.__lm_thin_reattached_second_custom_a",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36219)["result"]["result"]["value"],
        json!("rejected:thin-reattached-second-custom-a-error")
    );
    })
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_replacement_targets_keep_thin_mixed_cleanup_isolated_per_browser_context_without_runtime_enable()
 {
    patchright_replacement_targets_large_stack(|| async {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36220, 36221, 36222)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36223, 36224, 36225)
            .await;

    for (id, session_id, html) in [
        (
            36226_u64,
            first.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-first-mixed</div></body>",
        ),
        (
            36227_u64,
            second.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-second-mixed</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36228_u64,
            first.session_id.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            36229_u64,
            second.session_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin mixed initial utility context id");
        ctx.take_all();
    }

    let custom_wrapper_source = patchright_page_binding_wrapper_source(
        "customBindingA",
        "__lm_custom_binding_a_deliver",
        None,
        false,
    );
    let custom_handle_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingA",
        "__lm_custom_handle_binding_a_deliver",
        Some("__lm_custom_handle_binding_a_take"),
        true,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptBinding",
        "__lm_pw_kept_binding_deliver",
        None,
        false,
    );
    let retained_handle_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptHandleBinding",
        "__lm_pw_kept_handle_binding_deliver",
        Some("__lm_pw_kept_handle_binding_take"),
        true,
    );

    for (session_id, utility_context_id, id_base) in [
        (first.session_id.as_str(), first_utility_context, 36230_u64),
        (
            second.session_id.as_str(),
            second_utility_context,
            36246_u64,
        ),
    ] {
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base,
            id_base + 1,
            id_base + 2,
            id_base + 3,
            "customBindingA",
            &custom_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 4,
            id_base + 5,
            id_base + 6,
            id_base + 7,
            "customHandleBindingA",
            &custom_handle_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 8,
            id_base + 9,
            id_base + 10,
            id_base + 11,
            "__pw_keptBinding",
            &retained_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 12,
            id_base + 13,
            id_base + 14,
            id_base + 15,
            "__pw_keptHandleBinding",
            &retained_handle_wrapper_source,
        )
        .await;
    }

    for (session_id, id_base) in [
        (first.session_id.as_str(), 36262_u64),
        (second.session_id.as_str(), 36270_u64),
    ] {
        for (source, world_name, offset) in [
            (custom_wrapper_source.as_str(), None, 0_u64),
            (custom_handle_wrapper_source.as_str(), None, 1_u64),
            (retained_wrapper_source.as_str(), None, 2_u64),
            (retained_handle_wrapper_source.as_str(), None, 3_u64),
            (custom_wrapper_source.as_str(), Some("utility"), 4_u64),
            (
                custom_handle_wrapper_source.as_str(),
                Some("utility"),
                5_u64,
            ),
            (retained_wrapper_source.as_str(), Some("utility"), 6_u64),
            (
                retained_handle_wrapper_source.as_str(),
                Some("utility"),
                7_u64,
            ),
        ] {
            let mut params = json!({
                "source": source,
                "runImmediately": true
            });
            if let Some(world_name) = world_name {
                params["worldName"] = json!(world_name);
            }
            ctx.process_async(json!({
                "id": id_base + offset,
                "method": "Page.addScriptToEvaluateOnNewDocument",
                "sessionId": session_id,
                "params": params
            }))
            .await;
            assert!(
                take_response_by_id(&mut ctx, id_base + offset)["result"]["identifier"]
                    .as_str()
                    .is_some()
            );
        }
    }

    for (id, name) in [
        (36278_u64, "customBindingA"),
        (36279_u64, "customHandleBindingA"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.removeBinding",
            "sessionId": first.session_id,
            "params": { "name": name }
        }))
        .await;
        assert_eq!(take_response_by_id(&mut ctx, id)["result"], json!({}));
    }

    for (id, target_id) in [
        (36280_u64, first.target_id.as_str()),
        (36281_u64, second.target_id.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.closeTarget",
            "params": { "targetId": target_id }
        }))
        .await;
        ctx.expect_result(id, json!({ "success": true }), None);
        ctx.take_all();
    }

    let first_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &first.browser_context_id,
        36282,
        36283,
    )
    .await;
    let second_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &second.browser_context_id,
        36284,
        36285,
    )
    .await;

    for (id, session_id, html) in [
        (
            36286_u64,
            first_replacement.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-first-mixed-replacement</div></body>",
        ),
        (
            36287_u64,
            second_replacement.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-second-mixed-replacement</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            36288_u64,
            first_replacement.target_id.as_str(),
            first_replacement.session_id.as_str(),
        ),
        (
            36289_u64,
            second_replacement.target_id.as_str(),
            second_replacement.session_id.as_str(),
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
        "id": 36290,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(36290, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(attached_events.len(), 2);
    let first_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(first_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin mixed first replacement re-auto-attached session")
        .to_owned();
    let second_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(second_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin mixed second replacement re-auto-attached session")
        .to_owned();

    for (id, session_id, source, expected_type) in [
        (
            36291_u64,
            first_reauto_session.as_str(),
            custom_wrapper_source.as_str(),
            "undefined",
        ),
        (
            36292_u64,
            first_reauto_session.as_str(),
            custom_handle_wrapper_source.as_str(),
            "undefined",
        ),
        (
            36293_u64,
            first_reauto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            36294_u64,
            first_reauto_session.as_str(),
            retained_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            36295_u64,
            second_reauto_session.as_str(),
            custom_wrapper_source.as_str(),
            "function",
        ),
        (
            36296_u64,
            second_reauto_session.as_str(),
            custom_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            36297_u64,
            second_reauto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            36298_u64,
            second_reauto_session.as_str(),
            retained_handle_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "expression": source,
                "awaitPromise": true
            }
        }))
        .await;
        let installed = take_response_by_id(&mut ctx, id);
        assert_eq!(installed["result"]["result"]["value"], json!(expected_type));
    }

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36299_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            36300_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin mixed replay utility context id");
        ctx.take_all();
    }

    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36301,
        36302,
        36303,
        36304,
        "__pw_keptBinding",
        &retained_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36305,
        36306,
        36307,
        36308,
        "__pw_keptHandleBinding",
        &retained_handle_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36309,
        36310,
        36311,
        36312,
        "customBindingA",
        &custom_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36313,
        36314,
        36315,
        36316,
        "customHandleBindingA",
        &custom_handle_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36317,
        36318,
        36319,
        36320,
        "__pw_keptBinding",
        &retained_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36321,
        36322,
        36323,
        36324,
        "__pw_keptHandleBinding",
        &retained_handle_wrapper_source,
    )
    .await;

    for (id, session_id, context_id, expected_state) in [
        (
            36325_u64,
            first_reauto_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36326_u64,
            first_reauto_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36327_u64,
            second_reauto_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\",\"function\",\"function\"]"),
        ),
        (
            36328_u64,
            second_reauto_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\",\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customHandleBindingA, typeof globalThis.__pw_keptBinding, typeof globalThis.__pw_keptHandleBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36329,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "expression": "globalThis.__lm_thin_mixed_second_custom = customBindingA({ source: 'thin-mixed-second-custom', nested: { count: 28, values: ['mixed', 29, true] } }); 'scheduled-thin-mixed-second-custom'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_second_custom = take_response_by_id(&mut ctx, 36329);
    assert!(
        scheduled_second_custom["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed second custom")
            .starts_with("scheduled-")
    );
    let second_custom_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reauto_session)
                && message["params"]["name"] == json!("customBindingA")
        })
        .cloned()
        .expect("thin mixed second custom bindingCalled");
    let second_custom_payload: serde_json::Value = serde_json::from_str(
        second_custom_called["params"]["payload"]
            .as_str()
            .expect("thin mixed second custom payload string"),
    )
    .expect("thin mixed second custom payload json");
    let second_custom_seq = second_custom_payload["seq"]
        .as_i64()
        .expect("thin mixed second custom seq");
    assert_eq!(
        second_custom_payload["serializedArgs"],
        json!([{ "source": "thin-mixed-second-custom", "nested": { "count": 28, "values": ["mixed", 29, true] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36330,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "expression": format!("globalThis.__lm_custom_binding_a_deliver({{ name: 'customBindingA', seq: {second_custom_seq}, result: 'thin-mixed-second-custom-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36330)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36331,
        "method": "Runtime.evaluate",
        "sessionId": second_reauto_session,
        "params": {
            "expression": "globalThis.__lm_thin_mixed_second_custom",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36331)["result"]["result"]["value"],
        json!("thin-mixed-second-custom-ok")
    );

    ctx.process_async(json!({
            "id": 36332,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": "globalThis.__lm_thin_mixed_second_custom_handle = customHandleBindingA(document.getElementById('utility-handle-a')); 'scheduled-thin-mixed-second-custom-handle'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_second_handle = take_response_by_id(&mut ctx, 36332);
    assert!(
        scheduled_second_handle["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed second handle")
            .starts_with("scheduled-")
    );
    let second_handle_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reauto_session)
                && message["params"]["name"] == json!("customHandleBindingA")
                && message["params"]["executionContextId"] == json!(second_replay_utility_context)
        })
        .cloned()
        .expect("thin mixed second handle bindingCalled");
    let second_handle_payload: serde_json::Value = serde_json::from_str(
        second_handle_called["params"]["payload"]
            .as_str()
            .expect("thin mixed second handle payload string"),
    )
    .expect("thin mixed second handle payload json");
    let second_handle_seq = second_handle_payload["seq"]
        .as_i64()
        .expect("thin mixed second handle seq");
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36333,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": format!("(() => {{ const handle = globalThis.__lm_custom_handle_binding_a_take({{ name: 'customHandleBindingA', seq: {second_handle_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_custom_handle_binding_a_take({{ name: 'customHandleBindingA', seq: {second_handle_seq} }})]); }})()")
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36333)["result"]["result"]["value"],
        json!("[\"utility-handle-a\",\"thin-second-mixed-replacement\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 36334,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": format!("globalThis.__lm_custom_handle_binding_a_deliver({{ name: 'customHandleBindingA', seq: {second_handle_seq}, result: 'thin-mixed-second-custom-handle-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36334)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36335,
        "method": "Runtime.evaluate",
        "sessionId": second_reauto_session,
        "params": {
            "contextId": second_replay_utility_context,
            "expression": "globalThis.__lm_thin_mixed_second_custom_handle",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36335)["result"]["result"]["value"],
        json!("thin-mixed-second-custom-handle-ok")
    );

    ctx.process_async(json!({
            "id": 36336,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "expression": "globalThis.__lm_thin_mixed_first_pw = __pw_keptBinding({ source: 'thin-mixed-first-pw', nested: { count: 30, values: ['mixed', 31, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-mixed-first-pw'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_first_pw = take_response_by_id(&mut ctx, 36336);
    assert!(
        scheduled_first_pw["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed first pw")
            .starts_with("scheduled-")
    );
    let first_pw_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reauto_session)
                && message["params"]["name"] == json!("__pw_keptBinding")
        })
        .cloned()
        .expect("thin mixed first pw bindingCalled");
    let first_pw_payload: serde_json::Value = serde_json::from_str(
        first_pw_called["params"]["payload"]
            .as_str()
            .expect("thin mixed first pw payload string"),
    )
    .expect("thin mixed first pw payload json");
    let first_pw_seq = first_pw_payload["seq"]
        .as_i64()
        .expect("thin mixed first pw seq");
    assert_eq!(
        first_pw_payload["serializedArgs"],
        json!([{ "source": "thin-mixed-first-pw", "nested": { "count": 30, "values": ["mixed", 31, false] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36337,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "expression": format!("globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {first_pw_seq}, error: 'thin-mixed-first-pw-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36337)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36338,
        "method": "Runtime.evaluate",
        "sessionId": first_reauto_session,
        "params": {
            "expression": "globalThis.__lm_thin_mixed_first_pw",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36338)["result"]["result"]["value"],
        json!("rejected:thin-mixed-first-pw-error")
    );

    ctx.process_async(json!({
            "id": 36339,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": "globalThis.__lm_thin_mixed_first_pw_handle = __pw_keptHandleBinding(document.getElementById('utility-handle-a')); 'scheduled-thin-mixed-first-pw-handle'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_first_pw_handle = take_response_by_id(&mut ctx, 36339);
    assert!(
        scheduled_first_pw_handle["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed first pw handle")
            .starts_with("scheduled-")
    );
    let first_pw_handle_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reauto_session)
                && message["params"]["name"] == json!("__pw_keptHandleBinding")
                && message["params"]["executionContextId"] == json!(first_replay_utility_context)
        })
        .cloned()
        .expect("thin mixed first pw handle bindingCalled");
    let first_pw_handle_payload: serde_json::Value = serde_json::from_str(
        first_pw_handle_called["params"]["payload"]
            .as_str()
            .expect("thin mixed first pw handle payload string"),
    )
    .expect("thin mixed first pw handle payload json");
    let first_pw_handle_seq = first_pw_handle_payload["seq"]
        .as_i64()
        .expect("thin mixed first pw handle seq");
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36340,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("(() => {{ const handle = globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {first_pw_handle_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {first_pw_handle_seq} }})]); }})()")
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36340)["result"]["result"]["value"],
        json!("[\"utility-handle-a\",\"thin-first-mixed-replacement\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 36341,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("globalThis.__lm_pw_kept_handle_binding_deliver({{ name: '__pw_keptHandleBinding', seq: {first_pw_handle_seq}, result: 'thin-mixed-first-pw-handle-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36341)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36342,
        "method": "Runtime.evaluate",
        "sessionId": first_reauto_session,
        "params": {
            "contextId": first_replay_utility_context,
            "expression": "globalThis.__lm_thin_mixed_first_pw_handle",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36342)["result"]["result"]["value"],
        json!("thin-mixed-first-pw-handle-ok")
    );

    for (id, session_id, target_id) in [
        (
            36343_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
        ),
        (
            36344_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": {
                "targetId": target_id,
                "sessionId": session_id,
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
        "id": 36345,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": first_replacement.target_id,
            "flatten": true
        }
    }))
    .await;
    let first_reattached_session = take_response_by_id(&mut ctx, 36345)["result"]["sessionId"]
        .as_str()
        .expect("thin mixed first reattached session id")
        .to_owned();
    assert_ne!(first_reattached_session, first_reauto_session);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": first_reattached_session,
            "targetInfo": {
                "targetId": first_replacement.target_id,
                "browserContextId": first.browser_context_id,
            }
        })),
    );
    ctx.take_all();

    ctx.process_async(json!({
        "id": 36346,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": second_replacement.target_id,
            "flatten": true
        }
    }))
    .await;
    let second_reattached_session = take_response_by_id(&mut ctx, 36346)["result"]["sessionId"]
        .as_str()
        .expect("thin mixed second reattached session id")
        .to_owned();
    assert_ne!(second_reattached_session, second_reauto_session);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": second_reattached_session,
            "targetInfo": {
                "targetId": second_replacement.target_id,
                "browserContextId": second.browser_context_id,
            }
        })),
    );
    ctx.take_all();

    for (id, session_id, context_id, expected_state) in [
        (
            36347_u64,
            first_reattached_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36348_u64,
            first_reattached_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36349_u64,
            second_reattached_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\",\"function\",\"function\"]"),
        ),
        (
            36350_u64,
            second_reattached_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\",\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customHandleBindingA, typeof globalThis.__pw_keptBinding, typeof globalThis.__pw_keptHandleBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36351,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": "globalThis.__lm_thin_mixed_reattached_second_custom_handle = customHandleBindingA(document.getElementById('utility-handle-a')); 'scheduled-thin-mixed-reattached-second-custom-handle'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_reattached_second_handle = take_response_by_id(&mut ctx, 36351);
    assert!(
        scheduled_reattached_second_handle["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed reattached second handle")
            .starts_with("scheduled-")
    );
    let reattached_second_handle_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reattached_session)
                && message["params"]["name"] == json!("customHandleBindingA")
                && message["params"]["executionContextId"] == json!(second_replay_utility_context)
        })
        .cloned()
        .expect("thin mixed reattached second handle bindingCalled");
    let reattached_second_handle_payload: serde_json::Value = serde_json::from_str(
        reattached_second_handle_called["params"]["payload"]
            .as_str()
            .expect("thin mixed reattached second handle payload string"),
    )
    .expect("thin mixed reattached second handle payload json");
    let reattached_second_handle_seq = reattached_second_handle_payload["seq"]
        .as_i64()
        .expect("thin mixed reattached second handle seq");
    assert_eq!(reattached_second_handle_seq, 2);
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36352,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": format!("(() => {{ const handle = globalThis.__lm_custom_handle_binding_a_take({{ name: 'customHandleBindingA', seq: {reattached_second_handle_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_custom_handle_binding_a_take({{ name: 'customHandleBindingA', seq: {reattached_second_handle_seq} }})]); }})()")
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36352)["result"]["result"]["value"],
        json!("[\"utility-handle-a\",\"thin-second-mixed-replacement\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 36353,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "contextId": second_replay_utility_context,
                "expression": format!("globalThis.__lm_custom_handle_binding_a_deliver({{ name: 'customHandleBindingA', seq: {reattached_second_handle_seq}, result: 'thin-mixed-reattached-second-custom-handle-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36353)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36354,
        "method": "Runtime.evaluate",
        "sessionId": second_reattached_session,
        "params": {
            "contextId": second_replay_utility_context,
            "expression": "globalThis.__lm_thin_mixed_reattached_second_custom_handle",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36354)["result"]["result"]["value"],
        json!("thin-mixed-reattached-second-custom-handle-ok")
    );

    ctx.process_async(json!({
            "id": 36355,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "expression": "globalThis.__lm_thin_mixed_reattached_first_pw = __pw_keptBinding({ source: 'thin-mixed-reattached-first-pw', nested: { count: 32, values: ['reattach', 33, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-mixed-reattached-first-pw'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_reattached_first_pw = take_response_by_id(&mut ctx, 36355);
    assert!(
        scheduled_reattached_first_pw["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed reattached first pw")
            .starts_with("scheduled-")
    );
    let reattached_first_pw_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reattached_session)
                && message["params"]["name"] == json!("__pw_keptBinding")
        })
        .cloned()
        .expect("thin mixed reattached first pw bindingCalled");
    let reattached_first_pw_payload: serde_json::Value = serde_json::from_str(
        reattached_first_pw_called["params"]["payload"]
            .as_str()
            .expect("thin mixed reattached first pw payload string"),
    )
    .expect("thin mixed reattached first pw payload json");
    let reattached_first_pw_seq = reattached_first_pw_payload["seq"]
        .as_i64()
        .expect("thin mixed reattached first pw seq");
    assert_eq!(reattached_first_pw_seq, 2);
    assert_eq!(
        reattached_first_pw_payload["serializedArgs"],
        json!([{ "source": "thin-mixed-reattached-first-pw", "nested": { "count": 32, "values": ["reattach", 33, false] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36356,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "expression": format!("globalThis.__lm_pw_kept_binding_deliver({{ name: '__pw_keptBinding', seq: {reattached_first_pw_seq}, error: 'thin-mixed-reattached-first-pw-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36356)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36357,
        "method": "Runtime.evaluate",
        "sessionId": first_reattached_session,
        "params": {
            "expression": "globalThis.__lm_thin_mixed_reattached_first_pw",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36357)["result"]["result"]["value"],
        json!("rejected:thin-mixed-reattached-first-pw-error")
    );
    })
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_replacement_targets_keep_thin_mixed_name_cleanup_isolated_per_browser_context_without_runtime_enable()
 {
    patchright_replacement_targets_large_stack(|| async {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36358, 36359, 36360)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36361, 36362, 36363)
            .await;

    for (id, session_id, html) in [
        (
            36364_u64,
            first.session_id.as_str(),
            "<body><div id='utility-handle-b'>thin-first-mixed-name</div></body>",
        ),
        (
            36365_u64,
            second.session_id.as_str(),
            "<body><div id='utility-handle-b'>thin-second-mixed-name</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36366_u64,
            first.session_id.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            36367_u64,
            second.session_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin mixed-name initial utility context id");
        ctx.take_all();
    }

    let custom_a_wrapper_source = patchright_page_binding_wrapper_source(
        "customBindingA",
        "__lm_custom_binding_a_deliver",
        None,
        false,
    );
    let custom_b_wrapper_source = patchright_page_binding_wrapper_source(
        "customBindingB",
        "__lm_custom_binding_b_deliver",
        None,
        false,
    );
    let custom_handle_a_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingA",
        "__lm_custom_handle_binding_a_deliver",
        Some("__lm_custom_handle_binding_a_take"),
        true,
    );
    let custom_handle_b_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingB",
        "__lm_custom_handle_binding_b_deliver",
        Some("__lm_custom_handle_binding_b_take"),
        true,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptBinding",
        "__lm_pw_kept_binding_deliver",
        None,
        false,
    );

    for (session_id, utility_context_id, id_base) in [
        (first.session_id.as_str(), first_utility_context, 36368_u64),
        (
            second.session_id.as_str(),
            second_utility_context,
            36388_u64,
        ),
    ] {
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base,
            id_base + 1,
            id_base + 2,
            id_base + 3,
            "customBindingA",
            &custom_a_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 4,
            id_base + 5,
            id_base + 6,
            id_base + 7,
            "customBindingB",
            &custom_b_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 8,
            id_base + 9,
            id_base + 10,
            id_base + 11,
            "customHandleBindingA",
            &custom_handle_a_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 12,
            id_base + 13,
            id_base + 14,
            id_base + 15,
            "customHandleBindingB",
            &custom_handle_b_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 16,
            id_base + 17,
            id_base + 18,
            id_base + 19,
            "__pw_keptBinding",
            &retained_wrapper_source,
        )
        .await;
    }

    for (session_id, id_base) in [
        (first.session_id.as_str(), 36408_u64),
        (second.session_id.as_str(), 36418_u64),
    ] {
        for (source, world_name, offset) in [
            (custom_a_wrapper_source.as_str(), None, 0_u64),
            (custom_b_wrapper_source.as_str(), None, 1_u64),
            (custom_handle_a_wrapper_source.as_str(), None, 2_u64),
            (custom_handle_b_wrapper_source.as_str(), None, 3_u64),
            (retained_wrapper_source.as_str(), None, 4_u64),
            (custom_a_wrapper_source.as_str(), Some("utility"), 5_u64),
            (custom_b_wrapper_source.as_str(), Some("utility"), 6_u64),
            (
                custom_handle_a_wrapper_source.as_str(),
                Some("utility"),
                7_u64,
            ),
            (
                custom_handle_b_wrapper_source.as_str(),
                Some("utility"),
                8_u64,
            ),
            (retained_wrapper_source.as_str(), Some("utility"), 9_u64),
        ] {
            let mut params = json!({
                "source": source,
                "runImmediately": true
            });
            if let Some(world_name) = world_name {
                params["worldName"] = json!(world_name);
            }
            ctx.process_async(json!({
                "id": id_base + offset,
                "method": "Page.addScriptToEvaluateOnNewDocument",
                "sessionId": session_id,
                "params": params
            }))
            .await;
            assert!(
                take_response_by_id(&mut ctx, id_base + offset)["result"]["identifier"]
                    .as_str()
                    .is_some()
            );
        }
    }

    for (id, name) in [
        (36428_u64, "customBindingA"),
        (36429_u64, "customHandleBindingA"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.removeBinding",
            "sessionId": first.session_id,
            "params": { "name": name }
        }))
        .await;
        assert_eq!(take_response_by_id(&mut ctx, id)["result"], json!({}));
    }

    for (id, target_id) in [
        (36430_u64, first.target_id.as_str()),
        (36431_u64, second.target_id.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.closeTarget",
            "params": { "targetId": target_id }
        }))
        .await;
        ctx.expect_result(id, json!({ "success": true }), None);
        ctx.take_all();
    }

    let first_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &first.browser_context_id,
        36432,
        36433,
    )
    .await;
    let second_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &second.browser_context_id,
        36434,
        36435,
    )
    .await;

    for (id, session_id, html) in [
        (
            36436_u64,
            first_replacement.session_id.as_str(),
            "<body><div id='utility-handle-b'>thin-first-mixed-name-replacement</div></body>",
        ),
        (
            36437_u64,
            second_replacement.session_id.as_str(),
            "<body><div id='utility-handle-b'>thin-second-mixed-name-replacement</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            36438_u64,
            first_replacement.target_id.as_str(),
            first_replacement.session_id.as_str(),
        ),
        (
            36439_u64,
            second_replacement.target_id.as_str(),
            second_replacement.session_id.as_str(),
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
        "id": 36440,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(36440, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(attached_events.len(), 2);
    let first_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(first_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin mixed-name first replacement re-auto-attached session")
        .to_owned();
    let second_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(second_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin mixed-name second replacement re-auto-attached session")
        .to_owned();

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36441_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            36442_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin mixed-name replay utility context id");
        ctx.take_all();
    }

    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36443,
        36444,
        36445,
        36446,
        "customBindingB",
        &custom_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36447,
        36448,
        36449,
        36450,
        "customHandleBindingB",
        &custom_handle_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36451,
        36452,
        36453,
        36454,
        "__pw_keptBinding",
        &retained_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36455,
        36456,
        36457,
        36458,
        "customBindingA",
        &custom_a_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36459,
        36460,
        36461,
        36462,
        "customBindingB",
        &custom_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36463,
        36464,
        36465,
        36466,
        "customHandleBindingA",
        &custom_handle_a_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36467,
        36468,
        36469,
        36470,
        "customHandleBindingB",
        &custom_handle_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36471,
        36472,
        36473,
        36474,
        "__pw_keptBinding",
        &retained_wrapper_source,
    )
    .await;

    for (id, session_id, context_id, expected_state) in [
        (
            36475_u64,
            first_reauto_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"function\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36476_u64,
            first_reauto_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"function\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36477_u64,
            second_reauto_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\",\"function\",\"function\",\"function\"]"),
        ),
        (
            36478_u64,
            second_reauto_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\",\"function\",\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customBindingB, typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36479,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": "globalThis.__lm_thin_mixed_name_first_custom_handle_b = customHandleBindingB(document.getElementById('utility-handle-b')); 'scheduled-thin-mixed-name-first-custom-handle-b'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_first_handle_b = take_response_by_id(&mut ctx, 36479);
    assert!(
        scheduled_first_handle_b["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed-name first custom handle b")
            .starts_with("scheduled-")
    );
    let first_handle_b_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reauto_session)
                && message["params"]["name"] == json!("customHandleBindingB")
                && message["params"]["executionContextId"] == json!(first_replay_utility_context)
        })
        .cloned()
        .expect("thin mixed-name first custom handle b bindingCalled");
    let first_handle_b_payload: serde_json::Value = serde_json::from_str(
        first_handle_b_called["params"]["payload"]
            .as_str()
            .expect("thin mixed-name first custom handle b payload string"),
    )
    .expect("thin mixed-name first custom handle b payload json");
    let first_handle_b_seq = first_handle_b_payload["seq"]
        .as_i64()
        .expect("thin mixed-name first custom handle b seq");
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36480,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("(() => {{ const handle = globalThis.__lm_custom_handle_binding_b_take({{ name: 'customHandleBindingB', seq: {first_handle_b_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_custom_handle_binding_b_take({{ name: 'customHandleBindingB', seq: {first_handle_b_seq} }})]); }})()")
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36480)["result"]["result"]["value"],
        json!("[\"utility-handle-b\",\"thin-first-mixed-name-replacement\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 36481,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("globalThis.__lm_custom_handle_binding_b_deliver({{ name: 'customHandleBindingB', seq: {first_handle_b_seq}, result: 'thin-mixed-name-first-custom-handle-b-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36481)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36482,
        "method": "Runtime.evaluate",
        "sessionId": first_reauto_session,
        "params": {
            "contextId": first_replay_utility_context,
            "expression": "globalThis.__lm_thin_mixed_name_first_custom_handle_b",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36482)["result"]["result"]["value"],
        json!("thin-mixed-name-first-custom-handle-b-ok")
    );

    ctx.process_async(json!({
            "id": 36483,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "expression": "globalThis.__lm_thin_mixed_name_second_custom_a = customBindingA({ source: 'thin-mixed-name-second-custom-a', nested: { count: 34, values: ['mixed-name', 35, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-mixed-name-second-custom-a'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_second_custom_a = take_response_by_id(&mut ctx, 36483);
    assert!(
        scheduled_second_custom_a["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed-name second custom a")
            .starts_with("scheduled-")
    );
    let second_custom_a_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reauto_session)
                && message["params"]["name"] == json!("customBindingA")
        })
        .cloned()
        .expect("thin mixed-name second custom a bindingCalled");
    let second_custom_a_payload: serde_json::Value = serde_json::from_str(
        second_custom_a_called["params"]["payload"]
            .as_str()
            .expect("thin mixed-name second custom a payload string"),
    )
    .expect("thin mixed-name second custom a payload json");
    let second_custom_a_seq = second_custom_a_payload["seq"]
        .as_i64()
        .expect("thin mixed-name second custom a seq");
    assert_eq!(
        second_custom_a_payload["serializedArgs"],
        json!([{ "source": "thin-mixed-name-second-custom-a", "nested": { "count": 34, "values": ["mixed-name", 35, false] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36484,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "expression": format!("globalThis.__lm_custom_binding_a_deliver({{ name: 'customBindingA', seq: {second_custom_a_seq}, error: 'thin-mixed-name-second-custom-a-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36484)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36485,
        "method": "Runtime.evaluate",
        "sessionId": second_reauto_session,
        "params": {
            "expression": "globalThis.__lm_thin_mixed_name_second_custom_a",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36485)["result"]["result"]["value"],
        json!("rejected:thin-mixed-name-second-custom-a-error")
    );

    for (id, session_id, target_id) in [
        (
            36486_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
        ),
        (
            36487_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": {
                "targetId": target_id,
                "sessionId": session_id,
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
        "id": 36488,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": first_replacement.target_id,
            "flatten": true
        }
    }))
    .await;
    let first_reattached_session = take_response_by_id(&mut ctx, 36488)["result"]["sessionId"]
        .as_str()
        .expect("thin mixed-name first reattached session id")
        .to_owned();
    assert_ne!(first_reattached_session, first_reauto_session);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": first_reattached_session,
            "targetInfo": {
                "targetId": first_replacement.target_id,
                "browserContextId": first.browser_context_id,
            }
        })),
    );
    ctx.take_all();

    ctx.process_async(json!({
        "id": 36489,
        "method": "Target.attachToTarget",
        "params": {
            "targetId": second_replacement.target_id,
            "flatten": true
        }
    }))
    .await;
    let second_reattached_session = take_response_by_id(&mut ctx, 36489)["result"]["sessionId"]
        .as_str()
        .expect("thin mixed-name second reattached session id")
        .to_owned();
    assert_ne!(second_reattached_session, second_reauto_session);
    ctx.expect_event(
        "Target.attachedToTarget",
        Some(&json!({
            "sessionId": second_reattached_session,
            "targetInfo": {
                "targetId": second_replacement.target_id,
                "browserContextId": second.browser_context_id,
            }
        })),
    );
    ctx.take_all();

    for (id, session_id, context_id, expected_state) in [
        (
            36490_u64,
            first_reattached_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"function\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36491_u64,
            first_reattached_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"function\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36492_u64,
            second_reattached_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\",\"function\",\"function\",\"function\"]"),
        ),
        (
            36493_u64,
            second_reattached_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\",\"function\",\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customBindingB, typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36494,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": "globalThis.__lm_thin_mixed_name_reattached_first_custom_handle_b = customHandleBindingB(document.getElementById('utility-handle-b')); 'scheduled-thin-mixed-name-reattached-first-custom-handle-b'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_reattached_first_handle_b = take_response_by_id(&mut ctx, 36494);
    assert!(
        scheduled_reattached_first_handle_b["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed-name reattached first custom handle b")
            .starts_with("scheduled-")
    );
    let reattached_first_handle_b_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reattached_session)
                && message["params"]["name"] == json!("customHandleBindingB")
                && message["params"]["executionContextId"] == json!(first_replay_utility_context)
        })
        .cloned()
        .expect("thin mixed-name reattached first custom handle b bindingCalled");
    let reattached_first_handle_b_payload: serde_json::Value = serde_json::from_str(
        reattached_first_handle_b_called["params"]["payload"]
            .as_str()
            .expect("thin mixed-name reattached first custom handle b payload string"),
    )
    .expect("thin mixed-name reattached first custom handle b payload json");
    let reattached_first_handle_b_seq = reattached_first_handle_b_payload["seq"]
        .as_i64()
        .expect("thin mixed-name reattached first custom handle b seq");
    assert_eq!(reattached_first_handle_b_seq, 2);
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36495,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("(() => {{ const handle = globalThis.__lm_custom_handle_binding_b_take({{ name: 'customHandleBindingB', seq: {reattached_first_handle_b_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_custom_handle_binding_b_take({{ name: 'customHandleBindingB', seq: {reattached_first_handle_b_seq} }})]); }})()")
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36495)["result"]["result"]["value"],
        json!("[\"utility-handle-b\",\"thin-first-mixed-name-replacement\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 36496,
            "method": "Runtime.evaluate",
            "sessionId": first_reattached_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("globalThis.__lm_custom_handle_binding_b_deliver({{ name: 'customHandleBindingB', seq: {reattached_first_handle_b_seq}, result: 'thin-mixed-name-reattached-first-custom-handle-b-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36496)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36497,
        "method": "Runtime.evaluate",
        "sessionId": first_reattached_session,
        "params": {
            "contextId": first_replay_utility_context,
            "expression": "globalThis.__lm_thin_mixed_name_reattached_first_custom_handle_b",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36497)["result"]["result"]["value"],
        json!("thin-mixed-name-reattached-first-custom-handle-b-ok")
    );

    ctx.process_async(json!({
            "id": 36498,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "expression": "globalThis.__lm_thin_mixed_name_reattached_second_custom_a = customBindingA({ source: 'thin-mixed-name-reattached-second-custom-a', nested: { count: 36, values: ['reattach', 37, true] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-mixed-name-reattached-second-custom-a'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_reattached_second_custom_a = take_response_by_id(&mut ctx, 36498);
    assert!(
        scheduled_reattached_second_custom_a["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed-name reattached second custom a")
            .starts_with("scheduled-")
    );
    let reattached_second_custom_a_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reattached_session)
                && message["params"]["name"] == json!("customBindingA")
        })
        .cloned()
        .expect("thin mixed-name reattached second custom a bindingCalled");
    let reattached_second_custom_a_payload: serde_json::Value = serde_json::from_str(
        reattached_second_custom_a_called["params"]["payload"]
            .as_str()
            .expect("thin mixed-name reattached second custom a payload string"),
    )
    .expect("thin mixed-name reattached second custom a payload json");
    let reattached_second_custom_a_seq = reattached_second_custom_a_payload["seq"]
        .as_i64()
        .expect("thin mixed-name reattached second custom a seq");
    assert_eq!(reattached_second_custom_a_seq, 2);
    assert_eq!(
        reattached_second_custom_a_payload["serializedArgs"],
        json!([{ "source": "thin-mixed-name-reattached-second-custom-a", "nested": { "count": 36, "values": ["reattach", 37, true] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36499,
            "method": "Runtime.evaluate",
            "sessionId": second_reattached_session,
            "params": {
                "expression": format!("globalThis.__lm_custom_binding_a_deliver({{ name: 'customBindingA', seq: {reattached_second_custom_a_seq}, error: 'thin-mixed-name-reattached-second-custom-a-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36499)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36500,
        "method": "Runtime.evaluate",
        "sessionId": second_reattached_session,
        "params": {
            "expression": "globalThis.__lm_thin_mixed_name_reattached_second_custom_a",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36500)["result"]["result"]["value"],
        json!("rejected:thin-mixed-name-reattached-second-custom-a-error")
    );
    })
    .await;
}
#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_replacement_targets_keep_thin_mixed_name_handle_cleanup_isolated_per_browser_context_without_runtime_enable()
 {
    patchright_replacement_targets_large_stack(|| async {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36501, 36502, 36503)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 36504, 36505, 36506)
            .await;

    for (id, session_id, html) in [
        (
            36507_u64,
            first.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-first-mixed-name-handle</div></body>",
        ),
        (
            36508_u64,
            second.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-second-mixed-name-handle</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    let mut first_utility_context = 0_i64;
    let mut second_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36509_u64,
            first.session_id.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            36510_u64,
            second.session_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin mixed-name handle initial utility context id");
        ctx.take_all();
    }

    let custom_a_wrapper_source = patchright_page_binding_wrapper_source(
        "customBindingA",
        "__lm_custom_binding_a_deliver",
        None,
        false,
    );
    let custom_b_wrapper_source = patchright_page_binding_wrapper_source(
        "customBindingB",
        "__lm_custom_binding_b_deliver",
        None,
        false,
    );
    let custom_handle_a_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingA",
        "__lm_custom_handle_binding_a_deliver",
        Some("__lm_custom_handle_binding_a_take"),
        true,
    );
    let custom_handle_b_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingB",
        "__lm_custom_handle_binding_b_deliver",
        Some("__lm_custom_handle_binding_b_take"),
        true,
    );
    let retained_handle_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptHandleBinding",
        "__lm_pw_kept_handle_binding_deliver",
        Some("__lm_pw_kept_handle_binding_take"),
        true,
    );

    for (session_id, utility_context_id, id_base) in [
        (first.session_id.as_str(), first_utility_context, 36511_u64),
        (
            second.session_id.as_str(),
            second_utility_context,
            36531_u64,
        ),
    ] {
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base,
            id_base + 1,
            id_base + 2,
            id_base + 3,
            "customBindingA",
            &custom_a_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 4,
            id_base + 5,
            id_base + 6,
            id_base + 7,
            "customBindingB",
            &custom_b_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 8,
            id_base + 9,
            id_base + 10,
            id_base + 11,
            "customHandleBindingA",
            &custom_handle_a_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 12,
            id_base + 13,
            id_base + 14,
            id_base + 15,
            "customHandleBindingB",
            &custom_handle_b_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context_id,
            id_base + 16,
            id_base + 17,
            id_base + 18,
            id_base + 19,
            "__pw_keptHandleBinding",
            &retained_handle_wrapper_source,
        )
        .await;
    }

    for (session_id, id_base) in [
        (first.session_id.as_str(), 36551_u64),
        (second.session_id.as_str(), 36561_u64),
    ] {
        for (source, world_name, offset) in [
            (custom_a_wrapper_source.as_str(), None, 0_u64),
            (custom_b_wrapper_source.as_str(), None, 1_u64),
            (custom_handle_a_wrapper_source.as_str(), None, 2_u64),
            (custom_handle_b_wrapper_source.as_str(), None, 3_u64),
            (retained_handle_wrapper_source.as_str(), None, 4_u64),
            (custom_a_wrapper_source.as_str(), Some("utility"), 5_u64),
            (custom_b_wrapper_source.as_str(), Some("utility"), 6_u64),
            (
                custom_handle_a_wrapper_source.as_str(),
                Some("utility"),
                7_u64,
            ),
            (
                custom_handle_b_wrapper_source.as_str(),
                Some("utility"),
                8_u64,
            ),
            (
                retained_handle_wrapper_source.as_str(),
                Some("utility"),
                9_u64,
            ),
        ] {
            let mut params = json!({
                "source": source,
                "runImmediately": true
            });
            if let Some(world_name) = world_name {
                params["worldName"] = json!(world_name);
            }
            ctx.process_async(json!({
                "id": id_base + offset,
                "method": "Page.addScriptToEvaluateOnNewDocument",
                "sessionId": session_id,
                "params": params
            }))
            .await;
            assert!(
                take_response_by_id(&mut ctx, id_base + offset)["result"]["identifier"]
                    .as_str()
                    .is_some()
            );
        }
    }

    for (id, name) in [
        (36571_u64, "customBindingA"),
        (36572_u64, "customHandleBindingA"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.removeBinding",
            "sessionId": first.session_id,
            "params": { "name": name }
        }))
        .await;
        assert_eq!(take_response_by_id(&mut ctx, id)["result"], json!({}));
    }

    for (id, target_id) in [
        (36573_u64, first.target_id.as_str()),
        (36574_u64, second.target_id.as_str()),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.closeTarget",
            "params": { "targetId": target_id }
        }))
        .await;
        ctx.expect_result(id, json!({ "success": true }), None);
        ctx.take_all();
    }

    let first_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &first.browser_context_id,
        36575,
        36576,
    )
    .await;
    let second_replacement = attach_page_session_without_runtime_enable_in_existing_context_async(
        &mut ctx,
        &second.browser_context_id,
        36577,
        36578,
    )
    .await;

    for (id, session_id, html) in [
        (
            36579_u64,
            first_replacement.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-first-mixed-name-handle-replacement</div></body>",
        ),
        (
            36580_u64,
            second_replacement.session_id.as_str(),
            "<body><div id='utility-handle-a'>thin-second-mixed-name-handle-replacement</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            36581_u64,
            first_replacement.target_id.as_str(),
            first_replacement.session_id.as_str(),
        ),
        (
            36582_u64,
            second_replacement.target_id.as_str(),
            second_replacement.session_id.as_str(),
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
        "id": 36583,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(36583, json!({}), None);
    let events = ctx.take_all();
    let attached_events = events
        .iter()
        .filter(|event| event["method"] == json!("Target.attachedToTarget"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(attached_events.len(), 2);
    let first_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(first_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin mixed-name handle first replacement re-auto-attached session")
        .to_owned();
    let second_reauto_session = attached_events
        .iter()
        .find(|event| {
            event["params"]["targetInfo"]["targetId"] == json!(second_replacement.target_id)
        })
        .and_then(|event| event["params"]["sessionId"].as_str())
        .expect("thin mixed-name handle second replacement re-auto-attached session")
        .to_owned();

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, out_context) in [
        (
            36584_u64,
            first_reauto_session.as_str(),
            first_replacement.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            36585_u64,
            second_reauto_session.as_str(),
            second_replacement.target_id.as_str(),
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
        *out_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("thin mixed-name handle replay utility context id");
        ctx.take_all();
    }

    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36586,
        36587,
        36588,
        36589,
        "customBindingB",
        &custom_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36590,
        36591,
        36592,
        36593,
        "customHandleBindingB",
        &custom_handle_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        first_reauto_session.as_str(),
        first_replay_utility_context,
        36594,
        36595,
        36596,
        36597,
        "__pw_keptHandleBinding",
        &retained_handle_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36598,
        36599,
        36600,
        36601,
        "customBindingA",
        &custom_a_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36602,
        36603,
        36604,
        36605,
        "customBindingB",
        &custom_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36606,
        36607,
        36608,
        36609,
        "customHandleBindingA",
        &custom_handle_a_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36610,
        36611,
        36612,
        36613,
        "customHandleBindingB",
        &custom_handle_b_wrapper_source,
    )
    .await;
    install_patchright_crpage_binding_in_existing_worlds_async(
        &mut ctx,
        second_reauto_session.as_str(),
        second_replay_utility_context,
        36614,
        36615,
        36616,
        36617,
        "__pw_keptHandleBinding",
        &retained_handle_wrapper_source,
    )
    .await;

    for (id, session_id, context_id, expected_state) in [
        (
            36618_u64,
            first_reauto_session.as_str(),
            None::<i64>,
            json!("[\"undefined\",\"function\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36619_u64,
            first_reauto_session.as_str(),
            Some(first_replay_utility_context),
            json!("[\"undefined\",\"function\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            36620_u64,
            second_reauto_session.as_str(),
            None::<i64>,
            json!("[\"function\",\"function\",\"function\",\"function\",\"function\"]"),
        ),
        (
            36621_u64,
            second_reauto_session.as_str(),
            Some(second_replay_utility_context),
            json!("[\"function\",\"function\",\"function\",\"function\",\"function\"]"),
        ),
    ] {
        let mut params = json!({
            "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customBindingB, typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptHandleBinding])"
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
        assert_eq!(
            take_response_by_id(&mut ctx, id)["result"]["result"]["value"],
            expected_state
        );
    }

    ctx.process_async(json!({
            "id": 36622,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "expression": "globalThis.__lm_thin_mixed_name_handle_second_custom_a = customBindingA({ source: 'thin-mixed-name-handle-second-custom-a', nested: { count: 38, values: ['mixed-name-handle', 39, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-thin-mixed-name-handle-second-custom-a'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_second_custom_a = take_response_by_id(&mut ctx, 36622);
    assert!(
        scheduled_second_custom_a["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed-name handle second custom a")
            .starts_with("scheduled-")
    );
    let second_custom_a_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(second_reauto_session)
                && message["params"]["name"] == json!("customBindingA")
        })
        .cloned()
        .expect("thin mixed-name handle second custom a bindingCalled");
    let second_custom_a_payload: serde_json::Value = serde_json::from_str(
        second_custom_a_called["params"]["payload"]
            .as_str()
            .expect("thin mixed-name handle second custom a payload string"),
    )
    .expect("thin mixed-name handle second custom a payload json");
    let second_custom_a_seq = second_custom_a_payload["seq"]
        .as_i64()
        .expect("thin mixed-name handle second custom a seq");
    assert_eq!(
        second_custom_a_payload["serializedArgs"],
        json!([{ "source": "thin-mixed-name-handle-second-custom-a", "nested": { "count": 38, "values": ["mixed-name-handle", 39, false] } }])
    );
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36623,
            "method": "Runtime.evaluate",
            "sessionId": second_reauto_session,
            "params": {
                "expression": format!("globalThis.__lm_custom_binding_a_deliver({{ name: 'customBindingA', seq: {second_custom_a_seq}, error: 'thin-mixed-name-handle-second-custom-a-error' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36623)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36624,
        "method": "Runtime.evaluate",
        "sessionId": second_reauto_session,
        "params": {
            "expression": "globalThis.__lm_thin_mixed_name_handle_second_custom_a",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36624)["result"]["result"]["value"],
        json!("rejected:thin-mixed-name-handle-second-custom-a-error")
    );

    ctx.process_async(json!({
            "id": 36625,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": "globalThis.__lm_thin_mixed_name_handle_first_pw = __pw_keptHandleBinding(document.getElementById('utility-handle-a')); 'scheduled-thin-mixed-name-handle-first-pw'",
                "awaitPromise": true
            }
        })).await;
    let scheduled_first_pw_handle = take_response_by_id(&mut ctx, 36625);
    assert!(
        scheduled_first_pw_handle["result"]["result"]["value"]
            .as_str()
            .expect("scheduled thin mixed-name handle first pw")
            .starts_with("scheduled-")
    );
    let first_pw_handle_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(first_reauto_session)
                && message["params"]["name"] == json!("__pw_keptHandleBinding")
                && message["params"]["executionContextId"] == json!(first_replay_utility_context)
        })
        .cloned()
        .expect("thin mixed-name handle first pw bindingCalled");
    let first_pw_handle_payload: serde_json::Value = serde_json::from_str(
        first_pw_handle_called["params"]["payload"]
            .as_str()
            .expect("thin mixed-name handle first pw payload string"),
    )
    .expect("thin mixed-name handle first pw payload json");
    let first_pw_handle_seq = first_pw_handle_payload["seq"]
        .as_i64()
        .expect("thin mixed-name handle first pw seq");
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 36626,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("(() => {{ const handle = globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {first_pw_handle_seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {first_pw_handle_seq} }})]); }})()")
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36626)["result"]["result"]["value"],
        json!("[\"utility-handle-a\",\"thin-first-mixed-name-handle-replacement\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 36627,
            "method": "Runtime.evaluate",
            "sessionId": first_reauto_session,
            "params": {
                "contextId": first_replay_utility_context,
                "expression": format!("globalThis.__lm_pw_kept_handle_binding_deliver({{ name: '__pw_keptHandleBinding', seq: {first_pw_handle_seq}, result: 'thin-mixed-name-handle-first-pw-ok' }}); 'delivered'"),
                "awaitPromise": true
            }
        })).await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36627)["result"]["result"]["value"],
        json!("delivered")
    );

    ctx.process_async(json!({
        "id": 36628,
        "method": "Runtime.evaluate",
        "sessionId": first_reauto_session,
        "params": {
            "contextId": first_replay_utility_context,
            "expression": "globalThis.__lm_thin_mixed_name_handle_first_pw",
            "awaitPromise": true
        }
    }))
    .await;
    assert_eq!(
        take_response_by_id(&mut ctx, 36628)["result"]["result"]["value"],
        json!("thin-mixed-name-handle-first-pw-ok")
    );
    })
    .await;
}
