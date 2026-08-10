use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use moli_core::{
    RendererRuntimeInspectorResponseSender,
    page::{
        DocumentNodeClientRectResolution, DocumentNodeRuntimeObjectResolution,
        DocumentNodeSnapshot, Page, PageInputExt, RendererDocumentNodeAttributesResolution,
        RendererDocumentNodeTextResolution, RendererDocumentQuerySelectorNode,
        RendererDocumentQuerySelectorResolution,
    },
    runtime::{Browser, RenderedDomWaitUntil},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{
    self, AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};

use crate::{
    cli::{DumpFormat, StripOptions},
    fetch_dump::{render_page_dump_with_options_async, summarize_node_details_async},
};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

pub async fn serve_stdio(browser: Browser) -> Result<()> {
    let mut server = McpServer::new(browser);
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut writer = stdout;

    while let Some(message) = read_message(&mut reader).await? {
        let response = match serde_json::from_slice::<Value>(&message) {
            Ok(request) => server.handle_request(request).await,
            Err(error) => Some(error_response(
                Value::Null,
                -32700,
                format!("parse error: {error}"),
            )),
        };

        if let Some(response) = response {
            write_message(&mut writer, &response).await?;
        }
    }

    Ok(())
}

struct McpServer {
    browser: Browser,
    current_page: Option<Page>,
}

impl McpServer {
    fn new(browser: Browser) -> Self {
        Self {
            browser,
            current_page: None,
        }
    }

    async fn handle_request(&mut self, request: Value) -> Option<Value> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str)?;
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "initialize" => Some(success_response(
                id,
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {},
                        "resources": {}
                    },
                    "serverInfo": {
                        "name": "moli",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            )),
            "notifications/initialized" => None,
            "ping" => Some(success_response(id, json!({}))),
            "tools/list" => Some(success_response(id, json!({ "tools": tool_list() }))),
            "resources/list" => Some(success_response(
                id,
                json!({ "resources": resource_list() }),
            )),
            "resources/read" => Some(match self.handle_resource_read(params).await {
                Ok(result) => success_response(id, result),
                Err(error) => error_response(id, -32000, error.to_string()),
            }),
            "tools/call" => Some(match self.handle_tool_call(params).await {
                Ok(result) => success_response(id, result),
                Err(error) => success_response(id, tool_error(error.to_string())),
            }),
            _ => Some(error_response(
                id,
                -32601,
                format!("method `{method}` not found"),
            )),
        }
    }

    async fn handle_resource_read(&mut self, params: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct ResourceReadParams {
            uri: String,
        }

        let params: ResourceReadParams =
            serde_json::from_value(params).context("invalid resources/read params")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;

        let (mime_type, text) = match params.uri.as_str() {
            "mcp://page/html" => (
                "text/html",
                render_page_dump_with_options_async(
                    page,
                    DumpFormat::Html,
                    StripOptions::default(),
                    false,
                    false,
                    false,
                    None,
                )
                .await?,
            ),
            "mcp://page/markdown" => (
                "text/markdown",
                render_page_dump_with_options_async(
                    page,
                    DumpFormat::Markdown,
                    StripOptions::default(),
                    false,
                    false,
                    false,
                    None,
                )
                .await?,
            ),
            _ => return Err(anyhow!("resource `{}` not found", params.uri)),
        };

        Ok(json!({
            "contents": [{
                "uri": params.uri,
                "mimeType": mime_type,
                "text": text
            }]
        }))
    }

    async fn handle_tool_call(&mut self, params: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct ToolCallParams {
            name: String,
            #[serde(default)]
            arguments: Value,
        }

        let params: ToolCallParams =
            serde_json::from_value(params).context("invalid tools/call params")?;
        match params.name.as_str() {
            "goto" | "navigate" => self.tool_goto(params.arguments).await,
            "markdown" => self.tool_markdown(params.arguments).await,
            "links" => self.tool_links(params.arguments).await,
            "evaluate" | "eval" => self.tool_evaluate(params.arguments).await,
            "semantic_tree" => self.tool_semantic_tree(params.arguments).await,
            "nodeDetails" => self.tool_node_details(params.arguments).await,
            "interactiveElements" => self.tool_interactive_elements(params.arguments).await,
            "findElement" => self.tool_find_element(params.arguments).await,
            "waitForSelector" => self.tool_wait_for_selector(params.arguments).await,
            "click" => self.tool_click(params.arguments).await,
            "fill" => self.tool_fill(params.arguments).await,
            "press" => self.tool_press(params.arguments).await,
            "hover" => self.tool_hover(params.arguments).await,
            "scroll" => self.tool_scroll(params.arguments).await,
            "selectOption" => self.tool_select_option(params.arguments).await,
            "setChecked" => self.tool_set_checked(params.arguments).await,
            other => Err(anyhow!("tool `{other}` not found")),
        }
    }

    async fn tool_goto(&mut self, arguments: Value) -> Result<Value> {
        let args: NavigateParams =
            serde_json::from_value(arguments).context("invalid goto arguments")?;
        let page = self
            .navigate_to(
                &args.url,
                args.timeout.unwrap_or(10_000),
                parse_wait_until(args.wait_until.as_deref())?,
            )
            .await?;
        tool_text(format!(
            "Navigated to {} ({})",
            page.final_url(),
            page.document_title()
        ))
    }

    async fn tool_markdown(&mut self, arguments: Value) -> Result<Value> {
        let args: OptionalNavigateParams =
            serde_json::from_value(arguments).context("invalid markdown arguments")?;
        let page = self
            .ensure_page(
                args.url.as_deref(),
                args.timeout,
                args.wait_until.as_deref(),
            )
            .await?;
        let markdown = render_page_dump_with_options_async(
            page,
            DumpFormat::Markdown,
            StripOptions::default(),
            false,
            false,
            false,
            None,
        )
        .await?;
        tool_text(markdown)
    }

    async fn tool_links(&mut self, arguments: Value) -> Result<Value> {
        let args: OptionalNavigateParams =
            serde_json::from_value(arguments).context("invalid links arguments")?;
        let page = self
            .ensure_page(
                args.url.as_deref(),
                args.timeout,
                args.wait_until.as_deref(),
            )
            .await?;
        let nodes = query_selector_all_live(page, "a[href]").await?;
        let mut links = Vec::new();
        for node in nodes {
            let backend_node_id = node.backend_node_id;
            let Some(attributes) = node_attributes_live(page, backend_node_id).await? else {
                continue;
            };
            let href = attribute_value_from_pairs(&attributes, "href").unwrap_or_default();
            if href.is_empty() {
                continue;
            }
            let text = node_text_live(page, backend_node_id)
                .await?
                .unwrap_or_default()
                .trim()
                .to_owned();
            links.push(json!({
                "backendNodeId": backend_node_id,
                "href": href,
                "text": text,
            }));
        }
        tool_json(json!(links))
    }

    async fn tool_evaluate(&mut self, arguments: Value) -> Result<Value> {
        let args: EvaluateParams =
            serde_json::from_value(arguments).context("invalid evaluate arguments")?;
        let page = self
            .ensure_page(
                args.url.as_deref(),
                args.timeout,
                args.wait_until.as_deref(),
            )
            .await?;
        let result = page
            .evaluate_runtime_expression_with_await_async(&args.script, true)
            .await
            .context("script evaluation failed")?;
        tool_json(result)
    }

    async fn tool_semantic_tree(&mut self, arguments: Value) -> Result<Value> {
        let args: SemanticTreeParams =
            serde_json::from_value(arguments).context("invalid semantic_tree arguments")?;
        let page = self
            .ensure_page(
                args.url.as_deref(),
                args.timeout,
                args.wait_until.as_deref(),
            )
            .await?;
        let payloads = match args.backend_node_id {
            Some(backend_node_id) => {
                accessibility_tree_payloads_for_backend_node_id_live(
                    page,
                    backend_node_id,
                    args.max_depth,
                )
                .await?
            }
            None => {
                page.accessibility_tree_payloads_for_document_async(args.max_depth)
                    .await?
            }
        };
        let rendered = render_semantic_tree_text(&payloads);
        Ok(json!({
            "content": [{
                "type": "text",
                "text": rendered
            }],
            "structuredContent": payloads
        }))
    }

    async fn tool_node_details(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct NodeDetailsParams {
            #[serde(rename = "backendNodeId")]
            backend_node_id: u32,
        }

        let args: NodeDetailsParams =
            serde_json::from_value(arguments).context("invalid nodeDetails arguments")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        let details = summarize_node_details_async(page, args.backend_node_id).await?;
        tool_json(details)
    }

    async fn tool_interactive_elements(&mut self, arguments: Value) -> Result<Value> {
        let args: OptionalNavigateParams =
            serde_json::from_value(arguments).context("invalid interactiveElements arguments")?;
        let page = self
            .ensure_page(
                args.url.as_deref(),
                args.timeout,
                args.wait_until.as_deref(),
            )
            .await?;
        tool_json(json!(collect_interactive_elements(page).await?))
    }

    async fn tool_find_element(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct FindElementParams {
            role: Option<String>,
            name: Option<String>,
        }

        let args: FindElementParams =
            serde_json::from_value(arguments).context("invalid findElement arguments")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        let role_filter = args.role.as_deref().map(str::to_ascii_lowercase);
        let name_filter = args.name.as_deref().map(str::to_ascii_lowercase);
        let matches = collect_interactive_elements(page)
            .await?
            .into_iter()
            .filter(|item| {
                let role_ok = role_filter.as_deref().is_none_or(|role| {
                    item.get("role")
                        .and_then(Value::as_str)
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(role))
                });
                let name_ok = name_filter.as_deref().is_none_or(|needle| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(needle)
                });
                role_ok && name_ok
            })
            .collect::<Vec<_>>();
        tool_json(json!(matches))
    }

    async fn tool_wait_for_selector(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct WaitForSelectorParams {
            selector: String,
            timeout: Option<u64>,
        }

        let args: WaitForSelectorParams =
            serde_json::from_value(arguments).context("invalid waitForSelector arguments")?;
        let timeout = Duration::from_millis(args.timeout.unwrap_or(5_000));
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        let node = self
            .browser
            .wait_for_selector(page, &args.selector, timeout)
            .await?;
        tool_json(json!({
            "backendNodeId": node.backend_node_id,
            "selector": args.selector
        }))
    }

    async fn tool_click(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct ClickParams {
            #[serde(rename = "backendNodeId")]
            backend_node_id: u32,
        }

        let args: ClickParams =
            serde_json::from_value(arguments).context("invalid click arguments")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        click_backend_node_center_async(page, args.backend_node_id).await?;
        tool_json(json!({
            "backendNodeId": args.backend_node_id,
            "url": page.final_url(),
            "title": page.document_title()
        }))
    }

    async fn tool_fill(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct FillParams {
            #[serde(rename = "backendNodeId")]
            backend_node_id: u32,
            text: String,
        }

        let args: FillParams =
            serde_json::from_value(arguments).context("invalid fill arguments")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        click_backend_node_center_async(page, args.backend_node_id).await?;
        page.evaluate_runtime_expression_async(
            "(() => { const el = document.activeElement; if (!el) return false; if ('value' in el) { el.value = ''; } return true; })()",
        )
        .await?;
        if !page
            .insert_text_into_active_control_async(&args.text)
            .await?
        {
            return Err(anyhow!("failed to insert text into the active control"));
        }
        let value = page
            .evaluate_runtime_expression_async(
                "(() => { const el = document.activeElement; return el && 'value' in el ? String(el.value) : ''; })()",
            )
            .await?
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        tool_json(json!({
            "backendNodeId": args.backend_node_id,
            "value": value,
            "url": page.final_url(),
            "title": page.document_title()
        }))
    }

    async fn tool_press(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct PressParams {
            key: String,
            #[serde(rename = "backendNodeId")]
            backend_node_id: Option<u32>,
        }

        let args: PressParams =
            serde_json::from_value(arguments).context("invalid press arguments")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        if let Some(backend_node_id) = args.backend_node_id {
            click_backend_node_center_async(page, backend_node_id).await?;
        }
        let key_spec = key_spec(&args.key);
        page.dispatch_key_event_async(
            "keydown",
            key_spec.key,
            key_spec.code,
            key_spec.text,
            0,
            false,
            !key_spec.text.is_empty(),
        )
        .await?;
        page.dispatch_key_event_async("keyup", key_spec.key, key_spec.code, "", 0, false, false)
            .await?;
        tool_json(json!({
            "key": args.key,
            "url": page.final_url(),
            "title": page.document_title()
        }))
    }

    async fn tool_hover(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct HoverParams {
            #[serde(rename = "backendNodeId")]
            backend_node_id: u32,
        }

        let args: HoverParams =
            serde_json::from_value(arguments).context("invalid hover arguments")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        call_function_on_backend_node_async(
            page,
            args.backend_node_id,
            r#"function() {
                const rect = this.getBoundingClientRect();
                const init = {
                    bubbles: true,
                    cancelable: true,
                    clientX: rect.left + rect.width / 2,
                    clientY: rect.top + rect.height / 2
                };
                this.dispatchEvent(new MouseEvent('mouseover', init));
                this.dispatchEvent(new MouseEvent('mouseenter', init));
                this.dispatchEvent(new MouseEvent('mousemove', init));
                return true;
            }"#,
            vec![],
        )
        .await?;
        tool_json(json!({
            "backendNodeId": args.backend_node_id,
            "url": page.final_url(),
            "title": page.document_title()
        }))
    }

    async fn tool_scroll(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct ScrollParams {
            #[serde(rename = "backendNodeId")]
            backend_node_id: Option<u32>,
            x: Option<i32>,
            y: Option<i32>,
        }

        let args: ScrollParams =
            serde_json::from_value(arguments).context("invalid scroll arguments")?;
        let x = args.x.unwrap_or(0);
        let y = args.y.unwrap_or(0);
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;

        let positions = if let Some(backend_node_id) = args.backend_node_id {
            call_function_on_backend_node_async(
                page,
                backend_node_id,
                r#"function(x, y) {
                    this.scrollLeft = x;
                    this.scrollTop = y;
                    this.dispatchEvent(new Event('scroll'));
                    return { x: this.scrollLeft, y: this.scrollTop };
                }"#,
                vec![json!({ "value": x }), json!({ "value": y })],
            )
            .await?
        } else {
            let payload = page
                .evaluate_runtime_expression_async(&format!(
                "(() => {{ window.scrollTo({x}, {y}); window.dispatchEvent(new Event('scroll')); return {{ x: window.scrollX, y: window.scrollY }}; }})()"
            ))
                .await?;
            payload.get("value").cloned().unwrap_or(payload)
        };

        tool_json(json!({
            "position": positions,
            "url": page.final_url(),
            "title": page.document_title()
        }))
    }

    async fn tool_select_option(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct SelectOptionParams {
            #[serde(rename = "backendNodeId")]
            backend_node_id: u32,
            value: String,
        }

        let args: SelectOptionParams =
            serde_json::from_value(arguments).context("invalid selectOption arguments")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        let result = call_function_on_backend_node_async(
            page,
            args.backend_node_id,
            r#"function(value) {
                if (!(this instanceof HTMLSelectElement)) {
                    throw new TypeError('Node is not a <select> element');
                }
                this.value = value;
                this.dispatchEvent(new Event('input', { bubbles: true }));
                this.dispatchEvent(new Event('change', { bubbles: true }));
                return { value: this.value, selectedIndex: this.selectedIndex };
            }"#,
            vec![json!({ "value": args.value })],
        )
        .await?;
        tool_json(json!({
            "backendNodeId": args.backend_node_id,
            "selection": result,
            "url": page.final_url(),
            "title": page.document_title()
        }))
    }

    async fn tool_set_checked(&mut self, arguments: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct SetCheckedParams {
            #[serde(rename = "backendNodeId")]
            backend_node_id: u32,
            checked: bool,
        }

        let args: SetCheckedParams =
            serde_json::from_value(arguments).context("invalid setChecked arguments")?;
        let page = self
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        let result = call_function_on_backend_node_async(
            page,
            args.backend_node_id,
            r#"function(checked) {
                if (!(this instanceof HTMLInputElement) || !['checkbox', 'radio'].includes(this.type)) {
                    throw new TypeError('Node is not a checkbox or radio input');
                }
                this.checked = checked;
                this.dispatchEvent(new Event('input', { bubbles: true }));
                this.dispatchEvent(new Event('change', { bubbles: true }));
                this.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
                return { checked: this.checked, type: this.type };
            }"#,
            vec![json!({ "value": args.checked })],
        )
        .await?;
        tool_json(json!({
            "backendNodeId": args.backend_node_id,
            "state": result,
            "url": page.final_url(),
            "title": page.document_title()
        }))
    }

    async fn ensure_page(
        &mut self,
        url: Option<&str>,
        timeout: Option<u64>,
        wait_until: Option<&str>,
    ) -> Result<&mut Page> {
        if let Some(url) = url {
            let timeout = timeout.unwrap_or(10_000);
            let wait_until = parse_wait_until(wait_until)?;
            return self.navigate_to(url, timeout, wait_until).await;
        }

        self.current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))
    }

    async fn navigate_to(
        &mut self,
        url: &str,
        timeout_ms: u64,
        wait_until: RenderedDomWaitUntil,
    ) -> Result<&mut Page> {
        let timeout = Duration::from_millis(timeout_ms);
        let page = self
            .browser
            .fetch_with_wait_until(url, wait_until, timeout)
            .await
            .with_context(|| format!("failed to navigate to `{url}`"))?;
        self.current_page = Some(page);
        self.current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page navigation failed"))
    }
}

