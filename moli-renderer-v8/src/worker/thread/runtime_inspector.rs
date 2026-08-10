use std::{
    cell::{RefCell, UnsafeCell},
    collections::{HashMap, VecDeque},
    rc::Rc,
};

use serde_json::{Value, json};

use crate::inspector_microtasks::with_scoped_inspector_microtasks;
use crate::runtime::{RendererRuntimeInspectorMessage, RendererRuntimeInspectorResponseSender};
use crate::worker::handle::WorkerRuntimeInspectorMessageBatch;

use super::dispatch::perform_worker_microtask_checkpoint_and_report_pending_promise_rejections;

const WORKER_INSPECTOR_CONTEXT_GROUP_ID: i32 = 1;
const DEFAULT_WORKER_INSPECTOR_SESSION_KEY: &str = "";

#[derive(Default)]
struct WorkerInspectorOutboundState {
    pending_notifications: VecDeque<WorkerInspectorPendingMessage>,
    pending_response_callbacks: HashMap<(String, i32), RendererRuntimeInspectorResponseSender>,
    active_dispatch_scopes: Vec<WorkerInspectorDispatchScope>,
}

#[derive(Clone, Default)]
struct WorkerInspectorOutbound(Rc<RefCell<WorkerInspectorOutboundState>>);

#[derive(Default)]
struct WorkerInspectorDispatchScope {
    session_key: String,
    messages: Vec<RendererRuntimeInspectorMessage>,
}

struct WorkerInspectorPendingMessage {
    session_key: String,
    message: RendererRuntimeInspectorMessage,
}

#[derive(Debug, Clone, PartialEq)]
struct WorkerInspectorPendingMessageBatch {
    inspector_session_id: Option<String>,
    messages: Vec<RendererRuntimeInspectorMessage>,
}

struct WorkerInspectorDispatchScopeGuard {
    outbound: WorkerInspectorOutbound,
    completed: bool,
}

impl WorkerInspectorDispatchScopeGuard {
    fn finish(mut self) -> Vec<RendererRuntimeInspectorMessage> {
        self.completed = true;
        self.outbound
            .0
            .borrow_mut()
            .active_dispatch_scopes
            .pop()
            .expect("worker inspector dispatch scope stack underflow")
            .messages
    }
}

impl Drop for WorkerInspectorDispatchScopeGuard {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        let mut guard = self.outbound.0.borrow_mut();
        guard
            .active_dispatch_scopes
            .pop()
            .expect("worker inspector dispatch scope stack underflow");
    }
}

impl WorkerInspectorOutbound {
    fn push_message(
        &self,
        session_key: &str,
        mut message: v8::UniquePtr<v8::inspector::StringBuffer>,
    ) {
        let Some(message) = message.as_mut() else {
            return;
        };
        let view = message.string();
        let message_units = view.len();
        let message_width = if view.is_8bit() { 8 } else { 16 };
        match moli_v8_util::decode_inspector_protocol_message(view) {
            Ok(value) => self.push_value(session_key, value),
            Err(error) => {
                let raw = moli_v8_util::inspector_protocol_message_text(view);
                tracing::warn!(
                    target: "moli::cdp::worker_inspector",
                    %error,
                    %raw,
                    message_units,
                    message_width,
                    "failed to decode worker inspector protocol message; dropping"
                );
            }
        }
    }

    fn push_value(&self, session_key: &str, value: Value) {
        let message = RendererRuntimeInspectorMessage::from_v8_inspector_message(value);
        let mut guard = self.0.borrow_mut();
        if let Some(scope) = guard
            .active_dispatch_scopes
            .last_mut()
            .filter(|scope| scope.session_key == session_key)
        {
            scope.messages.push(message);
        } else {
            guard
                .pending_notifications
                .push_back(WorkerInspectorPendingMessage {
                    session_key: session_key.to_owned(),
                    message,
                });
        }
    }

    fn push_response_message(
        &self,
        session_key: &str,
        call_id: i32,
        mut message: v8::UniquePtr<v8::inspector::StringBuffer>,
    ) {
        let Some(message) = message.as_mut() else {
            return;
        };
        let view = message.string();
        let message_units = view.len();
        let message_width = if view.is_8bit() { 8 } else { 16 };
        match moli_v8_util::decode_inspector_protocol_message(view) {
            Ok(value) => self.push_response_value(session_key, call_id, value),
            Err(error) => {
                let raw = moli_v8_util::inspector_protocol_message_text(view);
                tracing::warn!(
                    target: "moli::cdp::worker_inspector",
                    %error,
                    %raw,
                    message_units,
                    message_width,
                    "failed to decode worker inspector protocol response; dropping"
                );
            }
        }
    }

