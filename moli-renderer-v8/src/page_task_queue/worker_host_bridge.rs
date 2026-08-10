//! Page-owned Worker host/control bridge tasks.
//!
//! DedicatedWorker client callbacks use their own typed source. This bridge
//! carries the remaining host-facing records (Network/Fetch/WebSocket,
//! console and relay terminal markers) in relay FIFO order. The queue lives
//! for the Page lifetime, while every task is stamped with the root Document
//! that supplied the producer so a late old-PageVm record cannot mutate a
//! replacement PageVm whose Worker ids happen to collide.

use moli_shared_worker::SharedWorkerInstanceId;

use crate::{
    runtime::{PageOwnerTurnOutcome, RendererDocumentToken, RendererOwnerResourceActivitySource},
    types::DedicatedWorkerId,
    worker::{WorkerRuntimeEvent, WorkerToParentMessage},
};

use super::networking::{RendererPageNetworkingRoute, RendererPageNetworkingTask};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RendererWorkerHostBridgeTarget {
    Dedicated(DedicatedWorkerId),
    Shared(SharedWorkerInstanceId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RendererPageWorkerHostBridgeOwner {
    root_document: RendererDocumentToken,
    target: RendererWorkerHostBridgeTarget,
}

impl RendererPageWorkerHostBridgeOwner {
    const fn new(
        root_document: RendererDocumentToken,
        target: RendererWorkerHostBridgeTarget,
    ) -> Self {
        Self {
            root_document,
            target,
        }
    }

    pub(crate) const fn root_document(self) -> RendererDocumentToken {
        self.root_document
    }

    #[cfg(test)]
    pub(crate) const fn target(self) -> RendererWorkerHostBridgeTarget {
        self.target
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkerHostBridgeActivity {
    Subresource,
    FetchInterception,
    FetchCancellation,
    ContinueEvent,
    WebSocket,
    Console,
}

impl WorkerHostBridgeActivity {
    const fn activity_source(self) -> RendererOwnerResourceActivitySource {
        match self {
            Self::Subresource => RendererOwnerResourceActivitySource::WorkerSubresource,
            Self::FetchInterception => RendererOwnerResourceActivitySource::WorkerFetchInterception,
            Self::FetchCancellation => RendererOwnerResourceActivitySource::WorkerFetchCancellation,
            Self::ContinueEvent => RendererOwnerResourceActivitySource::WorkerContinueEvent,
            Self::WebSocket => RendererOwnerResourceActivitySource::WorkerWebSocket,
            Self::Console => RendererOwnerResourceActivitySource::Worker,
        }
    }
}

pub(crate) fn worker_host_bridge_activity(
    message: &WorkerToParentMessage,
) -> Option<WorkerHostBridgeActivity> {
    match message {
        WorkerToParentMessage::SubresourceNetwork(_) => Some(WorkerHostBridgeActivity::Subresource),
        WorkerToParentMessage::PendingSubresourceFetch(_) => {
            Some(WorkerHostBridgeActivity::FetchInterception)
        }
        WorkerToParentMessage::PendingSubresourceFetchCanceled { .. } => {
            Some(WorkerHostBridgeActivity::FetchCancellation)
        }
        WorkerToParentMessage::SubresourceContinue(_) => {
            Some(WorkerHostBridgeActivity::ContinueEvent)
        }
        WorkerToParentMessage::WebSocketSubresource(_)
        | WorkerToParentMessage::WebSocketLifecycle(_)
        | WorkerToParentMessage::WebSocketFrame(_) => Some(WorkerHostBridgeActivity::WebSocket),
        WorkerToParentMessage::Console(_) => Some(WorkerHostBridgeActivity::Console),
        WorkerToParentMessage::Post(_)
        | WorkerToParentMessage::Error { .. }
        | WorkerToParentMessage::RuntimeInspectorMessages(_)
        | WorkerToParentMessage::ServiceWorkerLifecycleCompleted(_)
        | WorkerToParentMessage::ServiceWorkerFetchCompleted(_)
        | WorkerToParentMessage::ServiceWorkerFetchStreamStarted(_)
        | WorkerToParentMessage::ServiceWorkerFetchStreamChunk(_)
        | WorkerToParentMessage::ServiceWorkerMessageCompleted(_)
        | WorkerToParentMessage::ServiceWorkerNotificationCompleted(_)
        | WorkerToParentMessage::ServiceWorkerPushCompleted(_)
        | WorkerToParentMessage::ServiceWorkerPushSubscribe(_)
        | WorkerToParentMessage::ServiceWorkerPushGetSubscription(_)
        | WorkerToParentMessage::ServiceWorkerPushUnsubscribe(_)
        | WorkerToParentMessage::ServiceWorkerSyncCompleted(_)
        | WorkerToParentMessage::ServiceWorkerPeriodicSyncCompleted(_)
        | WorkerToParentMessage::ServiceWorkerShowNotification(_)
        | WorkerToParentMessage::ServiceWorkerGetNotifications(_)
        | WorkerToParentMessage::ServiceWorkerSyncRegistration(_)
        | WorkerToParentMessage::ServiceWorkerSyncGetTags(_)
        | WorkerToParentMessage::ServiceWorkerPeriodicSyncRegistration(_)
        | WorkerToParentMessage::ServiceWorkerPeriodicSyncGetTags(_)
        | WorkerToParentMessage::ServiceWorkerPeriodicSyncUnregistration(_)
        | WorkerToParentMessage::ServiceWorkerCloseNotification(_)
        | WorkerToParentMessage::ServiceWorkerClientMessage(_)
        | WorkerToParentMessage::ServiceWorkerWorkerMessage(_)
        | WorkerToParentMessage::ServiceWorkerClientQuery(_)
        | WorkerToParentMessage::ServiceWorkerClientNavigate(_)
        | WorkerToParentMessage::ServiceWorkerClientFocus(_)
        | WorkerToParentMessage::ServiceWorkerClientsOpenWindow(_)
        | WorkerToParentMessage::ServiceWorkerSkipWaiting { .. }
        | WorkerToParentMessage::ServiceWorkerClientsClaim { .. }
        | WorkerToParentMessage::ServiceWorkerImportedScriptLoaded { .. }
        | WorkerToParentMessage::SharedWorkerClosed
        | WorkerToParentMessage::SharedWorkerRuntimeInspectorResponse(_) => None,
    }
}

fn worker_host_bridge_target(event: &WorkerRuntimeEvent) -> RendererWorkerHostBridgeTarget {
    match event {
        WorkerRuntimeEvent::Message { worker_id, .. }
        | WorkerRuntimeEvent::HostBridgeDrained { worker_id } => {
            RendererWorkerHostBridgeTarget::Dedicated(*worker_id)
        }
        WorkerRuntimeEvent::SharedWorkerMessage { instance_id, .. } => {
            RendererWorkerHostBridgeTarget::Shared(*instance_id)
        }
    }
}

fn worker_host_bridge_activity_source_for_event(
    event: &WorkerRuntimeEvent,
) -> RendererOwnerResourceActivitySource {
    let message = match event {
        WorkerRuntimeEvent::Message { message, .. }
        | WorkerRuntimeEvent::SharedWorkerMessage { message, .. } => message,
        WorkerRuntimeEvent::HostBridgeDrained { .. } => {
            return RendererOwnerResourceActivitySource::Worker;
        }
    };
    worker_host_bridge_activity(message)
        .map(WorkerHostBridgeActivity::activity_source)
        .unwrap_or(RendererOwnerResourceActivitySource::Worker)
}

#[derive(Debug)]
pub(crate) struct RendererPageWorkerHostBridgeTask {
    owner: RendererPageWorkerHostBridgeOwner,
    activity_source: RendererOwnerResourceActivitySource,
    event: WorkerRuntimeEvent,
}

impl RendererPageWorkerHostBridgeTask {
    fn new(root_document: RendererDocumentToken, event: WorkerRuntimeEvent) -> Self {
        Self {
            owner: RendererPageWorkerHostBridgeOwner::new(
                root_document,
                worker_host_bridge_target(&event),
            ),
            activity_source: worker_host_bridge_activity_source_for_event(&event),
            event,
        }
    }

    pub(crate) const fn owner(&self) -> RendererPageWorkerHostBridgeOwner {
        self.owner
    }

    pub(crate) const fn activity_source(&self) -> RendererOwnerResourceActivitySource {
        self.activity_source
    }

    pub(crate) fn into_event(self) -> WorkerRuntimeEvent {
        self.event
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RendererWorkerHostBridgeRouteClosed;

/// PageVm-stamped producer retained by DedicatedWorker relays and
/// SharedWorker clients.
#[derive(Clone, Debug)]
pub(crate) struct RendererWorkerHostBridgeEventSender {
    route: RendererPageNetworkingRoute,
    root_document: RendererDocumentToken,
}

impl RendererWorkerHostBridgeEventSender {
    pub(crate) const fn new(
        route: RendererPageNetworkingRoute,
        root_document: RendererDocumentToken,
    ) -> Self {
        Self {
            route,
            root_document,
        }
    }

    pub(crate) fn send(
        &self,
        event: WorkerRuntimeEvent,
    ) -> Result<(), RendererWorkerHostBridgeRouteClosed> {
        self.route
            .send(RendererPageNetworkingTask::WorkerHostBridge(
                RendererPageWorkerHostBridgeTask::new(self.root_document, event),
            ))
            .map_err(|_| RendererWorkerHostBridgeRouteClosed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageWorkerHostBridgeCurrentEffect {
    /// Output/control state changed without entering the Page V8 context.
    /// The selected Page task still owns its ordinary task-end checkpoint.
    StateAppliedWithoutPageContext,
    /// The body entered the current Page context and therefore owes one
    /// ordinary selected-task checkpoint.
    StateAppliedInPageContext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageWorkerHostBridgeTargetEffect {
    AppliedToCurrentOwner(PageWorkerHostBridgeCurrentEffect),
    IgnoredStaleRoot,
    IgnoredStaleTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageWorkerHostBridgeTurnAction {
    owner: RendererPageWorkerHostBridgeOwner,
    activity_source: RendererOwnerResourceActivitySource,
    target_effect: PageWorkerHostBridgeTargetEffect,
}

impl PageWorkerHostBridgeTurnAction {
    pub(crate) const fn new(
        owner: RendererPageWorkerHostBridgeOwner,
        activity_source: RendererOwnerResourceActivitySource,
        target_effect: PageWorkerHostBridgeTargetEffect,
    ) -> Self {
        Self {
            owner,
            activity_source,
            target_effect,
        }
    }

    #[cfg(test)]
    pub(crate) const fn owner(self) -> RendererPageWorkerHostBridgeOwner {
        self.owner
    }

    #[cfg(test)]
    pub(crate) const fn activity_source(self) -> RendererOwnerResourceActivitySource {
        self.activity_source
    }

    pub(crate) const fn target_effect(self) -> PageWorkerHostBridgeTargetEffect {
        self.target_effect
    }

    pub(crate) const fn requires_output_capture(self) -> bool {
        matches!(
            self.target_effect,
            PageWorkerHostBridgeTargetEffect::AppliedToCurrentOwner(_)
        )
    }
}

pub(crate) type PageWorkerHostBridgeTurnOutcome =
    PageOwnerTurnOutcome<PageWorkerHostBridgeTurnAction>;

#[cfg(test)]
mod tests {
    use crate::{
        page_task_queue::{
            PageRuntimeWakeSignal, RendererOwnerWakeSender, RendererOwnerWakeSource,
            RendererPageNetworkingSource, RendererPageNetworkingTask,
        },
        runtime::{RendererDocumentToken, RendererOwnerResourceActivitySource, RendererPageToken},
        types::{DedicatedWorkerId, PendingSubresourceContinueEvent},
        worker::{WorkerRuntimeEvent, WorkerToParentMessage},
    };
    use moli_shared_worker::SharedWorkerInstanceId;

    use super::{RendererWorkerHostBridgeEventSender, RendererWorkerHostBridgeTarget};

    fn root_document(generation: u64) -> RendererDocumentToken {
        RendererDocumentToken {
            page_id: crate::PageId::new_for_testing(71),
            generation,
        }
    }

    fn dedicated_worker_id(raw: u64) -> DedicatedWorkerId {
        DedicatedWorkerId::new(raw)
    }

    fn pop_worker_task(
        source: &mut RendererPageNetworkingSource,
    ) -> super::RendererPageWorkerHostBridgeTask {
        let (_, task) = source
            .pop_front_task()
            .expect("Worker host bridge task should remain in the networking FIFO");
        let RendererPageNetworkingTask::WorkerHostBridge(task) = task else {
            panic!("Worker host bridge producer must not emit another networking task")
        };
        task
    }

    #[test]
    fn producer_stamps_exact_root_and_publishes_only_a_networking_wake() {
        let (wake_tx, mut wake_rx) = tokio::sync::mpsc::unbounded_channel();
        let page_token = RendererPageToken::new_for_testing(crate::PageId::new_for_testing(71));
        let mut source = RendererPageNetworkingSource::new_owner_attached(
            PageRuntimeWakeSignal::default(),
            RendererOwnerWakeSender::new(wake_tx, page_token),
        );
        let expected_root = root_document(3);
        let sender = RendererWorkerHostBridgeEventSender::new(source.route(), expected_root);
        let worker_id = dedicated_worker_id(8);

        sender
            .send(WorkerRuntimeEvent::HostBridgeDrained { worker_id })
            .expect("Worker host terminal should enter the networking FIFO");

        let wake = wake_rx
            .try_recv()
            .expect("empty-to-ready networking admission must wake the Page owner");
        assert_eq!(
            wake.source_for_test(),
            RendererOwnerWakeSource::NetworkingTask
        );
        assert!(wake_rx.try_recv().is_err(), "one admission needs one wake");

        let task = pop_worker_task(&mut source);
        assert_eq!(task.owner().root_document(), expected_root);
        assert_eq!(
            task.owner().target(),
            RendererWorkerHostBridgeTarget::Dedicated(worker_id)
        );
    }

    #[test]
    fn fetch_cancellation_and_continue_keep_specific_activity_sources() {
        let mut source = RendererPageNetworkingSource::new_for_test();
        let sender = RendererWorkerHostBridgeEventSender::new(source.route(), root_document(1));
        let worker_id = dedicated_worker_id(7);

        sender
            .send(WorkerRuntimeEvent::Message {
                worker_id,
                message: Box::new(WorkerToParentMessage::PendingSubresourceFetchCanceled {
                    fetch_id: 99,
                    error_text: "blocked".to_owned(),
                }),
            })
            .expect("fetch cancellation should send");
        sender
            .send(WorkerRuntimeEvent::Message {
                worker_id,
                message: Box::new(WorkerToParentMessage::SubresourceContinue(
                    PendingSubresourceContinueEvent::Completed { internal_id: 100 },
                )),
            })
            .expect("continue event should send");

        assert_eq!(
            pop_worker_task(&mut source).activity_source(),
            RendererOwnerResourceActivitySource::WorkerFetchCancellation
        );
        assert_eq!(
            pop_worker_task(&mut source).activity_source(),
            RendererOwnerResourceActivitySource::WorkerContinueEvent
        );
    }

    #[test]
    fn dedicated_worker_terminal_stays_after_the_prior_bridge_burst() {
        let mut source = RendererPageNetworkingSource::new_for_test();
        let sender = RendererWorkerHostBridgeEventSender::new(source.route(), root_document(1));
        let worker_id = dedicated_worker_id(7);

        // Source-level fairness may yield after eight Page turns. Keeping the
        // relay terminal in this same FIFO prevents it from overtaking an
        // older ninth host record when the source is selected again.
        for fetch_id in 1..=9 {
            sender
                .send(WorkerRuntimeEvent::Message {
                    worker_id,
                    message: Box::new(WorkerToParentMessage::PendingSubresourceFetchCanceled {
                        fetch_id,
                        error_text: "closed".to_owned(),
                    }),
                })
                .expect("DedicatedWorker host record should send");
        }
        sender
            .send(WorkerRuntimeEvent::HostBridgeDrained { worker_id })
            .expect("DedicatedWorker terminal should share the host bridge FIFO");

        for expected_fetch_id in 1..=9 {
            assert!(matches!(
                pop_worker_task(&mut source).into_event(),
                WorkerRuntimeEvent::Message {
                    worker_id: queued_worker_id,
                    message,
                } if queued_worker_id == worker_id
                    && matches!(
                        message.as_ref(),
                        WorkerToParentMessage::PendingSubresourceFetchCanceled {
                            fetch_id,
                            ..
                        } if *fetch_id == expected_fetch_id
                    )
            ));
        }
        assert!(matches!(
            pop_worker_task(&mut source).into_event(),
            WorkerRuntimeEvent::HostBridgeDrained {
                worker_id: queued_worker_id,
            } if queued_worker_id == worker_id
        ));
    }

    #[test]
    fn shared_worker_target_and_activity_are_not_inferred_from_dedicated_ids() {
        let mut source = RendererPageNetworkingSource::new_for_test();
        let sender = RendererWorkerHostBridgeEventSender::new(source.route(), root_document(1));
        let instance_id = SharedWorkerInstanceId::from_u64(7);

        sender
            .send(WorkerRuntimeEvent::SharedWorkerMessage {
                instance_id,
                message: Box::new(WorkerToParentMessage::PendingSubresourceFetchCanceled {
                    fetch_id: 99,
                    error_text: "blocked".to_owned(),
                }),
            })
            .expect("SharedWorker host record should send");

        let task = pop_worker_task(&mut source);
        assert_eq!(
            task.owner().target(),
            RendererWorkerHostBridgeTarget::Shared(instance_id)
        );
        assert_eq!(
            task.activity_source(),
            RendererOwnerResourceActivitySource::WorkerFetchCancellation
        );
    }

    #[test]
    fn closed_page_route_rejects_worker_record_without_fallback() {
        let source = RendererPageNetworkingSource::new_for_test();
        let sender = RendererWorkerHostBridgeEventSender::new(source.route(), root_document(1));
        drop(source);

        assert!(
            sender
                .send(WorkerRuntimeEvent::HostBridgeDrained {
                    worker_id: dedicated_worker_id(7),
                })
                .is_err(),
            "a retired Page route must not resurrect Worker work in a fallback queue"
        );
    }

    #[test]
    fn page_worker_action_accessors_preserve_the_applied_fact() {
        let owner = super::RendererPageWorkerHostBridgeOwner::new(
            root_document(1),
            RendererWorkerHostBridgeTarget::Dedicated(dedicated_worker_id(7)),
        );
        let action = super::PageWorkerHostBridgeTurnAction::new(
            owner,
            RendererOwnerResourceActivitySource::Worker,
            super::PageWorkerHostBridgeTargetEffect::AppliedToCurrentOwner(
                super::PageWorkerHostBridgeCurrentEffect::StateAppliedWithoutPageContext,
            ),
        );

        assert_eq!(action.owner(), owner);
        assert_eq!(
            action.activity_source(),
            RendererOwnerResourceActivitySource::Worker
        );
        assert_eq!(
            action.target_effect(),
            super::PageWorkerHostBridgeTargetEffect::AppliedToCurrentOwner(
                super::PageWorkerHostBridgeCurrentEffect::StateAppliedWithoutPageContext,
            )
        );
    }
}