#[derive(Deserialize)]
struct NavigateParams {
    url: String,
    timeout: Option<u64>,
    #[serde(rename = "waitUntil")]
    wait_until: Option<String>,
}

#[derive(Deserialize)]
struct OptionalNavigateParams {
    url: Option<String>,
    timeout: Option<u64>,
    #[serde(rename = "waitUntil")]
    wait_until: Option<String>,
}

#[derive(Deserialize)]
struct EvaluateParams {
    script: String,
    url: Option<String>,
    timeout: Option<u64>,
    #[serde(rename = "waitUntil")]
    wait_until: Option<String>,
}

#[derive(Deserialize)]
struct SemanticTreeParams {
    url: Option<String>,
    timeout: Option<u64>,
    #[serde(rename = "waitUntil")]
    wait_until: Option<String>,
    #[serde(rename = "backendNodeId")]
    backend_node_id: Option<u32>,
    #[serde(rename = "maxDepth")]
    max_depth: Option<i32>,
}

fn tool_list() -> Vec<Value> {
    vec![
        json!({
            "name": "goto",
            "description": "Navigate to a URL and keep the page loaded for later MCP calls.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "timeout": { "type": "integer", "description": "Timeout in milliseconds. Defaults to 10000." },
                    "waitUntil": { "type": "string", "enum": ["domcontentloaded", "load", "networkidle", "done"], "description": "Wait strategy. Defaults to done." }
                },
                "required": ["url"]
            }
        }),
        json!({
            "name": "navigate",
            "description": "Alias for goto.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "timeout": { "type": "integer" },
                    "waitUntil": { "type": "string", "enum": ["domcontentloaded", "load", "networkidle", "done"] }
                },
                "required": ["url"]
            }
        }),
        json!({
            "name": "markdown",
            "description": "Return the current page as markdown. Optionally navigate first.",
            "inputSchema": url_tool_schema()
        }),
        json!({
            "name": "links",
            "description": "Extract anchors from the current page. Optionally navigate first.",
            "inputSchema": url_tool_schema()
        }),
        json!({
            "name": "evaluate",
            "description": "Evaluate a JavaScript expression in the page context. Optionally navigate first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "script": { "type": "string" },
                    "url": { "type": "string" },
                    "timeout": { "type": "integer" },
                    "waitUntil": { "type": "string", "enum": ["domcontentloaded", "load", "networkidle", "done"] }
                },
                "required": ["script"]
            }
        }),
        json!({
            "name": "eval",
            "description": "Alias for evaluate.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "script": { "type": "string" },
                    "url": { "type": "string" },
                    "timeout": { "type": "integer" },
                    "waitUntil": { "type": "string", "enum": ["domcontentloaded", "load", "networkidle", "done"] }
                },
                "required": ["script"]
            }
        }),
        json!({
            "name": "semantic_tree",
            "description": "Return a text semantic tree for the current page, with structured payloads attached.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "timeout": { "type": "integer" },
                    "waitUntil": { "type": "string", "enum": ["domcontentloaded", "load", "networkidle", "done"] },
                    "backendNodeId": { "type": "integer" },
                    "maxDepth": { "type": "integer" }
                }
            }
        }),
        json!({
            "name": "nodeDetails",
            "description": "Inspect a specific backend node ID and return useful element details.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backendNodeId": { "type": "integer" }
                },
                "required": ["backendNodeId"]
            }
        }),
        json!({
            "name": "interactiveElements",
            "description": "List interactive elements discovered from the accessibility tree. Optionally navigate first.",
            "inputSchema": url_tool_schema()
        }),
        json!({
            "name": "findElement",
            "description": "Filter interactive elements by role and/or accessible name on the current page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "role": { "type": "string" },
                    "name": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "waitForSelector",
            "description": "Wait for a CSS selector to appear in the current page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string" },
                    "timeout": { "type": "integer", "description": "Timeout in milliseconds. Defaults to 5000." }
                },
                "required": ["selector"]
            }
        }),
        json!({
            "name": "click",
            "description": "Click an element by backend node ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backendNodeId": { "type": "integer" }
                },
                "required": ["backendNodeId"]
            }
        }),
        json!({
            "name": "fill",
            "description": "Fill an input or textarea by backend node ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backendNodeId": { "type": "integer" },
                    "text": { "type": "string" }
                },
                "required": ["backendNodeId", "text"]
            }
        }),
        json!({
            "name": "press",
            "description": "Dispatch a keyboard press, optionally targeting an element first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": { "type": "string" },
                    "backendNodeId": { "type": "integer" }
                },
                "required": ["key"]
            }
        }),
        json!({
            "name": "hover",
            "description": "Dispatch hover-style mouse events for an element by backend node ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backendNodeId": { "type": "integer" }
                },
                "required": ["backendNodeId"]
            }
        }),
        json!({
            "name": "scroll",
            "description": "Scroll the window or a specific element.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backendNodeId": { "type": "integer" },
                    "x": { "type": "integer" },
                    "y": { "type": "integer" }
                }
            }
        }),
        json!({
            "name": "selectOption",
            "description": "Select an option on a <select> element by value.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backendNodeId": { "type": "integer" },
                    "value": { "type": "string" }
                },
                "required": ["backendNodeId", "value"]
            }
        }),
        json!({
            "name": "setChecked",
            "description": "Set checked state on a checkbox or radio input.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backendNodeId": { "type": "integer" },
                    "checked": { "type": "boolean" }
                },
                "required": ["backendNodeId", "checked"]
            }
        }),
    ]
}