    fn push_response_value(&self, session_key: &str, call_id: i32, value: Value) {
        let callback = {
            self.0
                .borrow_mut()
                .pending_response_callbacks
                .remove(&(session_key.to_owned(), call_id))
        };
        if let Some(callback) = callback {
            if let Err(message) = callback.send(value) {
                tracing::debug!(
                    call_id,
                    message = ?message,
                    "dropping worker runtime inspector response because deferred receiver was closed"
                );
            }
            return;
        }
        let mut guard = self.0.borrow_mut();
        if let Some(scope) = guard
            .active_dispatch_scopes
            .last_mut()
            .filter(|scope| scope.session_key == session_key)
        {
            scope
                .messages
                .push(RendererRuntimeInspectorMessage::from_v8_inspector_message(
                    value,
                ));
        } else {
            tracing::debug!(
                call_id,
                message = ?value,
                "dropping stale worker runtime inspector response without a registered deferred callback"
            );
        }
    }

    fn register_response_callback(
        &self,
        session_key: &str,
        callback: RendererRuntimeInspectorResponseSender,
    ) {
        let call_id = callback.call_id();
        let mut guard = self.0.borrow_mut();
        let callback_key = (session_key.to_owned(), call_id);
        if guard.pending_response_callbacks.contains_key(&callback_key) {
            tracing::error!(
                session_key,
                call_id,
                "worker runtime inspector response callback registered twice; keeping the existing callback"
            );
            return;
        }
        guard
            .pending_response_callbacks
            .insert(callback_key, callback);
    }

    fn take_all(&self) -> Vec<WorkerInspectorPendingMessageBatch> {
        coalesce_worker_inspector_messages(
            self.0
                .borrow_mut()
                .pending_notifications
                .drain(..)
                .collect(),
        )
    }

    fn push_dispatch_scope(&self, session_key: &str) -> WorkerInspectorDispatchScopeGuard {
        self.0
            .borrow_mut()
            .active_dispatch_scopes
            .push(WorkerInspectorDispatchScope {
                session_key: session_key.to_owned(),
                messages: Vec::new(),
            });
        WorkerInspectorDispatchScopeGuard {
            outbound: self.clone(),
            completed: false,
        }
    }
}

struct WorkerInspectorChannel {
    outbound: WorkerInspectorOutbound,
    session_key: String,
}

impl v8::inspector::ChannelImpl for WorkerInspectorChannel {
    fn send_response(&self, call_id: i32, message: v8::UniquePtr<v8::inspector::StringBuffer>) {
        self.outbound
            .push_response_message(&self.session_key, call_id, message);
    }

    fn send_notification(&self, message: v8::UniquePtr<v8::inspector::StringBuffer>) {
        self.outbound.push_message(&self.session_key, message);
    }

    fn flush_protocol_notifications(&self) {}
}

struct WorkerInspectorClient {
    isolate: UnsafeCell<v8::UnsafeRawIsolatePtr>,
    default_context: Rc<RefCell<Option<v8::Global<v8::Context>>>>,
}

impl WorkerInspectorClient {
    fn new(
        isolate: v8::UnsafeRawIsolatePtr,
        default_context: Rc<RefCell<Option<v8::Global<v8::Context>>>>,
    ) -> Self {
        Self {
            isolate: UnsafeCell::new(isolate),
            default_context,
        }
    }
}

impl v8::inspector::V8InspectorClientImpl for WorkerInspectorClient {
    fn ensure_default_context_in_group(
        &self,
        context_group_id: i32,
    ) -> Option<v8::Local<'_, v8::Context>> {
        if context_group_id != WORKER_INSPECTOR_CONTEXT_GROUP_ID {
            return None;
        }
        let default_context = self.default_context.borrow();
        let default_context = default_context.as_ref()?;
        let isolate = unsafe { &mut *self.isolate.get() };
        let isolate = unsafe { v8::Isolate::ref_from_raw_isolate_ptr_mut(isolate) };
        v8::callback_scope!(unsafe let scope, isolate);
        Some(v8::Local::new(scope, default_context))
    }
}

