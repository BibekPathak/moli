use tokio::sync::{mpsc, oneshot};

use crate::runtime::{RendererRuntimeInspectorMessage, RendererRuntimeInspectorResponseSender};
use crate::worker::{WorkerHandle, WorkerMessage};

use super::RendererBrowserContextRuntime;

impl RendererBrowserContextRuntime {
    /// Allocates a browser-context-unique identity for one DedicatedWorker
    /// lifetime. The JavaScript-facing worker id is Page-local and therefore
    /// cannot safely key protocol targets after Page replacement.
    pub(crate) fn allocate_dedicated_worker_instance_id(&self) -> u64 {
        self.inner
            .next_dedicated_worker_instance_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(1)
    }

    pub(crate) fn attach_dedicated_worker_devtools_handle(
        &self,
        instance_id: u64,
        handle: &WorkerHandle,
    ) {
        self.attach_dedicated_worker_devtools_sender(instance_id, handle.tx.clone());
    }

    fn attach_dedicated_worker_devtools_sender(
        &self,
        instance_id: u64,
        sender: mpsc::UnboundedSender<WorkerMessage>,
    ) {
        self.inner
            .dedicated_worker_devtools_senders
            .lock()
            .insert(instance_id, sender);
    }

    pub(crate) fn unregister_dedicated_worker_devtools_handle(&self, instance_id: u64) {
        self.inner
            .dedicated_worker_devtools_senders
            .lock()
            .remove(&instance_id);
    }

    pub fn set_dedicated_worker_pause_on_start_for_devtools(&self, pause: bool) {
        self.inner
            .dedicated_worker_pause_on_start_for_devtools
            .store(pause, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn dedicated_worker_pause_on_start_for_devtools(&self) -> bool {
        self.inner
            .dedicated_worker_pause_on_start_for_devtools
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn dedicated_worker_devtools_sender(
        &self,
        instance_id: u64,
    ) -> Option<mpsc::UnboundedSender<WorkerMessage>> {
        self.inner
            .dedicated_worker_devtools_senders
            .lock()
            .get(&instance_id)
            .cloned()
    }

    pub async fn dispatch_dedicated_worker_runtime_protocol_message(
        &self,
        instance_id: u64,
        inspector_session_id: Option<String>,
        raw_json: String,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        self.dispatch_dedicated_worker_runtime_protocol_message_with_optional_deferred_response(
            instance_id,
            inspector_session_id,
            raw_json,
            None,
        )
        .await
    }

    pub async fn dispatch_dedicated_worker_runtime_protocol_message_with_deferred_response(
        &self,
        instance_id: u64,
        inspector_session_id: Option<String>,
        raw_json: String,
        deferred_response: RendererRuntimeInspectorResponseSender,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        self.dispatch_dedicated_worker_runtime_protocol_message_with_optional_deferred_response(
            instance_id,
            inspector_session_id,
            raw_json,
            Some(deferred_response),
        )
        .await
    }

    async fn dispatch_dedicated_worker_runtime_protocol_message_with_optional_deferred_response(
        &self,
        instance_id: u64,
        inspector_session_id: Option<String>,
        raw_json: String,
        deferred_response: Option<RendererRuntimeInspectorResponseSender>,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        let Some(sender) = self.dedicated_worker_devtools_sender(instance_id) else {
            return Err("DedicatedWorkerRuntimeUnavailable".to_owned());
        };
        let (response_tx, response_rx) = oneshot::channel();
        sender
            .send(WorkerMessage::DispatchRuntimeProtocolMessage {
                inspector_session_id,
                raw_json,
                deferred_response,
                response_tx,
            })
            .map_err(|_| "DedicatedWorkerRuntimeUnavailable".to_owned())?;
        response_rx
            .await
            .map_err(|_| "DedicatedWorkerRuntimeUnavailable".to_owned())?
    }

    pub fn attach_dedicated_worker_runtime_inspector_session(
        &self,
        instance_id: u64,
        inspector_session_id: Option<String>,
    ) -> bool {
        self.dedicated_worker_devtools_sender(instance_id)
            .is_some_and(|sender| {
                sender
                    .send(WorkerMessage::AttachRuntimeInspectorSession {
                        inspector_session_id,
                    })
                    .is_ok()
            })
    }

    pub fn detach_dedicated_worker_runtime_inspector_session(
        &self,
        instance_id: u64,
        inspector_session_id: Option<String>,
    ) -> bool {
        self.dedicated_worker_devtools_sender(instance_id)
            .is_some_and(|sender| {
                sender
                    .send(WorkerMessage::DetachRuntimeInspectorSession {
                        inspector_session_id,
                    })
                    .is_ok()
            })
    }

    pub fn run_dedicated_worker_if_waiting_for_debugger_for_devtools(
        &self,
        instance_id: u64,
    ) -> bool {
        self.dedicated_worker_devtools_sender(instance_id)
            .is_some_and(|sender| {
                sender
                    .send(WorkerMessage::RunIfWaitingForDebuggerForDevtools)
                    .is_ok()
            })
    }

    pub fn close_dedicated_worker_for_devtools(&self, instance_id: u64) -> bool {
        self.dedicated_worker_devtools_sender(instance_id)
            .is_some_and(|sender| sender.send(WorkerMessage::Terminate).is_ok())
    }
}
