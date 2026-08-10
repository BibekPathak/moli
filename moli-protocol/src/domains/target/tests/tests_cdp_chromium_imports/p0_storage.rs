use super::super::*;
use super::support::CdpPageHarness;
use serde_json::json;

// P0 browser contract source:
// Chromium Storage.clearDataForOrigin cookie visibility.
#[tokio::test(flavor = "multi_thread")]
async fn rust_cdp_p0_storage_clear_data_for_origin_clears_matching_cookies() {
    let mut ctx = TestContext::new_with_target_discovery(false);
    CdpPageHarness::attach(&mut ctx, 139_000).await;

    ctx.process_async(json!({
        "id": 139_005,
        "method": "Storage.setCookies",
        "params": {
            "cookies": [
                { "name": "host", "value": "1", "url": "https://app.example.com/app" },
                { "name": "shared", "value": "1", "domain": "example.com", "path": "/" },
                { "name": "sibling", "value": "1", "url": "https://cdn.example.com/app" },
                { "name": "other", "value": "1", "url": "https://foo.co.uk/app" }
            ]
        }
    }))
    .await;
    ctx.expect_result(139_005, json!({}), None);

    ctx.process_async(json!({
        "id": 139_006,
        "method": "Storage.clearDataForOrigin",
        "params": {
            "origin": "https://app.example.com",
            "storageTypes": "cookies,local_storage"
        }
    }))
    .await;
    ctx.expect_result(139_006, json!({}), None);

    ctx.process_async(json!({
        "id": 139_007,
        "method": "Storage.getCookies"
    }))
    .await;
    let response = take_response_by_id(&mut ctx, 139_007);
    let names = response["result"]["cookies"]
        .as_array()
        .expect("storage cookies")
        .iter()
        .map(|cookie| cookie["name"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["sibling", "other"]);
}