struct WorkerInspectorSessionState {
    session: v8::inspector::V8InspectorSession,
}

pub(super) struct WorkerRuntimeInspector {
    sessions: HashMap<String, WorkerInspectorSessionState>,
    inspector: v8::inspector::V8Inspector,
    outbound: WorkerInspectorOutbound,
    default_context: Rc<RefCell<Option<v8::Global<v8::Context>>>>,
    default_execution_context_id: Option<i64>,
}

impl WorkerRuntimeInspector {
    pub(super) fn new(isolate: &mut v8::Isolate) -> Self {
        let isolate_ptr = unsafe { isolate.as_raw_isolate_ptr() };
        let default_context = Rc::new(RefCell::new(None));
        let inspector_client = v8::inspector::V8InspectorClient::new(Box::new(
            WorkerInspectorClient::new(isolate_ptr, Rc::clone(&default_context)),
        ));
        Self {
            inspector: v8::inspector::V8Inspector::create(isolate, inspector_client),
            sessions: HashMap::new(),
            outbound: WorkerInspectorOutbound::default(),
            default_context,
            default_execution_context_id: None,
        }
    }

    pub(super) fn attach_context<'s>(
        &mut self,
        context: v8::Local<'s, v8::Context>,
        default_context: v8::Global<v8::Context>,
        script_url: &str,
    ) {
        *self.default_context.borrow_mut() = Some(default_context);
        self.inspector.context_created(
            context,
            WORKER_INSPECTOR_CONTEXT_GROUP_ID,
            v8::inspector::StringView::from(script_url.as_bytes()),
            v8::inspector::StringView::from(script_url.as_bytes()),
            v8::inspector::StringView::from(&br#"{"isDefault":true,"type":"worker"}"#[..]),
        );
        self.default_execution_context_id = Some(i64::from(
            v8::inspector::V8Inspector::execution_context_id(context),
        ));
    }

    pub(super) fn context_destroyed<'s>(&self, context: v8::Local<'s, v8::Context>) {
        self.inspector.context_destroyed(context);
    }

    pub(super) fn detach_session(&mut self, inspector_session_id: Option<&str>) {
        let session_key = worker_inspector_session_key(inspector_session_id);
        self.sessions.remove(&session_key);
    }

    pub(super) fn attach_session(&mut self, inspector_session_id: Option<&str>) {
        let session_key = worker_inspector_session_key(inspector_session_id);
        let _ = self.ensure_session(&session_key);
    }

    pub(super) fn dispatch_protocol_message(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        inspector_session_id: Option<&str>,
        raw_json: &str,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        self.dispatch_protocol_message_with_optional_deferred_response(
            scope,
            inspector_session_id,
            raw_json,
            None,
        )
    }

    pub(super) fn dispatch_protocol_message_with_deferred_response(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        inspector_session_id: Option<&str>,
        raw_json: &str,
        deferred_response: RendererRuntimeInspectorResponseSender,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        self.dispatch_protocol_message_with_optional_deferred_response(
            scope,
            inspector_session_id,
            raw_json,
            Some(deferred_response),
        )
    }

    fn dispatch_protocol_message_with_optional_deferred_response(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        inspector_session_id: Option<&str>,
        raw_json: &str,
        deferred_response: Option<RendererRuntimeInspectorResponseSender>,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        let session_key = worker_inspector_session_key(inspector_session_id);
        if let Some(callback) = deferred_response {
            self.outbound
                .register_response_callback(&session_key, callback);
        }
        let dispatch_scope = self.outbound.push_dispatch_scope(&session_key);
        let session = self.ensure_session(&session_key);
        with_scoped_inspector_microtasks(scope, || {
            session.dispatch_protocol_message(v8::inspector::StringView::from(raw_json.as_bytes()));
        });
        perform_worker_microtask_checkpoint_and_report_pending_promise_rejections(scope);
        let messages = dispatch_scope.finish();
        self.record_execution_context_state(&messages);
        Ok(messages)
    }

    pub(super) fn take_pending_messages(&mut self) -> Vec<WorkerRuntimeInspectorMessageBatch> {
        let batches = self.outbound.take_all();
        for batch in &batches {
            self.record_execution_context_state(&batch.messages);
        }
        batches
            .into_iter()
            .map(worker_runtime_inspector_message_batch)
            .collect()
    }

    pub(super) fn worker_script_loaded_messages(&self) -> Vec<WorkerRuntimeInspectorMessageBatch> {
        let mut session_keys = self.sessions.keys().collect::<Vec<_>>();
        session_keys.sort_unstable();
        session_keys
            .into_iter()
            .map(|session_key| WorkerRuntimeInspectorMessageBatch {
                inspector_session_id: worker_inspector_session_id_from_key(session_key),
                messages: vec![RendererRuntimeInspectorMessage::from_v8_inspector_message(
                    json!({
                        "method": "Inspector.workerScriptLoaded",
                        "params": {}
                    }),
                )],
            })
            .collect()
    }

    fn ensure_session(&mut self, session_key: &str) -> &v8::inspector::V8InspectorSession {
        &self
            .sessions
            .entry(session_key.to_owned())
            .or_insert_with(|| WorkerInspectorSessionState {
                session: self.inspector.connect(
                    WORKER_INSPECTOR_CONTEXT_GROUP_ID,
                    v8::inspector::Channel::new(Box::new(WorkerInspectorChannel {
                        outbound: self.outbound.clone(),
                        session_key: session_key.to_owned(),
                    })),
                    v8::inspector::StringView::from(&b"{}"[..]),
                    v8::inspector::V8InspectorClientTrustLevel::FullyTrusted,
                ),
            })
            .session
    }

    fn record_execution_context_state(&mut self, messages: &[RendererRuntimeInspectorMessage]) {
        for message in messages {
            match message {
                RendererRuntimeInspectorMessage::RuntimeContext(
                    crate::protocol_types::RuntimeContextRestoreEvent::Created(event),
                ) => {
                    if event.context_type.as_deref() == Some("worker")
                        && let Some(id) = event.context_id
                    {
                        self.default_execution_context_id = Some(id);
                    }
                }
                RendererRuntimeInspectorMessage::RuntimeContext(
                    crate::protocol_types::RuntimeContextRestoreEvent::Destroyed(event),
                ) => {
                    if event.context_id == self.default_execution_context_id {
                        self.default_execution_context_id = None;
                    }
                }
                RendererRuntimeInspectorMessage::RuntimeContext(
                    crate::protocol_types::RuntimeContextRestoreEvent::Cleared(_),
                ) => {
                    self.default_execution_context_id = None;
                }
                _ => {}
            }
        }
    }
}

