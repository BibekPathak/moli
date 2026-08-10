use super::super::tests_cdp_smoke_fixture::SmokeFixtureServer;
use super::super::*;
use super::support::CdpPageHarness;
use serde_json::json;

// P0 browser contract source:
// Chromium Input.dispatchKeyEvent plus Playwright keyboard typing basics.
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_p0_input_dispatch_key_event_inserts_text() {
    let fixture = SmokeFixtureServer::start().await;
    let mut ctx = TestContext::new_with_target_discovery(false);
    let page = CdpPageHarness::attach(&mut ctx, 138_000).await;

    page.navigate(&mut ctx, 138_005, fixture.url("/plain?input-key"))
        .await;
    page.evaluate_value(
        &mut ctx,
        138_006,
        r#"
            document.body.innerHTML = '<input id="field" value="">';
            window.__keyEvents = [];
            const field = document.getElementById('field');
            field.addEventListener('keydown', event => {
                window.__keyEvents.push({
                    type: event.type,
                    key: event.key,
                    code: event.code,
                    shiftKey: event.shiftKey,
                });
            });
            field.focus();
            'ready'
        "#,
    )
    .await;

    page.expect_empty_command(
        &mut ctx,
        138_007,
        "Input.dispatchKeyEvent",
        json!({
            "type": "keyDown",
            "key": "A",
            "code": "KeyA",
            "modifiers": 8,
            "text": "A"
        }),
    )
    .await;

    assert_eq!(
        page.evaluate_string(&mut ctx, 138_008, "document.getElementById('field').value")
            .await,
        "A"
    );
    assert_eq!(
        page.evaluate_string(&mut ctx, 138_009, "JSON.stringify(window.__keyEvents)")
            .await,
        r#"[{"type":"keydown","key":"A","code":"KeyA","shiftKey":true}]"#
    );
}