fn resource_list() -> Vec<Value> {
    vec![
        json!({
            "uri": "mcp://page/html",
            "name": "Page HTML",
            "description": "Serialized HTML of the current page",
            "mimeType": "text/html"
        }),
        json!({
            "uri": "mcp://page/markdown",
            "name": "Page Markdown",
            "description": "Markdown representation of the current page",
            "mimeType": "text/markdown"
        }),
    ]
}

fn url_tool_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "url": { "type": "string" },
            "timeout": { "type": "integer" },
            "waitUntil": { "type": "string", "enum": ["domcontentloaded", "load", "networkidle", "done"] }
        }
    })
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn tool_text(text: String) -> Result<Value> {
    Ok(json!({
        "content": [{
            "type": "text",
            "text": text
        }]
    }))
}

fn tool_json(value: Value) -> Result<Value> {
    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&value)?
        }],
        "structuredContent": value
    }))
}

fn tool_error(message: String) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": message
        }],
        "isError": true
    })
}

fn parse_wait_until(raw: Option<&str>) -> Result<RenderedDomWaitUntil> {
    match raw.unwrap_or("done").to_ascii_lowercase().as_str() {
        "domcontentloaded" | "dom_content_loaded" => Ok(RenderedDomWaitUntil::DomContentLoaded),
        "load" => Ok(RenderedDomWaitUntil::Load),
        "networkidle" | "network_idle" => Ok(RenderedDomWaitUntil::NetworkIdle),
        "domstable" | "dom_stable" => Ok(RenderedDomWaitUntil::DomStable),
        "done" => Ok(RenderedDomWaitUntil::Done),
        other => Err(anyhow!("unsupported waitUntil `{other}`")),
    }
}

