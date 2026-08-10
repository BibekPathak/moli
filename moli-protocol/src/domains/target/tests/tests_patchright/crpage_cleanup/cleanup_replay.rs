use super::*;

async fn patchright_cleanup_replay_large_stack<F, Fut>(name: &'static str, build: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()>,
{
    let result = std::thread::Builder::new()
        .name(name.into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("large-stack patchright cleanup-replay test runtime should build")
                .block_on(build());
        })
        .expect("large-stack patchright cleanup-replay test thread should spawn")
        .join();

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_only_clears_matching_name_in_cleaned_context()
 {
    patchright_cleanup_replay_large_stack(
        "patchright-handle-cleanup-replay-clear-matching",
        run_patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_only_clears_matching_name_in_cleaned_context,
    )
    .await;
}

async fn run_patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_only_clears_matching_name_in_cleaned_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34250, 34251, 34252)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34253, 34254, 34255)
            .await;

    for (id, session_id, html) in [
        (
            34256_u64,
            first.session_id.as_str(),
            "<body><div id='first-initial-a'>first-initial-a</div><div id='first-initial-b'>first-initial-b</div><div id='first-initial-kept'>first-initial-kept</div></body>",
        ),
        (
            34257_u64,
            second.session_id.as_str(),
            "<body><div id='second-initial-a'>second-initial-a</div><div id='second-initial-b'>second-initial-b</div><div id='second-initial-kept'>second-initial-kept</div></body>",
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
            34258_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            34259_u64,
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
        "id": 34260,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(34260, json!({}), None);
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
            34261_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            34262_u64,
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

    let custom_a_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingA",
        "__lm_custom_handle_a_deliver",
        Some("__lm_custom_handle_a_take"),
        true,
    );
    let custom_b_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingB",
        "__lm_custom_handle_b_deliver",
        Some("__lm_custom_handle_b_take"),
        true,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptHandleBinding",
        "__lm_pw_kept_handle_binding_deliver",
        Some("__lm_pw_kept_handle_binding_take"),
        true,
    );

    for (id, session_id, utility_context) in [
        (
            34263_u64,
            first_auto_session.as_str(),
            first_utility_context,
        ),
        (
            34269_u64,
            second_auto_session.as_str(),
            second_utility_context,
        ),
    ] {
        for (offset, binding_name) in [
            (0_u64, "customHandleBindingA"),
            (2_u64, "customHandleBindingB"),
            (4_u64, "__pw_keptHandleBinding"),
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
            (1_u64, custom_a_wrapper_source.as_str()),
            (3_u64, custom_b_wrapper_source.as_str()),
            (5_u64, retained_wrapper_source.as_str()),
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
        "id": 34275,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "customHandleBindingA"
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 34275)["result"], json!({}));

    for (id, session_id, html) in [
        (
            34276_u64,
            first_auto_session.as_str(),
            "<body><div id='first-replay-a'>first-replay-a</div><div id='first-replay-b'>first-replay-b</div><div id='first-replay-kept'>first-replay-kept</div></body>",
        ),
        (
            34277_u64,
            second_auto_session.as_str(),
            "<body><div id='second-replay-a'>second-replay-a</div><div id='second-replay-b'>second-replay-b</div><div id='second-replay-kept'>second-replay-kept</div></body>",
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

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            34278_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            34279_u64,
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
            .expect("replay utility context id");
        ctx.take_all();
    }

    for (id, session_id, utility_context, names) in [
        (
            34280_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            vec!["customHandleBindingB", "__pw_keptHandleBinding"],
        ),
        (
            34282_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            vec![
                "customHandleBindingA",
                "customHandleBindingB",
                "__pw_keptHandleBinding",
            ],
        ),
    ] {
        for (offset, binding_name) in names.iter().enumerate() {
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
    }

    for (id, session_id, utility_context, source, expected_type) in [
        (
            34284_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            custom_a_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34285_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            34286_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34287_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            custom_a_wrapper_source.as_str(),
            "function",
        ),
        (
            34288_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            34289_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            retained_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": source,
                "awaitPromise": true
            }
        }))
        .await;
        let replayed = take_response_by_id(&mut ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, utility_context, expected_state) in [
        (
            34290_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            json!("[\"undefined\",\"function\",\"function\"]"),
        ),
        (
            34291_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            json!("[\"function\",\"function\",\"function\"]"),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": "JSON.stringify([typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }

    for (
        id,
        session_id,
        utility_context,
        expression,
        binding_name,
        take_helper,
        expected_text,
        deliver_helper,
        promise_name,
        expected_result,
    ) in [
        (
            34292_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            "globalThis.__lm_first_replay_handle_b = customHandleBindingB(document.getElementById('first-replay-b')); 'scheduled-first-b'",
            "customHandleBindingB",
            "__lm_custom_handle_b_take",
            "first-replay-b",
            "__lm_custom_handle_b_deliver",
            "__lm_first_replay_handle_b",
            "first-replay-b-ok",
        ),
        (
            34293_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            "globalThis.__lm_first_replay_kept_handle = __pw_keptHandleBinding(document.getElementById('first-replay-kept')); 'scheduled-first-kept'",
            "__pw_keptHandleBinding",
            "__lm_pw_kept_handle_binding_take",
            "first-replay-kept",
            "__lm_pw_kept_handle_binding_deliver",
            "__lm_first_replay_kept_handle",
            "first-replay-kept-ok",
        ),
        (
            34294_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            "globalThis.__lm_second_replay_handle_a = customHandleBindingA(document.getElementById('second-replay-a')); 'scheduled-second-a'",
            "customHandleBindingA",
            "__lm_custom_handle_a_take",
            "second-replay-a",
            "__lm_custom_handle_a_deliver",
            "__lm_second_replay_handle_a",
            "second-replay-a-ok",
        ),
        (
            34295_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            "globalThis.__lm_second_replay_handle_b = customHandleBindingB(document.getElementById('second-replay-b')); 'scheduled-second-b'",
            "customHandleBindingB",
            "__lm_custom_handle_b_take",
            "second-replay-b",
            "__lm_custom_handle_b_deliver",
            "__lm_second_replay_handle_b",
            "second-replay-b-ok",
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
                .expect("scheduled handle replay value")
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
                    && message["params"]["executionContextId"] == json!(utility_context)
            })
            .cloned()
            .expect("replayed handle binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        let seq = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(seq, 1);
        ctx.sent.clear();

        ctx.process_async(json!({
                "id": id + 10,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": format!(
                        "(() => {{ const handle = globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }})]); }})()"
                    )
                }
            })).await;
        let taken = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(
            taken["result"]["result"]["value"],
            json!(format!(
                "[\"{}\",\"{}\",\"undefined\"]",
                expected_text, expected_text
            ))
        );

        ctx.process_async(json!({
                "id": id + 20,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": format!(
                        "globalThis.{deliver_helper}({{ name: '{binding_name}', seq: {seq}, result: '{expected_result}' }}); 'delivered'"
                    ),
                    "awaitPromise": true
                }
            })).await;
        let delivered = take_response_by_id(&mut ctx, id + 20);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        ctx.process_async(json!({
            "id": id + 30,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": format!("globalThis.{promise_name}"),
                "awaitPromise": true
            }
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 30);
        assert_eq!(
            resolved["result"]["result"]["value"],
            json!(expected_result)
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_keeps_pw_handle_bindings_and_only_clears_matching_name_in_cleaned_context()
 {
    super::super::patchright_8mb_stack(
        "patchright-handle-cleanup-replay-keeps-pw",
        run_patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_keeps_pw_handle_bindings_and_only_clears_matching_name_in_cleaned_context,
    )
    .await;
}

async fn run_patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_keeps_pw_handle_bindings_and_only_clears_matching_name_in_cleaned_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34600, 34601, 34602)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34603, 34604, 34605)
            .await;

    for (id, session_id, html) in [
        (
            34606_u64,
            first.session_id.as_str(),
            "<body><div id='first-main-a'>first-main-a</div><div id='first-main-b'>first-main-b</div><div id='first-main-kept'>first-main-kept</div><div id='first-utility-a'>first-utility-a</div><div id='first-utility-b'>first-utility-b</div><div id='first-utility-kept'>first-utility-kept</div></body>",
        ),
        (
            34607_u64,
            second.session_id.as_str(),
            "<body><div id='second-main-a'>second-main-a</div><div id='second-main-b'>second-main-b</div><div id='second-main-kept'>second-main-kept</div><div id='second-utility-a'>second-utility-a</div><div id='second-utility-b'>second-utility-b</div><div id='second-utility-kept'>second-utility-kept</div></body>",
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
            34608_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            34609_u64,
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
        "id": 34610,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(34610, json!({}), None);
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
            34611_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            34612_u64,
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

    let custom_a_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingA",
        "__lm_crpage_custom_handle_a_deliver",
        Some("__lm_crpage_custom_handle_a_take"),
        true,
    );
    let custom_b_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingB",
        "__lm_crpage_custom_handle_b_deliver",
        Some("__lm_crpage_custom_handle_b_take"),
        true,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptHandleBinding",
        "__lm_crpage_pw_kept_handle_deliver",
        Some("__lm_crpage_pw_kept_handle_take"),
        true,
    );

    for (id, session_id, utility_context) in [
        (
            34613_u64,
            first_auto_session.as_str(),
            first_utility_context,
        ),
        (
            34631_u64,
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
            "customHandleBindingA",
            &custom_a_wrapper_source,
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
            "customHandleBindingB",
            &custom_b_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context,
            id + 8,
            id + 9,
            id + 10,
            id + 11,
            "__pw_keptHandleBinding",
            &retained_wrapper_source,
        )
        .await;
    }

    for (id, source, world_name, session_id) in [
        (
            34649_u64,
            custom_a_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34650_u64,
            custom_b_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34651_u64,
            retained_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34652_u64,
            custom_a_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34653_u64,
            custom_b_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34654_u64,
            retained_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34655_u64,
            custom_a_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34656_u64,
            custom_b_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34657_u64,
            retained_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34658_u64,
            custom_a_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
        ),
        (
            34659_u64,
            custom_b_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
        ),
        (
            34660_u64,
            retained_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
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
            "id": id,
            "method": "Page.addScriptToEvaluateOnNewDocument",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        assert!(
            take_response_by_id(&mut ctx, id)["result"]["identifier"]
                .as_str()
                .is_some()
        );
    }

    ctx.process_async(json!({
        "id": 34661,
        "method": "Runtime.removeBinding",
        "sessionId": first_auto_session,
        "params": {
            "name": "customHandleBindingA"
        }
    }))
    .await;
    assert_eq!(take_response_by_id(&mut ctx, 34661)["result"], json!({}));

    for (id, session_id, label) in [
        (34662_u64, first_auto_session.as_str(), "first-replay"),
        (34663_u64, second_auto_session.as_str(), "second-replay"),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Page.navigate",
                "sessionId": session_id,
                "params": {
                    "url": format!("data:text/html,<body><div id='main-a'>{label}-main-a</div><div id='main-b'>{label}-main-b</div><div id='main-kept'>{label}-main-kept</div><div id='utility-a'>{label}-utility-a</div><div id='utility-b'>{label}-utility-b</div><div id='utility-kept'>{label}-utility-kept</div></body>")
                }
            })).await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, session_id, source, expected_type) in [
        (
            34664_u64,
            first_auto_session.as_str(),
            custom_a_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34665_u64,
            first_auto_session.as_str(),
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            34666_u64,
            first_auto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34667_u64,
            second_auto_session.as_str(),
            custom_a_wrapper_source.as_str(),
            "function",
        ),
        (
            34668_u64,
            second_auto_session.as_str(),
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            34669_u64,
            second_auto_session.as_str(),
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
        let replayed = take_response_by_id(&mut ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, expected_state) in [
        (
            34670_u64,
            first_auto_session.as_str(),
            json!("[\"undefined\",\"function\",\"function\"]"),
        ),
        (
            34671_u64,
            second_auto_session.as_str(),
            json!("[\"function\",\"function\",\"function\"]"),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": "JSON.stringify([typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            34672_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            34673_u64,
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
            .expect("replay utility context id");
        ctx.take_all();
    }

    for (id, session_id, utility_context, names) in [
        (
            34674_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            vec!["customHandleBindingB", "__pw_keptHandleBinding"],
        ),
        (
            34676_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            vec![
                "customHandleBindingA",
                "customHandleBindingB",
                "__pw_keptHandleBinding",
            ],
        ),
    ] {
        for (offset, binding_name) in names.iter().enumerate() {
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
    }

    for (id, session_id, utility_context, source, expected_type) in [
        (
            34679_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            custom_a_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34680_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            34681_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34682_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            custom_a_wrapper_source.as_str(),
            "function",
        ),
        (
            34683_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            34684_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            retained_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": source,
                "awaitPromise": true
            }
        }))
        .await;
        let replayed = take_response_by_id(&mut ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, utility_context, expected_state) in [
        (
            34685_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            json!("[\"undefined\",\"function\",\"function\"]"),
        ),
        (
            34686_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            json!("[\"function\",\"function\",\"function\"]"),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": "JSON.stringify([typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }

    for (
        id,
        session_id,
        context_id,
        expression,
        binding_name,
        take_helper,
        expected_id,
        expected_text,
        deliver_helper,
        promise_name,
        expected_result,
    ) in [
        (
            34687_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_replay_main_b = customHandleBindingB(document.getElementById('main-b')); 'scheduled-first-main-b'",
            "customHandleBindingB",
            "__lm_crpage_custom_handle_b_take",
            "main-b",
            "first-replay-main-b",
            "__lm_crpage_custom_handle_b_deliver",
            "__lm_first_replay_main_b",
            "first-replay-main-b-ok",
        ),
        (
            34688_u64,
            first_auto_session.as_str(),
            Some(first_replay_utility_context),
            "globalThis.__lm_first_replay_utility_kept = __pw_keptHandleBinding(document.getElementById('utility-kept')); 'scheduled-first-utility-kept'",
            "__pw_keptHandleBinding",
            "__lm_crpage_pw_kept_handle_take",
            "utility-kept",
            "first-replay-utility-kept",
            "__lm_crpage_pw_kept_handle_deliver",
            "__lm_first_replay_utility_kept",
            "first-replay-utility-kept-ok",
        ),
        (
            34689_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_replay_main_a = customHandleBindingA(document.getElementById('main-a')); 'scheduled-second-main-a'",
            "customHandleBindingA",
            "__lm_crpage_custom_handle_a_take",
            "main-a",
            "second-replay-main-a",
            "__lm_crpage_custom_handle_a_deliver",
            "__lm_second_replay_main_a",
            "second-replay-main-a-ok",
        ),
        (
            34690_u64,
            second_auto_session.as_str(),
            Some(second_replay_utility_context),
            "globalThis.__lm_second_replay_utility_kept = __pw_keptHandleBinding(document.getElementById('utility-kept')); 'scheduled-second-utility-kept'",
            "__pw_keptHandleBinding",
            "__lm_crpage_pw_kept_handle_take",
            "utility-kept",
            "second-replay-utility-kept",
            "__lm_crpage_pw_kept_handle_deliver",
            "__lm_second_replay_utility_kept",
            "second-replay-utility-kept-ok",
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
                .expect("scheduled replay handle value")
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
            .expect("replayed handle binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        let seq = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(seq, 1);
        ctx.sent.clear();

        let mut take_params = json!({
            "expression": format!(
                "(() => {{ const handle = globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }})]); }})()"
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
            json!(format!(
                "[\"{expected_id}\",\"{expected_text}\",\"undefined\"]"
            ))
        );

        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.{deliver_helper}({{ name: '{binding_name}', seq: {seq}, result: '{expected_result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 20,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id + 20);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 30,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 30);
        assert_eq!(
            resolved["result"]["result"]["value"],
            json!(expected_result)
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_removes_all_custom_bindings_and_retains_pw_only_in_cleaned_context()
 {
    super::super::patchright_8mb_stack(
        "patchright-handle-cleanup-replay-retains-pw",
        run_patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_removes_all_custom_bindings_and_retains_pw_only_in_cleaned_context,
    )
    .await;
}

async fn run_patchright_over_cdp_auto_attach_sweep_crpage_handle_cleanup_replay_removes_all_custom_bindings_and_retains_pw_only_in_cleaned_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34800, 34801, 34802)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34803, 34804, 34805)
            .await;

    for (id, session_id, html) in [
        (
            34806_u64,
            first.session_id.as_str(),
            "<body><div id='first-main-a'>first-main-a</div><div id='first-main-b'>first-main-b</div><div id='first-main-kept'>first-main-kept</div><div id='first-utility-a'>first-utility-a</div><div id='first-utility-b'>first-utility-b</div><div id='first-utility-kept'>first-utility-kept</div></body>",
        ),
        (
            34807_u64,
            second.session_id.as_str(),
            "<body><div id='second-main-a'>second-main-a</div><div id='second-main-b'>second-main-b</div><div id='second-main-kept'>second-main-kept</div><div id='second-utility-a'>second-utility-a</div><div id='second-utility-b'>second-utility-b</div><div id='second-utility-kept'>second-utility-kept</div></body>",
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
            34808_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            34809_u64,
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
        "id": 34810,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": false
        }
    }))
    .await;
    ctx.expect_result(34810, json!({}), None);
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
            34811_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            34812_u64,
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

    let custom_a_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingA",
        "__lm_crpage_custom_handle_a_deliver",
        Some("__lm_crpage_custom_handle_a_take"),
        true,
    );
    let custom_b_wrapper_source = patchright_page_binding_wrapper_source(
        "customHandleBindingB",
        "__lm_crpage_custom_handle_b_deliver",
        Some("__lm_crpage_custom_handle_b_take"),
        true,
    );
    let retained_wrapper_source = patchright_page_binding_wrapper_source(
        "__pw_keptHandleBinding",
        "__lm_crpage_pw_kept_handle_deliver",
        Some("__lm_crpage_pw_kept_handle_take"),
        true,
    );

    for (id, session_id, utility_context) in [
        (
            34813_u64,
            first_auto_session.as_str(),
            first_utility_context,
        ),
        (
            34825_u64,
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
            "customHandleBindingA",
            &custom_a_wrapper_source,
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
            "customHandleBindingB",
            &custom_b_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            &mut ctx,
            session_id,
            utility_context,
            id + 8,
            id + 9,
            id + 10,
            id + 11,
            "__pw_keptHandleBinding",
            &retained_wrapper_source,
        )
        .await;
    }

    for (id, source, world_name, session_id) in [
        (
            34837_u64,
            custom_a_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34838_u64,
            custom_b_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34839_u64,
            retained_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34840_u64,
            custom_a_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34841_u64,
            custom_b_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34842_u64,
            retained_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34843_u64,
            custom_a_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34844_u64,
            custom_b_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34845_u64,
            retained_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34846_u64,
            custom_a_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
        ),
        (
            34847_u64,
            custom_b_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
        ),
        (
            34848_u64,
            retained_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
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
            "id": id,
            "method": "Page.addScriptToEvaluateOnNewDocument",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        assert!(
            take_response_by_id(&mut ctx, id)["result"]["identifier"]
                .as_str()
                .is_some()
        );
    }

    for (id, binding_name) in [
        (34849_u64, "customHandleBindingA"),
        (34850_u64, "customHandleBindingB"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.removeBinding",
            "sessionId": first_auto_session,
            "params": {
                "name": binding_name
            }
        }))
        .await;
        assert_eq!(take_response_by_id(&mut ctx, id)["result"], json!({}));
    }

    for (id, session_id, label) in [
        (34851_u64, first_auto_session.as_str(), "first-replay"),
        (34852_u64, second_auto_session.as_str(), "second-replay"),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Page.navigate",
                "sessionId": session_id,
                "params": {
                    "url": format!("data:text/html,<body><div id='main-a'>{label}-main-a</div><div id='main-b'>{label}-main-b</div><div id='main-kept'>{label}-main-kept</div><div id='utility-a'>{label}-utility-a</div><div id='utility-b'>{label}-utility-b</div><div id='utility-kept'>{label}-utility-kept</div></body>")
                }
            })).await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, session_id, source, expected_type) in [
        (
            34853_u64,
            first_auto_session.as_str(),
            custom_a_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34854_u64,
            first_auto_session.as_str(),
            custom_b_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34855_u64,
            first_auto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34856_u64,
            second_auto_session.as_str(),
            custom_a_wrapper_source.as_str(),
            "function",
        ),
        (
            34857_u64,
            second_auto_session.as_str(),
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            34858_u64,
            second_auto_session.as_str(),
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
        let replayed = take_response_by_id(&mut ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, expected_state) in [
        (
            34859_u64,
            first_auto_session.as_str(),
            json!("[\"undefined\",\"undefined\",\"function\"]"),
        ),
        (
            34860_u64,
            second_auto_session.as_str(),
            json!("[\"function\",\"function\",\"function\"]"),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": "JSON.stringify([typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            34861_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            34862_u64,
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
            .expect("replay utility context id");
        ctx.take_all();
    }

    for (id, session_id, utility_context, names) in [
        (
            34863_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            vec!["__pw_keptHandleBinding"],
        ),
        (
            34864_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            vec![
                "customHandleBindingA",
                "customHandleBindingB",
                "__pw_keptHandleBinding",
            ],
        ),
    ] {
        for (offset, binding_name) in names.iter().enumerate() {
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
    }

    for (id, session_id, utility_context, source, expected_type) in [
        (
            34867_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            custom_a_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34868_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            custom_b_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34869_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34870_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            custom_a_wrapper_source.as_str(),
            "function",
        ),
        (
            34871_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            custom_b_wrapper_source.as_str(),
            "function",
        ),
        (
            34872_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            retained_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": source,
                "awaitPromise": true
            }
        }))
        .await;
        let replayed = take_response_by_id(&mut ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, utility_context, expected_state) in [
        (
            34873_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            json!("[\"undefined\",\"undefined\",\"function\"]"),
        ),
        (
            34874_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            json!("[\"function\",\"function\",\"function\"]"),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": "JSON.stringify([typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }

    for (
        id,
        session_id,
        context_id,
        expression,
        binding_name,
        take_helper,
        expected_id,
        expected_text,
        deliver_helper,
        promise_name,
        expected_result,
    ) in [
        (
            34875_u64,
            first_auto_session.as_str(),
            Some(first_replay_utility_context),
            "globalThis.__lm_first_replay_utility_kept = __pw_keptHandleBinding(document.getElementById('utility-kept')); 'scheduled-first-utility-kept'",
            "__pw_keptHandleBinding",
            "__lm_crpage_pw_kept_handle_take",
            "utility-kept",
            "first-replay-utility-kept",
            "__lm_crpage_pw_kept_handle_deliver",
            "__lm_first_replay_utility_kept",
            "first-replay-utility-kept-ok",
        ),
        (
            34876_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_replay_main_a = customHandleBindingA(document.getElementById('main-a')); 'scheduled-second-main-a'",
            "customHandleBindingA",
            "__lm_crpage_custom_handle_a_take",
            "main-a",
            "second-replay-main-a",
            "__lm_crpage_custom_handle_a_deliver",
            "__lm_second_replay_main_a",
            "second-replay-main-a-ok",
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
                .expect("scheduled replay handle value")
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
            .expect("replayed handle binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        let seq = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(seq, 1);
        ctx.sent.clear();

        let mut take_params = json!({
            "expression": format!(
                "(() => {{ const handle = globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }})]); }})()"
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
            json!(format!(
                "[\"{expected_id}\",\"{expected_text}\",\"undefined\"]"
            ))
        );

        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.{deliver_helper}({{ name: '{binding_name}', seq: {seq}, result: '{expected_result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 20,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id + 20);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 30,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 30);
        assert_eq!(
            resolved["result"]["result"]["value"],
            json!(expected_result)
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_cleanup_replay_mixed_binding_kinds_retain_pw_only_in_cleaned_context()
 {
    patchright_cleanup_replay_large_stack(
        "patchright-cleanup-replay-mixed-binding-kinds",
        run_patchright_over_cdp_auto_attach_sweep_crpage_cleanup_replay_mixed_binding_kinds_retain_pw_only_in_cleaned_context,
    )
    .await;
}

async fn run_patchright_over_cdp_auto_attach_sweep_crpage_cleanup_replay_mixed_binding_kinds_retain_pw_only_in_cleaned_context()
 {
    let mut ctx = TestContext::new();
    let first =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34900, 34901, 34902)
            .await;
    let second =
        create_attached_page_session_without_runtime_enable_async(&mut ctx, 34903, 34904, 34905)
            .await;

    for (id, session_id, html) in [
        (
            34906_u64,
            first.session_id.as_str(),
            "<body><div id='first-main-handle'>first-main-handle</div><div id='first-utility-handle'>first-utility-handle</div></body>",
        ),
        (
            34907_u64,
            second.session_id.as_str(),
            "<body><div id='second-main-handle'>second-main-handle</div><div id='second-utility-handle'>second-utility-handle</div></body>",
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
            34908_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            34909_u64,
            second.target_id.as_str(),
            second.session_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": { "targetId": target_id, "sessionId": session_id }
        }))
        .await;
        ctx.expect_result(id, json!({}), None);
        ctx.expect_event(
            "Target.detachedFromTarget",
            Some(&json!({ "targetId": target_id, "sessionId": session_id })),
        );
    }

    ctx.process_async(json!({
        "id": 34910,
        "method": "Target.setAutoAttach",
        "params": { "autoAttach": true, "waitForDebuggerOnStart": false }
    }))
    .await;
    ctx.expect_result(34910, json!({}), None);
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
            34911_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            34912_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.createIsolatedWorld",
            "sessionId": session_id,
            "params": { "frameId": target_id, "worldName": "utility" }
        }))
        .await;
        *utility_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("utility context id after auto-attach");
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

    install_mixed_binding_kinds_cleanup_replay_initial_bindings_async(
        &mut ctx,
        first_auto_session.as_str(),
        second_auto_session.as_str(),
        first_utility_context,
        second_utility_context,
        &custom_wrapper_source,
        &custom_handle_wrapper_source,
        &retained_wrapper_source,
        &retained_handle_wrapper_source,
    )
    .await;

    for (id, source, world_name, session_id) in [
        (
            34945_u64,
            custom_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34946_u64,
            custom_handle_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34947_u64,
            retained_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34948_u64,
            retained_handle_wrapper_source.as_str(),
            None,
            first_auto_session.as_str(),
        ),
        (
            34949_u64,
            custom_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34950_u64,
            custom_handle_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34951_u64,
            retained_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34952_u64,
            retained_handle_wrapper_source.as_str(),
            Some("utility"),
            first_auto_session.as_str(),
        ),
        (
            34953_u64,
            custom_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34954_u64,
            custom_handle_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34955_u64,
            retained_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34956_u64,
            retained_handle_wrapper_source.as_str(),
            None,
            second_auto_session.as_str(),
        ),
        (
            34957_u64,
            custom_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
        ),
        (
            34958_u64,
            custom_handle_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
        ),
        (
            34959_u64,
            retained_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
        ),
        (
            34960_u64,
            retained_handle_wrapper_source.as_str(),
            Some("utility"),
            second_auto_session.as_str(),
        ),
    ] {
        let mut params = json!({ "source": source, "runImmediately": true });
        if let Some(world_name) = world_name {
            params["worldName"] = json!(world_name);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Page.addScriptToEvaluateOnNewDocument",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        assert!(
            take_response_by_id(&mut ctx, id)["result"]["identifier"]
                .as_str()
                .is_some()
        );
    }

    for (id, binding_name) in [
        (34961_u64, "customBindingA"),
        (34962_u64, "customHandleBindingA"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.removeBinding",
            "sessionId": first_auto_session,
            "params": { "name": binding_name }
        }))
        .await;
        assert_eq!(take_response_by_id(&mut ctx, id)["result"], json!({}));
    }

    for (id, session_id, label) in [
        (34963_u64, first_auto_session.as_str(), "first-replay"),
        (34964_u64, second_auto_session.as_str(), "second-replay"),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Page.navigate",
                "sessionId": session_id,
                "params": {
                    "url": format!("data:text/html,<body><div id='main-handle'>{label}-main-handle</div><div id='utility-handle'>{label}-utility-handle</div></body>")
                }
            })).await;
        let navigation = take_response_by_id(&mut ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, session_id, source, expected_type) in [
        (
            34965_u64,
            first_auto_session.as_str(),
            custom_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34966_u64,
            first_auto_session.as_str(),
            custom_handle_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34967_u64,
            first_auto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34968_u64,
            first_auto_session.as_str(),
            retained_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            34969_u64,
            second_auto_session.as_str(),
            custom_wrapper_source.as_str(),
            "function",
        ),
        (
            34970_u64,
            second_auto_session.as_str(),
            custom_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            34971_u64,
            second_auto_session.as_str(),
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34972_u64,
            second_auto_session.as_str(),
            retained_handle_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": { "expression": source, "awaitPromise": true }
        }))
        .await;
        let replayed = take_response_by_id(&mut ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, expected_state) in [
        (
            34973_u64,
            first_auto_session.as_str(),
            json!("[\"undefined\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            34974_u64,
            second_auto_session.as_str(),
            json!("[\"function\",\"function\",\"function\",\"function\"]"),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customHandleBindingA, typeof globalThis.__pw_keptBinding, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }

    let mut first_replay_utility_context = 0_i64;
    let mut second_replay_utility_context = 0_i64;
    for (id, session_id, target_id, utility_context) in [
        (
            34975_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_replay_utility_context,
        ),
        (
            34976_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_replay_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.createIsolatedWorld",
            "sessionId": session_id,
            "params": { "frameId": target_id, "worldName": "utility" }
        }))
        .await;
        *utility_context = take_response_by_id(&mut ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("replay utility context id");
        ctx.take_all();
    }

    for (id, session_id, utility_context, names) in [
        (
            34977_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            vec!["__pw_keptBinding", "__pw_keptHandleBinding"],
        ),
        (
            34979_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            vec![
                "customBindingA",
                "customHandleBindingA",
                "__pw_keptBinding",
                "__pw_keptHandleBinding",
            ],
        ),
    ] {
        for (offset, binding_name) in names.iter().enumerate() {
            ctx.process_async(json!({
                "id": id + offset as u64,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": { "name": binding_name, "executionContextId": utility_context }
            }))
            .await;
            assert_eq!(
                take_response_by_id(&mut ctx, id + offset as u64)["result"],
                json!({})
            );
        }
    }

    for (id, session_id, utility_context, source, expected_type) in [
        (
            34983_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            custom_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34984_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            custom_handle_wrapper_source.as_str(),
            "undefined",
        ),
        (
            34985_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34986_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            retained_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            34987_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            custom_wrapper_source.as_str(),
            "function",
        ),
        (
            34988_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            custom_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            34989_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            retained_wrapper_source.as_str(),
            "function",
        ),
        (
            34990_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            retained_handle_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": source,
                "awaitPromise": true
            }
        }))
        .await;
        let replayed = take_response_by_id(&mut ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, utility_context, expected_state) in [
        (
            34991_u64,
            first_auto_session.as_str(),
            first_replay_utility_context,
            json!("[\"undefined\",\"undefined\",\"function\",\"function\"]"),
        ),
        (
            34992_u64,
            second_auto_session.as_str(),
            second_replay_utility_context,
            json!("[\"function\",\"function\",\"function\",\"function\"]"),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customHandleBindingA, typeof globalThis.__pw_keptBinding, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(&mut ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }

    for (
        id,
        session_id,
        context_id,
        expression,
        binding_name,
        expected_payload,
        deliver_expression,
        promise_name,
        expected_result,
    ) in [
        (
            34993_u64,
            first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_replay_pw = __pw_keptBinding({ source: 'first-replay-pw', nested: { count: 1, values: ['a', 2, true] } }); 'scheduled-first-pw'",
            "__pw_keptBinding",
            json!([{
                "source": "first-replay-pw",
                "nested": { "count": 1, "values": ["a", 2, true] }
            }]),
            "globalThis.__lm_pw_kept_binding_deliver({ name: '__pw_keptBinding', seq: 1, result: 'first-replay-pw-ok' }); 'delivered'",
            "__lm_first_replay_pw",
            "first-replay-pw-ok",
        ),
        (
            34994_u64,
            second_auto_session.as_str(),
            Some(second_replay_utility_context),
            "globalThis.__lm_second_replay_custom = customBindingA({ source: 'second-replay-custom', nested: { count: 2, values: ['b', 3, false] } }); 'scheduled-second-custom'",
            "customBindingA",
            json!([{
                "source": "second-replay-custom",
                "nested": { "count": 2, "values": ["b", 3, false] }
            }]),
            "globalThis.__lm_custom_binding_a_deliver({ name: 'customBindingA', seq: 1, result: 'second-replay-custom-ok' }); 'delivered'",
            "__lm_second_replay_custom",
            "second-replay-custom-ok",
        ),
    ] {
        let mut params = json!({ "expression": expression, "awaitPromise": true });
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
            .expect("binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        assert_eq!(payload["serializedArgs"], expected_payload);
        assert_eq!(payload["seq"], json!(1));
        ctx.sent.clear();

        let mut deliver_params = json!({ "expression": deliver_expression, "awaitPromise": true });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id + 10);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 20,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 20);
        assert_eq!(
            resolved["result"]["result"]["value"],
            json!(expected_result)
        );
    }

    for (
        id,
        session_id,
        context_id,
        expression,
        binding_name,
        take_helper,
        expected_id,
        expected_text,
        deliver_helper,
        promise_name,
        expected_result,
    ) in [
        (
            34995_u64,
            first_auto_session.as_str(),
            Some(first_replay_utility_context),
            "globalThis.__lm_first_replay_pw_handle = __pw_keptHandleBinding(document.getElementById('utility-handle')); 'scheduled-first-pw-handle'",
            "__pw_keptHandleBinding",
            "__lm_pw_kept_handle_binding_take",
            "utility-handle",
            "first-replay-utility-handle",
            "__lm_pw_kept_handle_binding_deliver",
            "__lm_first_replay_pw_handle",
            "first-replay-pw-handle-ok",
        ),
        (
            34996_u64,
            second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_replay_custom_handle = customHandleBindingA(document.getElementById('main-handle')); 'scheduled-second-custom-handle'",
            "customHandleBindingA",
            "__lm_custom_handle_binding_a_take",
            "main-handle",
            "second-replay-main-handle",
            "__lm_custom_handle_binding_a_deliver",
            "__lm_second_replay_custom_handle",
            "second-replay-custom-handle-ok",
        ),
    ] {
        let mut params = json!({ "expression": expression, "awaitPromise": true });
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
                .expect("scheduled replay handle value")
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
            .expect("replayed handle binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        let seq = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(seq, 1);
        ctx.sent.clear();

        let mut take_params = json!({
            "expression": format!(
                "(() => {{ const handle = globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }})]); }})()"
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
            json!(format!(
                "[\"{expected_id}\",\"{expected_text}\",\"undefined\"]"
            ))
        );

        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.{deliver_helper}({{ name: '{binding_name}', seq: {seq}, result: '{expected_result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 20,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(&mut ctx, id + 20);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 30,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(&mut ctx, id + 30);
        assert_eq!(
            resolved["result"]["result"]["value"],
            json!(expected_result)
        );
    }
}

async fn install_mixed_binding_kinds_cleanup_replay_initial_bindings_async(
    ctx: &mut TestContext,
    first_auto_session: &str,
    second_auto_session: &str,
    first_utility_context: i64,
    second_utility_context: i64,
    custom_wrapper_source: &str,
    custom_handle_wrapper_source: &str,
    retained_wrapper_source: &str,
    retained_handle_wrapper_source: &str,
) {
    for (id, session_id, utility_context) in [
        (34913_u64, first_auto_session, first_utility_context),
        (34929_u64, second_auto_session, second_utility_context),
    ] {
        install_patchright_crpage_binding_in_existing_worlds_async(
            ctx,
            session_id,
            utility_context,
            id,
            id + 1,
            id + 2,
            id + 3,
            "customBindingA",
            custom_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            ctx,
            session_id,
            utility_context,
            id + 4,
            id + 5,
            id + 6,
            id + 7,
            "customHandleBindingA",
            custom_handle_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            ctx,
            session_id,
            utility_context,
            id + 8,
            id + 9,
            id + 10,
            id + 11,
            "__pw_keptBinding",
            retained_wrapper_source,
        )
        .await;
        install_patchright_crpage_binding_in_existing_worlds_async(
            ctx,
            session_id,
            utility_context,
            id + 12,
            id + 13,
            id + 14,
            id + 15,
            "__pw_keptHandleBinding",
            retained_handle_wrapper_source,
        )
        .await;
    }
}

struct MixedCleanupReplayState {
    first: AttachedPageSession,
    second: AttachedPageSession,
    first_auto_session: String,
    second_auto_session: String,
    first_utility_context: i64,
    second_utility_context: i64,
    first_replay_utility_context: i64,
    second_replay_utility_context: i64,
}

struct MixedCleanupSources {
    custom_wrapper_a_source: String,
    custom_wrapper_b_source: String,
    custom_handle_wrapper_a_source: String,
    custom_handle_wrapper_b_source: String,
    retained_wrapper_source: String,
    retained_handle_wrapper_source: String,
}

fn mixed_cleanup_sources() -> MixedCleanupSources {
    MixedCleanupSources {
        custom_wrapper_a_source: patchright_page_binding_wrapper_source(
            "customBindingA",
            "__lm_custom_binding_a_deliver",
            None,
            false,
        ),
        custom_wrapper_b_source: patchright_page_binding_wrapper_source(
            "customBindingB",
            "__lm_custom_binding_b_deliver",
            None,
            false,
        ),
        custom_handle_wrapper_a_source: patchright_page_binding_wrapper_source(
            "customHandleBindingA",
            "__lm_custom_handle_binding_a_deliver",
            Some("__lm_custom_handle_binding_a_take"),
            true,
        ),
        custom_handle_wrapper_b_source: patchright_page_binding_wrapper_source(
            "customHandleBindingB",
            "__lm_custom_handle_binding_b_deliver",
            Some("__lm_custom_handle_binding_b_take"),
            true,
        ),
        retained_wrapper_source: patchright_page_binding_wrapper_source(
            "__pw_keptBinding",
            "__lm_pw_kept_binding_deliver",
            None,
            false,
        ),
        retained_handle_wrapper_source: patchright_page_binding_wrapper_source(
            "__pw_keptHandleBinding",
            "__lm_pw_kept_handle_binding_deliver",
            Some("__lm_pw_kept_handle_binding_take"),
            true,
        ),
    }
}

async fn setup_mixed_cleanup_replay_state(ctx: &mut TestContext) -> MixedCleanupReplayState {
    let first =
        create_attached_page_session_without_runtime_enable_async(ctx, 35000, 35001, 35002).await;
    let second =
        create_attached_page_session_without_runtime_enable_async(ctx, 35003, 35004, 35005).await;

    for (id, session_id, html) in [
        (
            35006_u64,
            first.session_id.as_str(),
            "<body><div id='first-main-handle-a'>first-main-handle-a</div><div id='first-main-handle-b'>first-main-handle-b</div><div id='first-utility-handle-a'>first-utility-handle-a</div><div id='first-utility-handle-b'>first-utility-handle-b</div></body>",
        ),
        (
            35007_u64,
            second.session_id.as_str(),
            "<body><div id='second-main-handle-a'>second-main-handle-a</div><div id='second-main-handle-b'>second-main-handle-b</div><div id='second-utility-handle-a'>second-utility-handle-a</div><div id='second-utility-handle-b'>second-utility-handle-b</div></body>",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.navigate",
            "sessionId": session_id,
            "params": { "url": format!("data:text/html,{html}") }
        }))
        .await;
        let navigation = take_response_by_id(ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, target_id, session_id) in [
        (
            35008_u64,
            first.target_id.as_str(),
            first.session_id.as_str(),
        ),
        (
            35009_u64,
            second.target_id.as_str(),
            second.session_id.as_str(),
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Target.detachFromTarget",
            "params": { "targetId": target_id, "sessionId": session_id }
        }))
        .await;
        ctx.expect_result(id, json!({}), None);
        ctx.expect_event(
            "Target.detachedFromTarget",
            Some(&json!({ "targetId": target_id, "sessionId": session_id })),
        );
    }

    ctx.process_async(json!({
        "id": 35010,
        "method": "Target.setAutoAttach",
        "params": { "autoAttach": true, "waitForDebuggerOnStart": false }
    }))
    .await;
    ctx.expect_result(35010, json!({}), None);
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
            35011_u64,
            first_auto_session.as_str(),
            first.target_id.as_str(),
            &mut first_utility_context,
        ),
        (
            35012_u64,
            second_auto_session.as_str(),
            second.target_id.as_str(),
            &mut second_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.createIsolatedWorld",
            "sessionId": session_id,
            "params": { "frameId": target_id, "worldName": "utility" }
        }))
        .await;
        *utility_context = take_response_by_id(ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("utility context id after auto-attach");
        ctx.take_all();
    }

    MixedCleanupReplayState {
        first,
        second,
        first_auto_session,
        second_auto_session,
        first_utility_context,
        second_utility_context,
        first_replay_utility_context: 0,
        second_replay_utility_context: 0,
    }
}

async fn install_mixed_cleanup_bindings_and_scripts(
    ctx: &mut TestContext,
    state: &MixedCleanupReplayState,
    sources: &MixedCleanupSources,
) {
    for (id, session_id, utility_context) in [
        (
            35013_u64,
            state.first_auto_session.as_str(),
            state.first_utility_context,
        ),
        (
            35037_u64,
            state.second_auto_session.as_str(),
            state.second_utility_context,
        ),
    ] {
        for (offset, binding_name, source) in [
            (
                0_u64,
                "customBindingA",
                sources.custom_wrapper_a_source.as_str(),
            ),
            (
                4_u64,
                "customBindingB",
                sources.custom_wrapper_b_source.as_str(),
            ),
            (
                8_u64,
                "customHandleBindingA",
                sources.custom_handle_wrapper_a_source.as_str(),
            ),
            (
                12_u64,
                "customHandleBindingB",
                sources.custom_handle_wrapper_b_source.as_str(),
            ),
            (
                16_u64,
                "__pw_keptBinding",
                sources.retained_wrapper_source.as_str(),
            ),
            (
                20_u64,
                "__pw_keptHandleBinding",
                sources.retained_handle_wrapper_source.as_str(),
            ),
        ] {
            install_patchright_crpage_binding_in_existing_worlds_async(
                ctx,
                session_id,
                utility_context,
                id + offset,
                id + offset + 1,
                id + offset + 2,
                id + offset + 3,
                binding_name,
                source,
            )
            .await;
        }
    }

    for (id, source, world_name, session_id) in vec![
        (
            35061_u64,
            sources.custom_wrapper_a_source.as_str(),
            None,
            state.first_auto_session.as_str(),
        ),
        (
            35062_u64,
            sources.custom_wrapper_b_source.as_str(),
            None,
            state.first_auto_session.as_str(),
        ),
        (
            35063_u64,
            sources.custom_handle_wrapper_a_source.as_str(),
            None,
            state.first_auto_session.as_str(),
        ),
        (
            35064_u64,
            sources.custom_handle_wrapper_b_source.as_str(),
            None,
            state.first_auto_session.as_str(),
        ),
        (
            35065_u64,
            sources.retained_wrapper_source.as_str(),
            None,
            state.first_auto_session.as_str(),
        ),
        (
            35066_u64,
            sources.retained_handle_wrapper_source.as_str(),
            None,
            state.first_auto_session.as_str(),
        ),
        (
            35067_u64,
            sources.custom_wrapper_a_source.as_str(),
            Some("utility"),
            state.first_auto_session.as_str(),
        ),
        (
            35068_u64,
            sources.custom_wrapper_b_source.as_str(),
            Some("utility"),
            state.first_auto_session.as_str(),
        ),
        (
            35069_u64,
            sources.custom_handle_wrapper_a_source.as_str(),
            Some("utility"),
            state.first_auto_session.as_str(),
        ),
        (
            35070_u64,
            sources.custom_handle_wrapper_b_source.as_str(),
            Some("utility"),
            state.first_auto_session.as_str(),
        ),
        (
            35071_u64,
            sources.retained_wrapper_source.as_str(),
            Some("utility"),
            state.first_auto_session.as_str(),
        ),
        (
            35072_u64,
            sources.retained_handle_wrapper_source.as_str(),
            Some("utility"),
            state.first_auto_session.as_str(),
        ),
        (
            35073_u64,
            sources.custom_wrapper_a_source.as_str(),
            None,
            state.second_auto_session.as_str(),
        ),
        (
            35074_u64,
            sources.custom_wrapper_b_source.as_str(),
            None,
            state.second_auto_session.as_str(),
        ),
        (
            35075_u64,
            sources.custom_handle_wrapper_a_source.as_str(),
            None,
            state.second_auto_session.as_str(),
        ),
        (
            35076_u64,
            sources.custom_handle_wrapper_b_source.as_str(),
            None,
            state.second_auto_session.as_str(),
        ),
        (
            35077_u64,
            sources.retained_wrapper_source.as_str(),
            None,
            state.second_auto_session.as_str(),
        ),
        (
            35078_u64,
            sources.retained_handle_wrapper_source.as_str(),
            None,
            state.second_auto_session.as_str(),
        ),
        (
            35079_u64,
            sources.custom_wrapper_a_source.as_str(),
            Some("utility"),
            state.second_auto_session.as_str(),
        ),
        (
            35080_u64,
            sources.custom_wrapper_b_source.as_str(),
            Some("utility"),
            state.second_auto_session.as_str(),
        ),
        (
            35081_u64,
            sources.custom_handle_wrapper_a_source.as_str(),
            Some("utility"),
            state.second_auto_session.as_str(),
        ),
        (
            35082_u64,
            sources.custom_handle_wrapper_b_source.as_str(),
            Some("utility"),
            state.second_auto_session.as_str(),
        ),
        (
            35083_u64,
            sources.retained_wrapper_source.as_str(),
            Some("utility"),
            state.second_auto_session.as_str(),
        ),
        (
            35084_u64,
            sources.retained_handle_wrapper_source.as_str(),
            Some("utility"),
            state.second_auto_session.as_str(),
        ),
    ] {
        let mut params = json!({ "source": source, "runImmediately": true });
        if let Some(world_name) = world_name {
            params["worldName"] = json!(world_name);
        }
        ctx.process_async(json!({
            "id": id,
            "method": "Page.addScriptToEvaluateOnNewDocument",
            "sessionId": session_id,
            "params": params
        }))
        .await;
        assert!(
            take_response_by_id(ctx, id)["result"]["identifier"]
                .as_str()
                .is_some()
        );
    }

    for (id, binding_name) in [
        (35085_u64, "customBindingA"),
        (35086_u64, "customHandleBindingA"),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.removeBinding",
            "sessionId": state.first_auto_session,
            "params": { "name": binding_name }
        }))
        .await;
        assert_eq!(take_response_by_id(ctx, id)["result"], json!({}));
    }
}

async fn replay_mixed_cleanup_pages_and_assert_main_world_state(
    ctx: &mut TestContext,
    state: &MixedCleanupReplayState,
    sources: &MixedCleanupSources,
) {
    for (id, session_id, label) in [
        (35087_u64, state.first_auto_session.as_str(), "first-replay"),
        (
            35088_u64,
            state.second_auto_session.as_str(),
            "second-replay",
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Page.navigate",
                "sessionId": session_id,
                "params": {
                    "url": format!(
                        "data:text/html,<body><div id='main-handle-a'>{label}-main-handle-a</div><div id='main-handle-b'>{label}-main-handle-b</div><div id='utility-handle-a'>{label}-utility-handle-a</div><div id='utility-handle-b'>{label}-utility-handle-b</div></body>"
                    )
                }
            })).await;
        let navigation = take_response_by_id(ctx, id);
        assert_eq!(navigation["sessionId"], json!(session_id));
        ctx.take_all();
    }

    for (id, session_id, source, expected_type) in vec![
        (
            35089_u64,
            state.first_auto_session.as_str(),
            sources.custom_wrapper_a_source.as_str(),
            "undefined",
        ),
        (
            35090_u64,
            state.first_auto_session.as_str(),
            sources.custom_wrapper_b_source.as_str(),
            "function",
        ),
        (
            35091_u64,
            state.first_auto_session.as_str(),
            sources.custom_handle_wrapper_a_source.as_str(),
            "undefined",
        ),
        (
            35092_u64,
            state.first_auto_session.as_str(),
            sources.custom_handle_wrapper_b_source.as_str(),
            "function",
        ),
        (
            35093_u64,
            state.first_auto_session.as_str(),
            sources.retained_wrapper_source.as_str(),
            "function",
        ),
        (
            35094_u64,
            state.first_auto_session.as_str(),
            sources.retained_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            35095_u64,
            state.second_auto_session.as_str(),
            sources.custom_wrapper_a_source.as_str(),
            "function",
        ),
        (
            35096_u64,
            state.second_auto_session.as_str(),
            sources.custom_wrapper_b_source.as_str(),
            "function",
        ),
        (
            35097_u64,
            state.second_auto_session.as_str(),
            sources.custom_handle_wrapper_a_source.as_str(),
            "function",
        ),
        (
            35098_u64,
            state.second_auto_session.as_str(),
            sources.custom_handle_wrapper_b_source.as_str(),
            "function",
        ),
        (
            35099_u64,
            state.second_auto_session.as_str(),
            sources.retained_wrapper_source.as_str(),
            "function",
        ),
        (
            35100_u64,
            state.second_auto_session.as_str(),
            sources.retained_handle_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": { "expression": source, "awaitPromise": true }
        }))
        .await;
        let replayed = take_response_by_id(ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, expected_state) in [
        (
            35101_u64,
            state.first_auto_session.as_str(),
            json!(
                "[\"undefined\",\"function\",\"undefined\",\"function\",\"function\",\"function\"]"
            ),
        ),
        (
            35102_u64,
            state.second_auto_session.as_str(),
            json!(
                "[\"function\",\"function\",\"function\",\"function\",\"function\",\"function\"]"
            ),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customBindingB, typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptBinding, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }
}

async fn setup_replay_utility_worlds_and_assert_state(
    ctx: &mut TestContext,
    state: &mut MixedCleanupReplayState,
    sources: &MixedCleanupSources,
) {
    for (id, session_id, target_id, utility_context) in [
        (
            35103_u64,
            state.first_auto_session.as_str(),
            state.first.target_id.as_str(),
            &mut state.first_replay_utility_context,
        ),
        (
            35104_u64,
            state.second_auto_session.as_str(),
            state.second.target_id.as_str(),
            &mut state.second_replay_utility_context,
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Page.createIsolatedWorld",
            "sessionId": session_id,
            "params": { "frameId": target_id, "worldName": "utility" }
        }))
        .await;
        *utility_context = take_response_by_id(ctx, id)["result"]["executionContextId"]
            .as_i64()
            .expect("replay utility context id");
        ctx.take_all();
    }

    for (id, session_id, utility_context, names) in [
        (
            35105_u64,
            state.first_auto_session.as_str(),
            state.first_replay_utility_context,
            vec![
                "customBindingB",
                "customHandleBindingB",
                "__pw_keptBinding",
                "__pw_keptHandleBinding",
            ],
        ),
        (
            35110_u64,
            state.second_auto_session.as_str(),
            state.second_replay_utility_context,
            vec![
                "customBindingA",
                "customBindingB",
                "customHandleBindingA",
                "customHandleBindingB",
                "__pw_keptBinding",
                "__pw_keptHandleBinding",
            ],
        ),
    ] {
        for (offset, binding_name) in names.iter().enumerate() {
            ctx.process_async(json!({
                "id": id + offset as u64,
                "method": "Runtime.addBinding",
                "sessionId": session_id,
                "params": { "name": binding_name, "executionContextId": utility_context }
            }))
            .await;
            assert_eq!(
                take_response_by_id(ctx, id + offset as u64)["result"],
                json!({})
            );
        }
    }

    for (id, session_id, utility_context, source, expected_type) in vec![
        (
            35120_u64,
            state.first_auto_session.as_str(),
            state.first_replay_utility_context,
            sources.custom_wrapper_a_source.as_str(),
            "undefined",
        ),
        (
            35121_u64,
            state.first_auto_session.as_str(),
            state.first_replay_utility_context,
            sources.custom_wrapper_b_source.as_str(),
            "function",
        ),
        (
            35122_u64,
            state.first_auto_session.as_str(),
            state.first_replay_utility_context,
            sources.custom_handle_wrapper_a_source.as_str(),
            "undefined",
        ),
        (
            35123_u64,
            state.first_auto_session.as_str(),
            state.first_replay_utility_context,
            sources.custom_handle_wrapper_b_source.as_str(),
            "function",
        ),
        (
            35124_u64,
            state.first_auto_session.as_str(),
            state.first_replay_utility_context,
            sources.retained_wrapper_source.as_str(),
            "function",
        ),
        (
            35125_u64,
            state.first_auto_session.as_str(),
            state.first_replay_utility_context,
            sources.retained_handle_wrapper_source.as_str(),
            "function",
        ),
        (
            35126_u64,
            state.second_auto_session.as_str(),
            state.second_replay_utility_context,
            sources.custom_wrapper_a_source.as_str(),
            "function",
        ),
        (
            35127_u64,
            state.second_auto_session.as_str(),
            state.second_replay_utility_context,
            sources.custom_wrapper_b_source.as_str(),
            "function",
        ),
        (
            35128_u64,
            state.second_auto_session.as_str(),
            state.second_replay_utility_context,
            sources.custom_handle_wrapper_a_source.as_str(),
            "function",
        ),
        (
            35129_u64,
            state.second_auto_session.as_str(),
            state.second_replay_utility_context,
            sources.custom_handle_wrapper_b_source.as_str(),
            "function",
        ),
        (
            35130_u64,
            state.second_auto_session.as_str(),
            state.second_replay_utility_context,
            sources.retained_wrapper_source.as_str(),
            "function",
        ),
        (
            35131_u64,
            state.second_auto_session.as_str(),
            state.second_replay_utility_context,
            sources.retained_handle_wrapper_source.as_str(),
            "function",
        ),
    ] {
        ctx.process_async(json!({
            "id": id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": utility_context,
                "expression": source,
                "awaitPromise": true
            }
        }))
        .await;
        let replayed = take_response_by_id(ctx, id);
        assert_eq!(replayed["result"]["result"]["value"], json!(expected_type));
    }

    for (id, session_id, utility_context, expected_state) in [
        (
            35132_u64,
            state.first_auto_session.as_str(),
            state.first_replay_utility_context,
            json!(
                "[\"undefined\",\"function\",\"undefined\",\"function\",\"function\",\"function\"]"
            ),
        ),
        (
            35133_u64,
            state.second_auto_session.as_str(),
            state.second_replay_utility_context,
            json!(
                "[\"function\",\"function\",\"function\",\"function\",\"function\",\"function\"]"
            ),
        ),
    ] {
        ctx.process_async(json!({
                "id": id,
                "method": "Runtime.evaluate",
                "sessionId": session_id,
                "params": {
                    "contextId": utility_context,
                    "expression": "JSON.stringify([typeof globalThis.customBindingA, typeof globalThis.customBindingB, typeof globalThis.customHandleBindingA, typeof globalThis.customHandleBindingB, typeof globalThis.__pw_keptBinding, typeof globalThis.__pw_keptHandleBinding])"
                }
            })).await;
        let state = take_response_by_id(ctx, id);
        assert_eq!(state["result"]["result"]["value"], expected_state);
    }
}

async fn verify_mixed_cleanup_replayed_value_bindings(
    ctx: &mut TestContext,
    state: &MixedCleanupReplayState,
) {
    for (
        id,
        session_id,
        context_id,
        expression,
        binding_name,
        expected_payload,
        deliver_expression,
        promise_name,
        expected_result,
    ) in [
        (
            35134_u64,
            state.first_auto_session.as_str(),
            None,
            "globalThis.__lm_first_replay_custom_b = customBindingB({ source: 'first-replay-custom-b', nested: { count: 1, values: ['a', 2, true] } }); 'scheduled-first-custom-b'",
            "customBindingB",
            json!([{
                "source": "first-replay-custom-b",
                "nested": { "count": 1, "values": ["a", 2, true] }
            }]),
            "globalThis.__lm_custom_binding_b_deliver({ name: 'customBindingB', seq: 1, result: 'first-replay-custom-b-ok' }); 'delivered'",
            "__lm_first_replay_custom_b",
            "first-replay-custom-b-ok",
        ),
        (
            35135_u64,
            state.first_auto_session.as_str(),
            Some(state.first_replay_utility_context),
            "globalThis.__lm_first_replay_pw = __pw_keptBinding({ source: 'first-replay-pw', nested: { count: 2, values: ['b', 3, false] } }); 'scheduled-first-pw'",
            "__pw_keptBinding",
            json!([{
                "source": "first-replay-pw",
                "nested": { "count": 2, "values": ["b", 3, false] }
            }]),
            "globalThis.__lm_pw_kept_binding_deliver({ name: '__pw_keptBinding', seq: 1, result: 'first-replay-pw-ok' }); 'delivered'",
            "__lm_first_replay_pw",
            "first-replay-pw-ok",
        ),
        (
            35136_u64,
            state.second_auto_session.as_str(),
            Some(state.second_replay_utility_context),
            "globalThis.__lm_second_replay_custom_a = customBindingA({ source: 'second-replay-custom-a', nested: { count: 3, values: ['c', 4, true] } }); 'scheduled-second-custom-a'",
            "customBindingA",
            json!([{
                "source": "second-replay-custom-a",
                "nested": { "count": 3, "values": ["c", 4, true] }
            }]),
            "globalThis.__lm_custom_binding_a_deliver({ name: 'customBindingA', seq: 1, result: 'second-replay-custom-a-ok' }); 'delivered'",
            "__lm_second_replay_custom_a",
            "second-replay-custom-a-ok",
        ),
    ] {
        let mut params = json!({ "expression": expression, "awaitPromise": true });
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
        let scheduled = take_response_by_id(ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled value")
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
            .expect("binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        assert_eq!(payload["serializedArgs"], expected_payload);
        assert_eq!(payload["seq"], json!(1));
        ctx.sent.clear();

        let mut deliver_params = json!({ "expression": deliver_expression, "awaitPromise": true });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(ctx, id + 10);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 20,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(ctx, id + 20);
        assert_eq!(
            resolved["result"]["result"]["value"],
            json!(expected_result)
        );
    }
}

async fn verify_mixed_cleanup_replayed_handle_bindings(
    ctx: &mut TestContext,
    state: &MixedCleanupReplayState,
) {
    for (
        id,
        session_id,
        context_id,
        expression,
        binding_name,
        take_helper,
        expected_id,
        expected_text,
        deliver_helper,
        promise_name,
        expected_result,
    ) in [
        (
            35137_u64,
            state.first_auto_session.as_str(),
            Some(state.first_replay_utility_context),
            "globalThis.__lm_first_replay_custom_handle_b = customHandleBindingB(document.getElementById('utility-handle-b')); 'scheduled-first-custom-handle-b'",
            "customHandleBindingB",
            "__lm_custom_handle_binding_b_take",
            "utility-handle-b",
            "first-replay-utility-handle-b",
            "__lm_custom_handle_binding_b_deliver",
            "__lm_first_replay_custom_handle_b",
            "first-replay-custom-handle-b-ok",
        ),
        (
            35138_u64,
            state.second_auto_session.as_str(),
            None,
            "globalThis.__lm_second_replay_custom_handle_a = customHandleBindingA(document.getElementById('main-handle-a')); 'scheduled-second-custom-handle-a'",
            "customHandleBindingA",
            "__lm_custom_handle_binding_a_take",
            "main-handle-a",
            "second-replay-main-handle-a",
            "__lm_custom_handle_binding_a_deliver",
            "__lm_second_replay_custom_handle_a",
            "second-replay-custom-handle-a-ok",
        ),
    ] {
        let mut params = json!({ "expression": expression, "awaitPromise": true });
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
        let scheduled = take_response_by_id(ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled replay handle value")
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
            .expect("replayed handle binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        let seq = payload["seq"]
            .as_i64()
            .expect("binding payload seq should be an integer");
        assert_eq!(seq, 1);
        ctx.sent.clear();

        let mut take_params = json!({
            "expression": format!(
                "(() => {{ const handle = globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.{take_helper}({{ name: '{binding_name}', seq: {seq} }})]); }})()"
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
        let taken = take_response_by_id(ctx, id + 10);
        assert_eq!(
            taken["result"]["result"]["value"],
            json!(format!(
                "[\"{expected_id}\",\"{expected_text}\",\"undefined\"]"
            ))
        );

        let mut deliver_params = json!({
            "expression": format!(
                "globalThis.{deliver_helper}({{ name: '{binding_name}', seq: {seq}, result: '{expected_result}' }}); 'delivered'"
            ),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 20,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(ctx, id + 20);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 30,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(ctx, id + 30);
        assert_eq!(
            resolved["result"]["result"]["value"],
            json!(expected_result)
        );
    }
}

async fn verify_mixed_cleanup_replayed_rejections(
    ctx: &mut TestContext,
    state: &MixedCleanupReplayState,
) {
    for (
        id,
        session_id,
        context_id,
        expression,
        binding_name,
        expected_payload,
        deliver_expression,
        promise_name,
        expected_result,
    ) in [
        (
            35139_u64,
            state.first_auto_session.as_str(),
            None::<i64>,
            "globalThis.__lm_first_replay_custom_b_reject = customBindingB({ source: 'first-replay-custom-b-reject', nested: { count: 4, values: ['d', 5, false] } }).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-first-custom-b-reject'",
            "customBindingB",
            json!([{
                "source": "first-replay-custom-b-reject",
                "nested": { "count": 4, "values": ["d", 5, false] }
            }]),
            "globalThis.__lm_custom_binding_b_deliver({ name: 'customBindingB', seq: 2, error: 'first-replay-custom-b-error' }); 'delivered'",
            "__lm_first_replay_custom_b_reject",
            "rejected:first-replay-custom-b-error",
        ),
        (
            35140_u64,
            state.second_auto_session.as_str(),
            None::<i64>,
            "globalThis.__lm_second_replay_custom_handle_b_reject = customHandleBindingB(document.getElementById('main-handle-b')).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-second-custom-handle-b-reject'",
            "customHandleBindingB",
            json!({ "name": "customHandleBindingB", "seq": 1 }),
            "globalThis.__lm_custom_handle_binding_b_deliver({ name: 'customHandleBindingB', seq: 1, error: 'second-replay-custom-handle-b-error' }); 'delivered'",
            "__lm_second_replay_custom_handle_b_reject",
            "rejected:second-replay-custom-handle-b-error",
        ),
    ] {
        let mut params = json!({ "expression": expression, "awaitPromise": true });
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
        let scheduled = take_response_by_id(ctx, id);
        assert!(
            scheduled["result"]["result"]["value"]
                .as_str()
                .expect("scheduled rejection value")
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
            .expect("rejection binding should emit Runtime.bindingCalled");
        let payload = binding_called["params"]["payload"]
            .as_str()
            .expect("binding payload should be a json string");
        let payload: serde_json::Value =
            serde_json::from_str(payload).expect("binding payload should be valid json");
        assert_eq!(payload["name"], json!(binding_name));
        assert_eq!(
            if binding_name == "customBindingB" {
                payload["serializedArgs"].clone()
            } else {
                json!({ "name": payload["name"], "seq": payload["seq"] })
            },
            expected_payload
        );
        ctx.sent.clear();

        let mut deliver_params = json!({ "expression": deliver_expression, "awaitPromise": true });
        if let Some(context_id) = context_id {
            deliver_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 10,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": deliver_params
        }))
        .await;
        let delivered = take_response_by_id(ctx, id + 10);
        assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

        let mut promise_params = json!({
            "expression": format!("globalThis.{promise_name}"),
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            promise_params["contextId"] = json!(context_id);
        }
        ctx.process_async(json!({
            "id": id + 20,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": promise_params
        }))
        .await;
        let resolved = take_response_by_id(ctx, id + 20);
        assert_eq!(
            resolved["result"]["result"]["value"],
            json!(expected_result)
        );
    }

    let session_id = state.first_auto_session.as_str();
    let context_id = state.first_replay_utility_context;
    ctx.process_async(json!({
            "id": 35141,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": context_id,
                "expression": "globalThis.__lm_first_replay_pw_handle_reject = __pw_keptHandleBinding(document.getElementById('utility-handle-b')).then(() => 'unexpected-success', error => `rejected:${error}`); 'scheduled-first-pw-handle-reject'",
                "awaitPromise": true
            }
        })).await;
    let scheduled = take_response_by_id(ctx, 35141);
    assert!(
        scheduled["result"]["result"]["value"]
            .as_str()
            .expect("scheduled retained handle rejection value")
            .starts_with("scheduled-")
    );

    let binding_called = ctx
        .sent
        .iter()
        .rev()
        .find(|message| {
            message["method"] == json!("Runtime.bindingCalled")
                && message["sessionId"] == json!(session_id)
                && message["params"]["name"] == json!("__pw_keptHandleBinding")
                && message["params"]["executionContextId"] == json!(context_id)
        })
        .cloned()
        .expect("retained handle binding should emit Runtime.bindingCalled");
    let payload = binding_called["params"]["payload"]
        .as_str()
        .expect("binding payload should be string");
    let payload: serde_json::Value =
        serde_json::from_str(payload).expect("binding payload should be valid json");
    let seq = payload["seq"]
        .as_i64()
        .expect("binding payload seq should be an integer");
    assert_eq!(seq, 1);
    ctx.sent.clear();

    ctx.process_async(json!({
            "id": 35142,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": context_id,
                "expression": format!(
                    "(() => {{ const handle = globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {seq} }}); return JSON.stringify([handle.id, handle.textContent, typeof globalThis.__lm_pw_kept_handle_binding_take({{ name: '__pw_keptHandleBinding', seq: {seq} }})]); }})()"
                )
            }
        })).await;
    let taken = take_response_by_id(ctx, 35142);
    assert_eq!(
        taken["result"]["result"]["value"],
        json!("[\"utility-handle-b\",\"first-replay-utility-handle-b\",\"undefined\"]")
    );

    ctx.process_async(json!({
            "id": 35143,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": {
                "contextId": context_id,
                "expression": format!(
                    "globalThis.__lm_pw_kept_handle_binding_deliver({{ name: '__pw_keptHandleBinding', seq: {seq}, error: 'first-replay-pw-handle-error' }}); 'delivered'"
                ),
                "awaitPromise": true
            }
        })).await;
    let delivered = take_response_by_id(ctx, 35143);
    assert_eq!(delivered["result"]["result"]["value"], json!("delivered"));

    ctx.process_async(json!({
        "id": 35144,
        "method": "Runtime.evaluate",
        "sessionId": session_id,
        "params": {
            "contextId": context_id,
            "expression": "globalThis.__lm_first_replay_pw_handle_reject",
            "awaitPromise": true
        }
    }))
    .await;
    let rejected = take_response_by_id(ctx, 35144);
    assert_eq!(
        rejected["result"]["result"]["value"],
        json!("rejected:first-replay-pw-handle-error")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn patchright_over_cdp_auto_attach_sweep_crpage_cleanup_replay_only_clears_matching_names_across_mixed_binding_kinds_in_cleaned_context()
 {
    // Keep this scenario split across helper futures. The end-to-end case is
    // intentionally large, and one monolithic async test overflows libtest's stack
    // before it exercises the cleanup/replay behavior we actually care about.
    let mut ctx = TestContext::new();
    let mut state = setup_mixed_cleanup_replay_state(&mut ctx).await;
    let sources = mixed_cleanup_sources();

    install_mixed_cleanup_bindings_and_scripts(&mut ctx, &state, &sources).await;
    replay_mixed_cleanup_pages_and_assert_main_world_state(&mut ctx, &state, &sources).await;
    setup_replay_utility_worlds_and_assert_state(&mut ctx, &mut state, &sources).await;
    verify_mixed_cleanup_replayed_value_bindings(&mut ctx, &state).await;
    verify_mixed_cleanup_replayed_handle_bindings(&mut ctx, &state).await;
    verify_mixed_cleanup_replayed_rejections(&mut ctx, &state).await;
}
