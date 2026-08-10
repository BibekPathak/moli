use super::load_delivery_tasks::FrameDocumentLoadDeliveryAdmissionId;
use super::records::FrameRealmId;
use super::records::{
    DocumentId, DocumentLoadDelayTokenId, FrameNavigationId, FrameRequestId, FrameSchedulerLaneId,
    LocalWindowId, WindowProxyId,
};

#[derive(Debug)]
pub(super) struct FrameOwnerIdAllocator {
    next_window_proxy_id: u64,
    next_local_window_id: u64,
    next_document_id: u64,
    next_document_load_delay_token_id: u64,
    next_document_load_delivery_admission_id: u64,
    next_frame_navigation_id: u64,
    next_frame_realm_id: i64,
    next_frame_request_id: u64,
    next_scheduler_lane_id: u64,
}

impl Default for FrameOwnerIdAllocator {
    fn default() -> Self {
        Self {
            next_window_proxy_id: 1,
            next_local_window_id: 1,
            next_document_id: 1,
            next_document_load_delay_token_id: 1,
            next_document_load_delivery_admission_id: 1,
            next_frame_navigation_id: 1,
            next_frame_realm_id: 1,
            next_frame_request_id: 1,
            next_scheduler_lane_id: 1,
        }
    }
}

impl FrameOwnerIdAllocator {
    pub(super) fn window_proxy(&mut self) -> WindowProxyId {
        let id = WindowProxyId(self.next_window_proxy_id);
        self.next_window_proxy_id += 1;
        id
    }

    pub(super) fn local_window(&mut self) -> LocalWindowId {
        let id = LocalWindowId(self.next_local_window_id);
        self.next_local_window_id += 1;
        id
    }

    pub(super) fn document(&mut self) -> DocumentId {
        let id = DocumentId(self.next_document_id);
        self.next_document_id += 1;
        id
    }

    pub(super) fn document_load_delay_token(&mut self) -> DocumentLoadDelayTokenId {
        let id = DocumentLoadDelayTokenId(self.next_document_load_delay_token_id);
        self.next_document_load_delay_token_id += 1;
        id
    }

    pub(super) fn document_load_delivery_admission(
        &mut self,
    ) -> FrameDocumentLoadDeliveryAdmissionId {
        let id =
            FrameDocumentLoadDeliveryAdmissionId(self.next_document_load_delivery_admission_id);
        self.next_document_load_delivery_admission_id += 1;
        id
    }

    pub(super) fn frame_navigation(&mut self) -> FrameNavigationId {
        let id = FrameNavigationId(self.next_frame_navigation_id);
        self.next_frame_navigation_id += 1;
        id
    }

    pub(super) fn frame_realm(&mut self) -> FrameRealmId {
        let id = FrameRealmId(self.next_frame_realm_id);
        self.next_frame_realm_id += 1;
        id
    }

    pub(super) fn frame_request(&mut self) -> FrameRequestId {
        let id = FrameRequestId(self.next_frame_request_id);
        self.next_frame_request_id += 1;
        id
    }

    pub(super) fn scheduler_lane(&mut self) -> FrameSchedulerLaneId {
        let id = FrameSchedulerLaneId(self.next_scheduler_lane_id);
        self.next_scheduler_lane_id += 1;
        id
    }
}