async fn collect_interactive_elements(page: &mut Page) -> Result<Vec<Value>> {
    let payloads = page
        .accessibility_tree_payloads_for_document_async(None)
        .await?;
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for payload in payloads {
        let backend_node_id = payload
            .get("backendDOMNodeId")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default();
        if backend_node_id == 0 || !seen.insert(backend_node_id) {
            continue;
        }

        let role = payload["role"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let name = payload["name"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let value = payload["value"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let snapshot =
            document_node_snapshot_for_backend_node_id_live(page, backend_node_id).await?;
        if !is_interactive_role(&role) && !snapshot.as_ref().is_some_and(snapshot_looks_interactive)
        {
            continue;
        }

        items.push(json!({
            "backendNodeId": backend_node_id,
            "role": role,
            "name": name,
            "value": value,
            "tag": snapshot.as_ref().map(|snapshot| snapshot.local_name.clone()).unwrap_or_default(),
            "href": snapshot.as_ref().and_then(|snapshot| snapshot_attribute(snapshot, "href")).unwrap_or_default(),
            "inputType": snapshot.as_ref().and_then(|snapshot| snapshot_attribute(snapshot, "type")).unwrap_or_default(),
            "disabled": snapshot.as_ref().is_some_and(|snapshot| snapshot_attribute(snapshot, "disabled").is_some()),
        }));
    }

    Ok(items)
}

fn is_interactive_role(role: &str) -> bool {
    matches!(
        role,
        "button"
            | "link"
            | "textbox"
            | "searchbox"
            | "checkbox"
            | "radio"
            | "combobox"
            | "listbox"
            | "option"
            | "menuitem"
            | "tab"
            | "switch"
            | "slider"
            | "spinbutton"
    )
}

fn snapshot_looks_interactive(snapshot: &DocumentNodeSnapshot) -> bool {
    match snapshot.local_name.as_str() {
        "a" => snapshot_attribute(snapshot, "href").is_some(),
        "button" | "select" | "textarea" => true,
        "input" => !matches!(
            snapshot_attribute(snapshot, "type")
                .unwrap_or_else(|| "text".to_owned())
                .as_str(),
            "hidden"
        ),
        _ => false,
    }
}

fn snapshot_attribute(snapshot: &DocumentNodeSnapshot, name: &str) -> Option<String> {
    snapshot
        .attributes
        .iter()
        .find(|attribute| attribute.local_name == name)
        .map(|attribute| attribute.value.clone())
}

fn attribute_value_from_pairs(attributes: &[(String, String)], name: &str) -> Option<String> {
    attributes
        .iter()
        .find(|(local_name, _)| local_name == name)
        .map(|(_, value)| value.clone())
}

async fn query_selector_all_live(
    page: &mut Page,
    selector: &str,
) -> Result<Vec<RendererDocumentQuerySelectorNode>> {
    let pending = page.start_document_query_selector_for_document(selector.to_owned(), true)?;
    let completion = pending.wait().await?;
    match page.finish_document_query_selector(completion)? {
        RendererDocumentQuerySelectorResolution::Found(node_ids) => Ok(node_ids),
        RendererDocumentQuerySelectorResolution::MissingRoot => {
            Err(anyhow!("document root missing for selector `{selector}`"))
        }
        RendererDocumentQuerySelectorResolution::InvalidSelector(message) => {
            Err(anyhow!("invalid selector `{selector}`: {message}"))
        }
    }
}

async fn node_attributes_live(
    page: &mut Page,
    backend_node_id: u32,
) -> Result<Option<Vec<(String, String)>>> {
    let pending = page.start_document_node_attributes_for_backend_node_id(backend_node_id)?;
    let completion = pending.wait().await?;
    match page.finish_document_node_attributes(completion)? {
        RendererDocumentNodeAttributesResolution::Found(attributes) => Ok(Some(attributes)),
        RendererDocumentNodeAttributesResolution::NotElement
        | RendererDocumentNodeAttributesResolution::MissingNode => Ok(None),
    }
}

async fn node_text_live(page: &mut Page, backend_node_id: u32) -> Result<Option<String>> {
    let pending = page.start_document_node_text_for_backend_node_id(backend_node_id)?;
    let completion = pending.wait().await?;
    match page.finish_document_node_text(completion)? {
        RendererDocumentNodeTextResolution::Found(text) => Ok(Some(text)),
        RendererDocumentNodeTextResolution::MissingNode => Ok(None),
    }
}

async fn document_node_snapshot_for_backend_node_id_live(
    page: &mut Page,
    backend_node_id: u32,
) -> Result<Option<DocumentNodeSnapshot>> {
    let pending =
        page.start_document_node_snapshot_for_backend_node_id(backend_node_id, 0, false)?;
    let completion = pending.wait().await?;
    Ok(page
        .finish_document_node_snapshot_for_backend_node_id(completion)?
        .map(|object_snapshot| object_snapshot.snapshot))
}

async fn accessibility_tree_payloads_for_backend_node_id_live(
    page: &mut Page,
    backend_node_id: u32,
    max_depth: Option<i32>,
) -> Result<Vec<Value>> {
    let pending =
        page.start_accessibility_tree_payloads_for_backend_node_id(backend_node_id, max_depth)?;
    let completion = pending.wait().await?;
    Ok(page
        .finish_accessibility_payloads_for_backend_node_id(completion)?
        .and_then(|payload| payload.payloads)
        .unwrap_or_default())
}

async fn client_rect_for_backend_node_id_live(
    page: &mut Page,
    backend_node_id: u32,
) -> Result<Option<moli_core::page::ClientRect>> {
    let pending = page.start_client_rect_for_backend_node_id(backend_node_id)?;
    let completion = pending.wait().await?;
    Ok(
        match page.finish_client_rect_for_backend_node_id(completion)? {
            Some(DocumentNodeClientRectResolution::Found(rect)) => Some(rect),
            Some(
                DocumentNodeClientRectResolution::FoundNonElement(_)
                | DocumentNodeClientRectResolution::NotElement,
            )
            | None => None,
        },
    )
}

async fn click_backend_node_center_async(page: &mut Page, backend_node_id: u32) -> Result<()> {
    let rect = client_rect_for_backend_node_id_live(page, backend_node_id)
        .await?
        .ok_or_else(|| anyhow!("backendNodeId `{backend_node_id}` has no clickable geometry"))?;
    let x = rect.left + (rect.width / 2.0);
    let y = rect.top + (rect.height / 2.0);
    if !page
        .dispatch_mouse_event_at_point_async(x, y, "mousedown", 0, None, 0.0, 0.0)
        .await?
    {
        return Err(anyhow!("failed to dispatch click at node center"));
    }
    if !page
        .dispatch_mouse_event_at_point_async(x, y, "mouseup", 0, None, 0.0, 0.0)
        .await?
    {
        return Err(anyhow!("failed to dispatch click at node center"));
    }
    Ok(())
}

async fn call_function_on_backend_node_async(
    page: &mut Page,
    backend_node_id: u32,
    function_declaration: &str,
    arguments: Vec<Value>,
) -> Result<Value> {
    let pending = page.start_resolve_runtime_object_for_backend_node_id_in_inspector_session(
        None,
        backend_node_id,
        None,
        Some("mcp-actions"),
    )?;
    let completion = pending.wait().await?;
    let resolved = match page.finish_resolve_runtime_object_for_backend_node_id(completion)? {
        DocumentNodeRuntimeObjectResolution::Found(remote_object) => remote_object,
        DocumentNodeRuntimeObjectResolution::MissingContext => {
            return Err(anyhow!(
                "backendNodeId `{backend_node_id}` could not be resolved in the current runtime context"
            ));
        }
        DocumentNodeRuntimeObjectResolution::MissingNode => {
            return Err(anyhow!("backendNodeId `{backend_node_id}` not found"));
        }
    };
    let object_id = resolved
        .as_protocol_value()
        .get("objectId")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow!("backendNodeId `{backend_node_id}` runtime object did not include an objectId")
        })?;

    let request = json!({
        "id": 1,
        "method": "Runtime.callFunctionOn",
        "params": {
            "objectId": object_id,
            "functionDeclaration": function_declaration,
            "arguments": arguments,
            "returnByValue": true
        }
    });
    let raw = serde_json::to_string(&request)?;
    let prepared = page
        .prepare_runtime_protocol_message_async("callFunctionOn", &raw)
        .await?;
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    let pending = page.start_runtime_protocol_message_with_deferred_response(
        prepared,
        RendererRuntimeInspectorResponseSender::new(1, response_tx),
    )?;
    let completion = pending.wait().await?;
    let output = page.finish_runtime_protocol_message_command_turn(completion)?;
    let response = match output
        .runtime_inspector_output()
        .and_then(|output| output.protocol_response(1))
        .cloned()
    {
        Some(response) => response,
        None => response_rx
            .await
            .map_err(|_| anyhow!("runtime callFunctionOn response callback was canceled"))?
            .output
            .into_protocol_response(1)
            .ok_or_else(|| anyhow!("runtime callFunctionOn returned no protocol response"))?,
    };
    if let Some(error) = response.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("runtime callFunctionOn failed");
        return Err(anyhow!(message.to_owned()));
    }
    let result = response
        .get("result")
        .and_then(|result| result.get("result"))
        .cloned()
        .ok_or_else(|| anyhow!("runtime callFunctionOn returned no result payload"))?;
    Ok(result.get("value").cloned().unwrap_or(result))
}

