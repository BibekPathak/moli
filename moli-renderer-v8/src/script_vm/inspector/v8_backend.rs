use super::InspectorOutbound;
use super::context_registry::{
    DocumentInspectorContextGroupId, DocumentInspectorContextRegistrationId,
    DocumentInspectorContextRegistry,
};
use crate::{
    inspector_microtasks::with_scoped_inspector_microtasks,
    runtime::RendererRuntimeInspectorResponseSender,
    script_vm::inspector_pause::{RendererInspectorPauseBridge, RendererInspectorPauseCommand},
};
use moli_page_types::{DevToolsSessionKey, RendererDevToolsAgentToken, V8InspectorSessionState};
use serde_json::json;
use std::{
    cell::{Cell, RefCell, UnsafeCell},
    collections::HashMap,
    rc::{Rc, Weak},
    sync::atomic::{AtomicI64, Ordering},
};

struct RendererInspectorClient {
    isolate: UnsafeCell<v8::UnsafeRawIsolatePtr>,
    context_registry: DocumentInspectorContextRegistry,
    unique_id_state: Rc<RendererInspectorClientUniqueIdState>,
    pause_loop: Rc<RendererInspectorPauseLoopLocal>,
}

#[derive(Clone)]
struct RendererInspectorPauseSession {
    session: Weak<v8::inspector::V8InspectorSession>,
    outbound: InspectorOutbound,
    agent_token: RendererDevToolsAgentToken,
    session_key: DevToolsSessionKey,
}

pub(super) struct RendererInspectorPauseSessionRegistration {
    pause_loop: Weak<RendererInspectorPauseLoopLocal>,
    context_group_id: i32,
    session_key: DevToolsSessionKey,
    session: Weak<v8::inspector::V8InspectorSession>,
}

struct RendererInspectorPauseLoopLocal {
    bridge: RendererInspectorPauseBridge,
    sessions: RefCell<HashMap<(i32, DevToolsSessionKey), RendererInspectorPauseSession>>,
}

impl Drop for RendererInspectorPauseSessionRegistration {
    fn drop(&mut self) {
        let Some(pause_loop) = self.pause_loop.upgrade() else {
            return;
        };
        let key = (self.context_group_id, self.session_key.clone());
        let mut sessions = pause_loop.sessions.borrow_mut();
        if sessions
            .get(&key)
            .is_some_and(|entry| Weak::ptr_eq(&entry.session, &self.session))
        {
            sessions.remove(&key);
        }
    }
}

impl RendererInspectorPauseLoopLocal {
    fn new(bridge: RendererInspectorPauseBridge) -> Self {
        Self {
            bridge,
            sessions: RefCell::new(HashMap::new()),
        }
    }

    fn register_session(
        self: &Rc<Self>,
        context_group_id: DocumentInspectorContextGroupId,
        agent_token: RendererDevToolsAgentToken,
        session_key: DevToolsSessionKey,
        session: &Rc<v8::inspector::V8InspectorSession>,
        outbound: InspectorOutbound,
    ) -> RendererInspectorPauseSessionRegistration {
        let weak_session = Rc::downgrade(session);
        self.sessions.borrow_mut().insert(
            (context_group_id.get(), session_key.clone()),
            RendererInspectorPauseSession {
                session: weak_session.clone(),
                outbound,
                agent_token,
                session_key: session_key.clone(),
            },
        );
        RendererInspectorPauseSessionRegistration {
            pause_loop: Rc::downgrade(self),
            context_group_id: context_group_id.get(),
            session_key,
            session: weak_session,
        }
    }

    fn run_message_loop_on_pause(&self, context_group_id: i32) {
        if !self.bridge.enter_pause() {
            return;
        }
        while let Some(command) = self.bridge.wait_for_command() {
            self.dispatch_command(context_group_id, command);
        }
        self.bridge.leave_pause();
    }