fn worker_inspector_session_key(inspector_session_id: Option<&str>) -> String {
    inspector_session_id
        .filter(|session_id| !session_id.is_empty())
        .unwrap_or(DEFAULT_WORKER_INSPECTOR_SESSION_KEY)
        .to_owned()
}

fn worker_inspector_session_id_from_key(session_key: &str) -> Option<String> {
    if session_key == DEFAULT_WORKER_INSPECTOR_SESSION_KEY {
        None
    } else {
        Some(session_key.to_owned())
    }
}

fn coalesce_worker_inspector_messages(
    messages: Vec<WorkerInspectorPendingMessage>,
) -> Vec<WorkerInspectorPendingMessageBatch> {
    let mut batches = Vec::<WorkerInspectorPendingMessageBatch>::new();
    for pending in messages {
        let inspector_session_id = worker_inspector_session_id_from_key(&pending.session_key);
        if let Some(batch) = batches
            .last_mut()
            .filter(|batch| batch.inspector_session_id.as_ref() == inspector_session_id.as_ref())
        {
            batch.messages.push(pending.message);
            continue;
        }
        batches.push(WorkerInspectorPendingMessageBatch {
            inspector_session_id,
            messages: vec![pending.message],
        });
    }
    batches
}

fn worker_runtime_inspector_message_batch(
    batch: WorkerInspectorPendingMessageBatch,
) -> WorkerRuntimeInspectorMessageBatch {
    WorkerRuntimeInspectorMessageBatch {
        inspector_session_id: batch.inspector_session_id,
        messages: batch.messages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::pin::pin;

    fn inspector_message(value: Value) -> RendererRuntimeInspectorMessage {
        RendererRuntimeInspectorMessage::from_v8_inspector_message(value)
    }

    fn protocol_response(messages: &[RendererRuntimeInspectorMessage], call_id: i64) -> Value {
        messages
            .iter()
            .find_map(|message| match message {
                RendererRuntimeInspectorMessage::Protocol(message)
                    if message.value().get("id").and_then(Value::as_i64) == Some(call_id) =>
                {
                    Some(message.value().clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing worker Inspector response {call_id}: {messages:#?}"))
    }

    fn force_gc_and_report_worker_inspector_policy_for_test(
        scope: &mut v8::PinScope<'_, '_>,
        _args: v8::FunctionCallbackArguments<'_>,
        mut rv: v8::ReturnValue<'_, v8::Value>,
    ) {
        let inspector_policy_is_scoped =
            scope.get_microtasks_policy() == v8::MicrotasksPolicy::Scoped;
        scope.memory_pressure_notification(v8::MemoryPressureLevel::Critical);
        scope.low_memory_notification();
        rv.set(v8::Boolean::new(scope, inspector_policy_is_scoped).into());
    }

    #[test]
    fn worker_inspector_dispatch_scopes_microtasks_and_restores_owner_policy() {
        crate::ensure_v8_for_test();
        let mut isolate = v8::Isolate::new(Default::default());
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        let mut inspector = WorkerRuntimeInspector::new(&mut isolate);
        let context = {
            let scope = pin!(v8::HandleScope::new(&mut isolate));
            let scope = &mut scope.init();
            let context = v8::Context::new(scope, Default::default());
            let scope = &mut v8::ContextScope::new(scope, context);
            let callback =
                v8::Function::new(scope, force_gc_and_report_worker_inspector_policy_for_test)
                    .expect("worker test GC callback");
            let key = v8::String::new(scope, "__moliWorkerForceGcAndReportInspectorPolicyForTest")
                .expect("worker test GC callback key");
            assert_eq!(
                context
                    .global(scope)
                    .set(scope, key.into(), callback.into()),
                Some(true),
                "worker test GC callback should install"
            );
            let retained_context = v8::Global::new(scope, context);
            inspector.attach_context(
                context,
                v8::Global::new(scope, context),
                "https://worker-inspector-microtasks.test/worker.js",
            );
            retained_context
        };

        let scope = pin!(v8::HandleScope::new(&mut isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &context);
        let scope = &mut v8::ContextScope::new(scope, context);
        let messages = inspector
            .dispatch_protocol_message(
                scope,
                None,
                &json!({
                    "id": 81,
                    "method": "Runtime.evaluate",
                    "params": {
                        "expression": r#"(() => {
                            Promise.resolve().then(() => {
                                const values = [];
                                for (let index = 0; index < 5000; index += 1) {
                                    values.push({ index, text: "x".repeat(128) });
                                }
                                globalThis.__workerQueuedAllocationCount = values.length;
                                globalThis.__workerInspectorPolicyWasScoped =
                                    __moliWorkerForceGcAndReportInspectorPolicyForTest();
                            });
                            return "worker-sync-result";
                        })()"#,
                        "awaitPromise": true,
                        "returnByValue": true,
                    }
                })
                .to_string(),
            )
            .expect("worker awaitPromise dispatch");
        let response = protocol_response(&messages, 81);
        assert!(
            response.get("error").is_none(),
            "worker synchronous awaitPromise result must survive its Inspector checkpoint: {response:#?}"
        );
        assert_eq!(
            response["result"]["result"]["value"],
            json!("worker-sync-result")
        );
        assert_eq!(
            scope.get_microtasks_policy(),
            v8::MicrotasksPolicy::Explicit,
            "worker awaitPromise dispatch must restore the owner policy"
        );

        let messages = inspector
            .dispatch_protocol_message(
                scope,
                None,
                &json!({
                    "id": 82,
                    "method": "Runtime.evaluate",
                    "params": {
                        "expression": "({ count: globalThis.__workerQueuedAllocationCount, queuedScoped: globalThis.__workerInspectorPolicyWasScoped, scoped: __moliWorkerForceGcAndReportInspectorPolicyForTest() })",
                        "returnByValue": true,
                    }
                })
                .to_string(),
            )
            .expect("worker non-await Runtime.evaluate dispatch");
        let response = protocol_response(&messages, 82);
        assert_eq!(response["result"]["result"]["value"]["count"], json!(5000));
        assert_eq!(
            response["result"]["result"]["value"]["queuedScoped"],
            json!(true)
        );
        assert_eq!(response["result"]["result"]["value"]["scoped"], json!(true));
        assert_eq!(
            scope.get_microtasks_policy(),
            v8::MicrotasksPolicy::Explicit,
            "worker non-await dispatch must restore the owner policy"
        );
    }

    #[test]
    fn worker_inspector_outbound_captures_dispatch_and_drops_stale_late_response() {
        let outbound = WorkerInspectorOutbound::default();

        let dispatch_scope = outbound.push_dispatch_scope("SID-1");
        outbound.push_response_value("SID-1", 1, json!({"id": 1, "result": "command-tail"}));
        let current = dispatch_scope.finish();
        outbound.push_response_value("SID-1", 2, json!({"id": 2, "result": "late"}));

        assert_eq!(
            current,
            vec![inspector_message(
                json!({"id": 1, "result": "command-tail"})
            )],
            "stale worker inspector responses without callbacks must not be forwarded as target lifecycle output"
        );
        assert!(
            outbound.take_all().is_empty(),
            "stale worker late response must not contaminate pending notifications"
        );
    }

    #[test]
    fn worker_inspector_outbound_duplicate_response_callback_keeps_existing_callback() {
        let outbound = WorkerInspectorOutbound::default();
        let (first_tx, mut first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, mut second_rx) = tokio::sync::oneshot::channel();

        outbound.register_response_callback(
            "SID-1",
            RendererRuntimeInspectorResponseSender::new(7, first_tx),
        );
        outbound.register_response_callback(
            "SID-1",
            RendererRuntimeInspectorResponseSender::new(7, second_tx),
        );
        outbound.push_response_value("SID-1", 7, json!({"id": 7, "result": "first"}));

        let completion = first_rx.try_recv().expect("first deferred response");
        assert_eq!(completion.call_id, 7);
        assert_eq!(
            completion.output.protocol_response(7),
            Some(&json!({"id": 7, "result": "first"}))
        );
        assert!(matches!(
            second_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        ));
        assert!(
            outbound.take_all().is_empty(),
            "duplicate worker callback registration must not leave pending notification output"
        );
    }

    #[test]
    fn worker_inspector_outbound_isolates_callbacks_by_session() {
        let outbound = WorkerInspectorOutbound::default();
        let (first_tx, mut first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, mut second_rx) = tokio::sync::oneshot::channel();

        outbound.register_response_callback(
            "SID-1",
            RendererRuntimeInspectorResponseSender::new(7, first_tx),
        );
        outbound.register_response_callback(
            "SID-2",
            RendererRuntimeInspectorResponseSender::new(7, second_tx),
        );

        outbound.push_response_value("SID-2", 7, json!({"id": 7, "result": "second"}));
        outbound.push_response_value("SID-1", 7, json!({"id": 7, "result": "first"}));

        let first_completion = first_rx.try_recv().expect("first session response");
        assert_eq!(first_completion.call_id, 7);
        assert_eq!(
            first_completion.output.protocol_response(7),
            Some(&json!({"id": 7, "result": "first"}))
        );
        let second_completion = second_rx.try_recv().expect("second session response");
        assert_eq!(second_completion.call_id, 7);
        assert_eq!(
            second_completion.output.protocol_response(7),
            Some(&json!({"id": 7, "result": "second"}))
        );
    }

    #[test]
    fn worker_inspector_outbound_tags_peer_session_pending_messages() {
        let outbound = WorkerInspectorOutbound::default();

        let dispatch_scope = outbound.push_dispatch_scope("SID-1");
        outbound.push_value(
            "SID-2",
            json!({"method": "Runtime.executionContextCreated"}),
        );
        outbound.push_value("SID-1", json!({"id": 1, "result": {}}));
        let current = dispatch_scope.finish();

        assert_eq!(
            current,
            vec![inspector_message(json!({"id": 1, "result": {}}))]
        );
        assert_eq!(
            outbound.take_all(),
            vec![WorkerInspectorPendingMessageBatch {
                inspector_session_id: Some("SID-2".to_owned()),
                messages: vec![inspector_message(json!({
                    "method": "Runtime.executionContextCreated"
                }))],
            }]
        );
    }
}