struct KeySpec<'a> {
    key: &'a str,
    code: &'a str,
    text: &'a str,
}

fn key_spec(key: &str) -> KeySpec<'_> {
    match key {
        "Enter" => KeySpec {
            key,
            code: "Enter",
            text: "",
        },
        "Tab" => KeySpec {
            key,
            code: "Tab",
            text: "",
        },
        "Escape" => KeySpec {
            key,
            code: "Escape",
            text: "",
        },
        "Backspace" => KeySpec {
            key,
            code: "Backspace",
            text: "",
        },
        "ArrowUp" => KeySpec {
            key,
            code: "ArrowUp",
            text: "",
        },
        "ArrowDown" => KeySpec {
            key,
            code: "ArrowDown",
            text: "",
        },
        "ArrowLeft" => KeySpec {
            key,
            code: "ArrowLeft",
            text: "",
        },
        "ArrowRight" => KeySpec {
            key,
            code: "ArrowRight",
            text: "",
        },
        _ if key.len() == 1 => {
            let ch = key.chars().next().unwrap();
            if ch.is_ascii_alphabetic() {
                let upper = if ch.is_ascii_lowercase() {
                    ch.to_ascii_uppercase()
                } else {
                    ch
                };
                let code = match upper {
                    'A' => "KeyA",
                    'B' => "KeyB",
                    'C' => "KeyC",
                    'D' => "KeyD",
                    'E' => "KeyE",
                    'F' => "KeyF",
                    'G' => "KeyG",
                    'H' => "KeyH",
                    'I' => "KeyI",
                    'J' => "KeyJ",
                    'K' => "KeyK",
                    'L' => "KeyL",
                    'M' => "KeyM",
                    'N' => "KeyN",
                    'O' => "KeyO",
                    'P' => "KeyP",
                    'Q' => "KeyQ",
                    'R' => "KeyR",
                    'S' => "KeyS",
                    'T' => "KeyT",
                    'U' => "KeyU",
                    'V' => "KeyV",
                    'W' => "KeyW",
                    'X' => "KeyX",
                    'Y' => "KeyY",
                    _ => "KeyZ",
                };
                KeySpec {
                    key,
                    code,
                    text: key,
                }
            } else if ch.is_ascii_digit() {
                let code = match ch {
                    '0' => "Digit0",
                    '1' => "Digit1",
                    '2' => "Digit2",
                    '3' => "Digit3",
                    '4' => "Digit4",
                    '5' => "Digit5",
                    '6' => "Digit6",
                    '7' => "Digit7",
                    '8' => "Digit8",
                    _ => "Digit9",
                };
                KeySpec {
                    key,
                    code,
                    text: key,
                }
            } else if ch == ' ' {
                KeySpec {
                    key: " ",
                    code: "Space",
                    text: " ",
                }
            } else {
                KeySpec {
                    key,
                    code: "Unidentified",
                    text: key,
                }
            }
        }
        _ => KeySpec {
            key,
            code: key,
            text: "",
        },
    }
}