    fn dispatch_command(&self, context_group_id: i32, command: RendererInspectorPauseCommand) {
        let session_key = DevToolsSessionKey::from_wire_session_id(
            command
                .inspector_session_id
                .as_deref()
                .filter(|session_id| !session_id.is_empty()),
        );
        let session = self
            .sessions
            .borrow()
            .get(&(context_group_id, session_key))
            .cloned();
        let Some(session) = session else {
            send_pause_dispatch_error(command.response, "Inspector session is not available");
            return;
        };
        let Some(v8_session) = session.session.upgrade() else {
            send_pause_dispatch_error(command.response, "Inspector session has been detached");
            return;
        };
        let _command_dispatch = self.bridge.begin_command_dispatch(&command);
        session
            .outbound
            .register_response_callback(command.response);
        v8_session.dispatch_protocol_message(v8::inspector::StringView::from(
            command.raw_json.as_bytes(),
        ));
        self.bridge.record_v8_state_update(
            session.agent_token,
            session.session_key,
            V8InspectorSessionState::from_bytes(v8_session.state()),
        );
    }

    fn quit_message_loop_on_pause(&self) {
        self.bridge.request_quit();
    }
}

fn send_pause_dispatch_error(response: RendererRuntimeInspectorResponseSender, message: &str) {
    let call_id = response.call_id();
    let _ = response.send(json!({
        "id": call_id,
        "error": {
            "code": -32000,
            "message": message,
        },
    }));
}

pub(super) struct RendererInspectorClientUniqueIdState {
    capture_depth: Cell<usize>,
    captured_ids: RefCell<Vec<i64>>,
}

struct RendererInspectorUniqueIdCaptureGuard<'a> {
    state: &'a RendererInspectorClientUniqueIdState,
}

impl Drop for RendererInspectorUniqueIdCaptureGuard<'_> {
    fn drop(&mut self) {
        let depth = self.state.capture_depth.get();
        debug_assert!(depth > 0, "V8 inspector unique-id capture underflow");
        self.state.capture_depth.set(depth.saturating_sub(1));
    }
}

impl RendererInspectorClientUniqueIdState {
    pub(super) fn new() -> Self {
        Self {
            capture_depth: Cell::new(0),
            captured_ids: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn generate_unique_id(&self) -> i64 {
        static NEXT_UNIQUE_ID: AtomicI64 = AtomicI64::new(1);

        let id = NEXT_UNIQUE_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("V8 inspector unique id exhausted");
        assert!(id > 0, "V8 inspector unique id exhausted");
        if self.capture_depth.get() > 0 {
            self.captured_ids.borrow_mut().push(id);
        }
        id
    }

    pub(super) fn capture_context_unique_id(&self, op: impl FnOnce()) -> Option<String> {
        debug_assert_eq!(
            self.capture_depth.get(),
            0,
            "nested V8 inspector context unique-id capture"
        );
        self.captured_ids.borrow_mut().clear();
        self.capture_depth.set(self.capture_depth.get() + 1);
        {
            let _capture_guard = RendererInspectorUniqueIdCaptureGuard { state: self };
            op();
        }
        let ids = self.captured_ids.borrow();
        (ids.len() >= 2).then(|| format!("{}.{}", ids[0], ids[1]))
    }

    #[cfg(test)]
    pub(super) fn capture_depth_for_test(&self) -> usize {
        self.capture_depth.get()
    }
}

impl RendererInspectorClient {
    fn new(
        isolate: v8::UnsafeRawIsolatePtr,
        context_registry: DocumentInspectorContextRegistry,
        unique_id_state: Rc<RendererInspectorClientUniqueIdState>,
        pause_loop: Rc<RendererInspectorPauseLoopLocal>,
    ) -> Self {
        Self {
            isolate: UnsafeCell::new(isolate),
            context_registry,
            unique_id_state,
            pause_loop,
        }
    }
}

impl v8::inspector::V8InspectorClientImpl for RendererInspectorClient {
    fn run_message_loop_on_pause(&self, context_group_id: i32) {
        // A pause may originate in an ordinary page task, outside the guarded
        // frontend dispatch path. Keep every nested pause-loop command under
        // the same Inspector policy boundary without adding another isolate
        // pointer or changing the pause bridge protocol.
        let isolate = unsafe { &mut *self.isolate.get() };
        let isolate = unsafe { v8::Isolate::ref_from_raw_isolate_ptr_mut(isolate) };
        with_scoped_inspector_microtasks(isolate, || {
            self.pause_loop.run_message_loop_on_pause(context_group_id);
        });
    }

    fn quit_message_loop_on_pause(&self) {
        self.pause_loop.quit_message_loop_on_pause();
    }

    fn generate_unique_id(&self) -> i64 {
        self.unique_id_state.generate_unique_id()
    }

    fn ensure_default_context_in_group(
        &self,
        context_group_id: i32,
    ) -> Option<v8::Local<'_, v8::Context>> {
        let context_group_id = DocumentInspectorContextGroupId::from_raw(context_group_id);

        let isolate = unsafe { &mut *self.isolate.get() };
        let isolate = unsafe { v8::Isolate::ref_from_raw_isolate_ptr_mut(isolate) };
        v8::callback_scope!(unsafe let scope, isolate);
        self.context_registry
            .with_default_context(context_group_id, |default_context| {
                v8::Local::new(scope, default_context)
            })
    }
}

struct RendererInspectorIsolateBackendIdentity;

/// Opaque reference tying a renderer agent to its isolate's Inspector backend.
///
/// The reference deliberately exposes no V8 operations. The document-isolate
/// holder remains the backend owner and controls isolate entry and teardown.
#[derive(Clone)]
pub(crate) struct RendererInspectorIsolateBackendHandle {
    identity: Rc<RendererInspectorIsolateBackendIdentity>,
    pause_bridge: RendererInspectorPauseBridge,
}

impl std::fmt::Debug for RendererInspectorIsolateBackendHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RendererInspectorIsolateBackendHandle")
            .field("identity_key", &Rc::as_ptr(&self.identity))
            .finish()
    }
}