fn render_semantic_tree_text(payloads: &[Value]) -> String {
    if payloads.is_empty() {
        return String::new();
    }

    let mut by_id = HashMap::new();
    for payload in payloads {
        if let Some(node_id) = payload.get("nodeId").and_then(Value::as_str) {
            by_id.insert(node_id.to_owned(), payload);
        }
    }

    let Some(root_id) = payloads[0].get("nodeId").and_then(Value::as_str) else {
        return String::new();
    };

    let mut out = String::new();
    render_semantic_tree_line(root_id, &by_id, 0, &mut out);
    out.trim_end().to_owned()
}

fn render_semantic_tree_line(
    node_id: &str,
    by_id: &HashMap<String, &Value>,
    depth: usize,
    out: &mut String,
) {
    let Some(payload) = by_id.get(node_id) else {
        return;
    };

    let role = payload["role"]["value"].as_str().unwrap_or("unknown");
    let name = payload["name"]["value"].as_str().unwrap_or_default();
    let value = payload["value"]["value"].as_str().unwrap_or_default();
    let backend = payload["backendDOMNodeId"].as_u64().unwrap_or(0);

    out.push_str(&"  ".repeat(depth));
    out.push_str("- ");
    out.push_str(role);
    if !name.is_empty() {
        out.push_str(": ");
        out.push_str(name);
    }
    if !value.is_empty() {
        out.push_str(" = ");
        out.push_str(value);
    }
    if backend != 0 {
        out.push_str(&format!(" [backendNodeId={backend}]"));
    }
    out.push('\n');

    for child_id in payload["childIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        render_semantic_tree_line(child_id, by_id, depth + 1, out);
    }
}