pub(in crate::script_vm) struct RendererInspectorIsolateBackend {
    identity: Rc<RendererInspectorIsolateBackendIdentity>,
    inspector: v8::inspector::V8Inspector,
    pub(super) context_registry: DocumentInspectorContextRegistry,
    unique_id_state: Rc<RendererInspectorClientUniqueIdState>,
    pause_bridge: RendererInspectorPauseBridge,
    pause_loop: Rc<RendererInspectorPauseLoopLocal>,
}

impl RendererInspectorIsolateBackend {
    pub(in crate::script_vm) fn new(isolate: &mut v8::Isolate) -> Self {
        let isolate_ptr = unsafe { isolate.as_raw_isolate_ptr() };
        let context_registry = DocumentInspectorContextRegistry::default();
        let unique_id_state = Rc::new(RendererInspectorClientUniqueIdState::new());
        let pause_bridge = RendererInspectorPauseBridge::default();
        let pause_loop = Rc::new(RendererInspectorPauseLoopLocal::new(pause_bridge.clone()));
        let inspector_client =
            v8::inspector::V8InspectorClient::new(Box::new(RendererInspectorClient::new(
                isolate_ptr,
                context_registry.clone(),
                unique_id_state.clone(),
                pause_loop.clone(),
            )));
        Self {
            identity: Rc::new(RendererInspectorIsolateBackendIdentity),
            inspector: v8::inspector::V8Inspector::create(isolate, inspector_client),
            context_registry,
            unique_id_state,
            pause_bridge,
            pause_loop,
        }
    }

    pub(crate) fn handle(&self) -> RendererInspectorIsolateBackendHandle {
        RendererInspectorIsolateBackendHandle {
            identity: Rc::clone(&self.identity),
            pause_bridge: self.pause_bridge.clone(),
        }
    }

    pub(super) fn pause_bridge(&self) -> RendererInspectorPauseBridge {
        self.pause_bridge.clone()
    }