async fn read_message<R>(reader: &mut R) -> Result<Option<Vec<u8>>>
where
    R: AsyncBufRead + Unpin,
{
    let mut content_length = None;
    loop {
        let mut line = Vec::new();
        let bytes_read = reader.read_until(b'\n', &mut line).await?;
        if bytes_read == 0 {
            return if content_length.is_none() {
                Ok(None)
            } else {
                Err(anyhow!("unexpected EOF while reading MCP headers"))
            };
        }

        if line == b"\n" || line == b"\r\n" {
            break;
        }

        let line = std::str::from_utf8(&line)?.trim();
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse::<usize>()?);
        }
    }

    let content_length = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}

async fn write_message<W>(writer: &mut W, response: &Value) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let body = serde_json::to_vec(response)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use moli_core::runtime::BrowserConfig;
    use std::sync::Arc;
    use tokio::{net::TcpListener, task::JoinHandle};

    async fn server_with_page(html: &str) -> Result<(McpServer, JoinHandle<()>)> {
        let browser = Browser::new(BrowserConfig::default())?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let body = Arc::new(html.to_owned());
        let server_body = Arc::clone(&body);
        let http_server = tokio::spawn(async move {
            let app = Router::new().route(
                "/",
                get(move || {
                    let body = Arc::clone(&server_body);
                    async move { (*body).clone() }
                }),
            );
            axum::serve(listener, app).await.unwrap();
        });
        let page = browser.fetch(&format!("http://{addr}/")).await?;
        let mut server = McpServer::new(browser);
        server.current_page = Some(page);
        Ok((server, http_server))
    }

    async fn backend_node_id_for_selector(page: &mut Page, selector: &str) -> Result<u32> {
        query_selector_all_live(page, selector)
            .await?
            .into_iter()
            .next()
            .map(|node| node.backend_node_id)
            .ok_or_else(|| anyhow!("selector `{selector}` not found"))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_links_uses_renderer_live_dom_commands() -> Result<()> {
        let (mut server, http_server) = server_with_page(
            "<body><a id=one href=/one>One <span>Link</span></a><a id=empty href=''></a></body>",
        )
        .await?;

        let result = server.tool_links(json!({})).await?;
        let links = result["structuredContent"]
            .as_array()
            .ok_or_else(|| anyhow!("links result should be an array"))?;

        assert_eq!(links.len(), 1);
        assert_eq!(links[0]["href"], json!("/one"));
        assert_eq!(links[0]["text"], json!("One Link"));
        assert!(
            links[0]["backendNodeId"]
                .as_u64()
                .and_then(|backend_node_id| u32::try_from(backend_node_id).ok())
                .is_some_and(moli_core::page::is_renderer_backend_node_id)
        );
        http_server.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_wait_for_selector_returns_renderer_backend_node_id() -> Result<()> {
        let (mut server, http_server) =
            server_with_page("<body><div id=ready>Ready</div></body>").await?;

        let result = server
            .tool_wait_for_selector(json!({
                "selector": "#ready",
                "timeout": 500
            }))
            .await?;
        let backend_node_id = result["structuredContent"]["backendNodeId"]
            .as_u64()
            .and_then(|backend_node_id| u32::try_from(backend_node_id).ok())
            .ok_or_else(|| anyhow!("waitForSelector should return backendNodeId"))?;
        assert!(moli_core::page::is_renderer_backend_node_id(
            backend_node_id
        ));

        let details = server
            .tool_node_details(json!({
                "backendNodeId": backend_node_id
            }))
            .await?;
        assert_eq!(details["structuredContent"]["tag"], json!("div"));
        http_server.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_interactive_elements_uses_renderer_live_snapshot_enrichment() -> Result<()> {
        let (mut server, http_server) =
            server_with_page("<body><a id=nav href=/go>Go</a><button>Save</button></body>").await?;

        let result = server.tool_interactive_elements(json!({})).await?;
        let items = result["structuredContent"]
            .as_array()
            .ok_or_else(|| anyhow!("interactive elements result should be an array"))?;
        let link = items
            .iter()
            .find(|item| item["href"] == json!("/go"))
            .ok_or_else(|| anyhow!("link should be present in interactive elements"))?;
        assert_eq!(link["tag"], json!("a"));
        assert_eq!(link["role"], json!("link"));

        let result = server
            .tool_find_element(json!({
                "role": "link",
                "name": "Go"
            }))
            .await?;
        let matches = result["structuredContent"]
            .as_array()
            .ok_or_else(|| anyhow!("findElement result should be an array"))?;
        assert!(matches.iter().any(|item| item["href"] == json!("/go")));
        http_server.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_semantic_tree_uses_renderer_live_accessibility_tree() -> Result<()> {
        let (mut server, http_server) =
            server_with_page(r#"<body><button id="target">old</button></body>"#).await?;

        let completion = {
            let page = server
                .current_page
                .as_mut()
                .ok_or_else(|| anyhow!("page not loaded"))?;
            let mutation = json!({
                "id": 21,
                "method": "Runtime.evaluate",
                "params": {
                    "expression": "document.getElementById('target').textContent = 'live'; 'done';",
                    "returnByValue": true
                }
            });
            let pending = page.start_runtime_protocol_message(serde_json::to_string(&mutation)?)?;
            pending.wait().await?
        };

        let result = server.tool_semantic_tree(json!({})).await?;
        let payloads = result["structuredContent"]
            .as_array()
            .ok_or_else(|| anyhow!("semantic_tree result should be an array"))?;
        assert!(
            payloads
                .iter()
                .any(|payload| payload["name"]["value"] == json!("live"))
        );
        assert!(
            !payloads
                .iter()
                .any(|payload| payload["name"]["value"] == json!("old"))
        );

        let page = server
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?;
        let _ = page.finish_runtime_protocol_message(completion)?;
        http_server.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_node_details_uses_renderer_live_snapshot() -> Result<()> {
        let (mut server, http_server) = server_with_page(
            "<body><select id=sel><option value=one>One</option><option value=two selected>Two</option></select></body>",
        )
        .await?;
        let backend_node_id = {
            let page = server
                .current_page
                .as_mut()
                .ok_or_else(|| anyhow!("page not loaded"))?;
            backend_node_id_for_selector(page, "#sel").await?
        };

        let result = server
            .tool_node_details(json!({
                "backendNodeId": backend_node_id
            }))
            .await?;

        let details = &result["structuredContent"];
        assert_eq!(details["backendNodeId"], json!(backend_node_id));
        assert_eq!(details["tag"], json!("select"));
        assert_eq!(details["options"][0]["value"], json!("one"));
        assert_eq!(details["options"][1]["text"], json!("Two"));
        assert_eq!(details["options"][1]["selected"], json!(true));
        http_server.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_node_details_reads_accessibility_by_backend_node_id() -> Result<()> {
        let (mut server, http_server) =
            server_with_page("<body><button id=submit>Submit order</button></body>").await?;
        let backend_node_id = {
            let page = server
                .current_page
                .as_mut()
                .ok_or_else(|| anyhow!("page not loaded"))?;
            backend_node_id_for_selector(page, "#submit").await?
        };

        let result = server
            .tool_node_details(json!({
                "backendNodeId": backend_node_id
            }))
            .await?;

        let details = &result["structuredContent"];
        assert_eq!(details["backendNodeId"], json!(backend_node_id));
        assert_eq!(details["tag"], json!("button"));
        assert_eq!(details["role"], json!("button"));
        assert_eq!(details["name"], json!("Submit order"));
        http_server.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_fill_uses_async_page_commands() -> Result<()> {
        let (mut server, http_server) =
            server_with_page("<body><input id=field value=seed><div id=done></div></body>").await?;
        let backend_node_id = {
            let page = server
                .current_page
                .as_mut()
                .ok_or_else(|| anyhow!("page not loaded"))?;
            backend_node_id_for_selector(page, "#field").await?
        };

        let result = server
            .tool_fill(json!({
                "backendNodeId": backend_node_id,
                "text": "async fill"
            }))
            .await?;

        assert_eq!(result["structuredContent"]["value"], json!("async fill"));
        http_server.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_evaluate_awaits_timer_promise_through_owner_continuation() -> Result<()> {
        let (mut server, http_server) =
            server_with_page("<body><main>evaluate await</main></body>").await?;

        let result = server
            .tool_evaluate(json!({
                "script": r#"new Promise(resolve => {
  setTimeout(() => resolve("mcp-owner-await"), 25);
})"#
            }))
            .await?;

        assert_eq!(
            result["structuredContent"]["value"],
            json!("mcp-owner-await")
        );
        http_server.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_select_option_uses_async_runtime_protocol_commands() -> Result<()> {
        let (mut server, http_server) = server_with_page(
            "<body><select id=sel><option value=one>One</option><option value=two>Two</option></select></body>",
        )
        .await?;
        let backend_node_id = {
            let page = server
                .current_page
                .as_mut()
                .ok_or_else(|| anyhow!("page not loaded"))?;
            backend_node_id_for_selector(page, "#sel").await?
        };

        let result = server
            .tool_select_option(json!({
                "backendNodeId": backend_node_id,
                "value": "two"
            }))
            .await?;

        assert_eq!(
            result["structuredContent"]["backendNodeId"],
            json!(backend_node_id)
        );
        let selected_value = server
            .current_page
            .as_mut()
            .ok_or_else(|| anyhow!("page not loaded"))?
            .evaluate_runtime_expression_async("document.getElementById('sel').value")
            .await?;
        assert_eq!(selected_value["value"], json!("two"));
        http_server.abort();
        Ok(())
    }
}