    pub(super) fn register_pause_session(
        &self,
        context_group_id: DocumentInspectorContextGroupId,
        agent_token: RendererDevToolsAgentToken,
        session_key: DevToolsSessionKey,
        session: &Rc<v8::inspector::V8InspectorSession>,
        outbound: InspectorOutbound,
    ) -> RendererInspectorPauseSessionRegistration {
        self.pause_loop.register_session(
            context_group_id,
            agent_token,
            session_key,
            session,
            outbound,
        )
    }

    pub(super) fn connect_session(
        &mut self,
        context_group_id: DocumentInspectorContextGroupId,
        channel: v8::inspector::Channel,
        state: &[u8],
    ) -> v8::inspector::V8InspectorSession {
        self.inspector.connect(
            context_group_id.get(),
            channel,
            v8::inspector::StringView::from(state),
            v8::inspector::V8InspectorClientTrustLevel::FullyTrusted,
        )
    }

    pub(super) fn context_created_with_unique_id<'s>(
        &self,
        context: v8::Local<'s, v8::Context>,
        context_group_id: DocumentInspectorContextGroupId,
        name: &[u8],
        origin: &[u8],
        aux_data: &[u8],
    ) -> Option<String> {
        self.unique_id_state.capture_context_unique_id(|| {
            self.inspector.context_created(
                context,
                context_group_id.get(),
                v8::inspector::StringView::from(name),
                v8::inspector::StringView::from(origin),
                v8::inspector::StringView::from(aux_data),
            );
        })
    }

    pub(in crate::script_vm) fn context_destroyed<'s>(&self, context: v8::Local<'s, v8::Context>) {
        self.inspector.context_destroyed(context);
    }

    fn reset_context_group(&self, context_group_id: DocumentInspectorContextGroupId) {
        self.inspector.reset_context_group(context_group_id.get());
    }

    pub(super) fn default_context_destroyed<'s>(
        &self,
        context_group_id: DocumentInspectorContextGroupId,
        registration_id: DocumentInspectorContextRegistrationId,
        context: v8::Local<'s, v8::Context>,
    ) {
        if self
            .context_registry
            .default_context_is_owned_by(context_group_id, registration_id)
        {
            self.reset_context_group(context_group_id);
            self.context_registry
                .remove_default_context_if_owned_by(context_group_id, registration_id);
        }
        self.inspector.context_destroyed(context);
    }

    pub(super) fn detach_default_context_if_same(
        &self,
        context_group_id: DocumentInspectorContextGroupId,
        registration_id: DocumentInspectorContextRegistrationId,
    ) {
        if self
            .context_registry
            .default_context_is_owned_by(context_group_id, registration_id)
        {
            self.reset_context_group(context_group_id);
            self.context_registry
                .remove_default_context_if_owned_by(context_group_id, registration_id);
        }
    }

    pub(super) fn reset_default_context_group_before_replacement(
        &self,
        context_group_id: DocumentInspectorContextGroupId,
    ) -> bool {
        if self.context_registry.has_default_context(context_group_id) {
            self.reset_context_group(context_group_id);
            self.context_registry
                .remove_default_context(context_group_id);
            true
        } else {
            false
        }
    }

    pub(in crate::script_vm) fn default_context_registry_count(&self) -> usize {
        self.context_registry.len()
    }
}

impl RendererInspectorIsolateBackendHandle {
    pub(super) fn pause_bridge(&self) -> RendererInspectorPauseBridge {
        self.pause_bridge.clone()
    }

    pub(super) fn assert_matches(&self, backend: &RendererInspectorIsolateBackend) {
        assert!(
            Rc::ptr_eq(&self.identity, &backend.identity),
            "renderer DevTools agent used a different isolate Inspector backend"
        );
    }

    #[cfg(test)]
    pub(super) fn new_for_test() -> Self {
        Self {
            identity: Rc::new(RendererInspectorIsolateBackendIdentity),
            pause_bridge: RendererInspectorPauseBridge::default(),
        }
    }

    #[cfg(test)]
    pub(super) fn is_same_backend(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
