use crate::{frame_owner_model::FrameDocumentTaskOwner, runtime::RendererDocumentToken};

/// Exact main-Document epoch shared by task families that target one live
/// parser/runtime instance.
///
/// The three identity layers are intentionally kept together:
///
/// - `root_document` rejects tasks from a retired PageVm generation;
/// - `document_owner` rejects `Document` replacement;
/// - `runtime_generation` rejects `document.open()` epochs that deliberately
///   keep the same V8 realm and PageVm.
///
/// A task may still be selected after this owner becomes stale. Selection
/// removes the task from its FIFO; the executor then compares this locator
/// with the current runtime and discards a mismatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RendererPageMainDocumentTaskOwner {
    root_document: RendererDocumentToken,
    document_owner: FrameDocumentTaskOwner,
    runtime_generation: u64,
}

impl RendererPageMainDocumentTaskOwner {
    pub(crate) const fn new(
        root_document: RendererDocumentToken,
        document_owner: FrameDocumentTaskOwner,
        runtime_generation: u64,
    ) -> Self {
        Self {
            root_document,
            document_owner,
            runtime_generation,
        }
    }

    pub(crate) const fn root_document(self) -> RendererDocumentToken {
        self.root_document
    }

    pub(crate) const fn document_owner(self) -> FrameDocumentTaskOwner {
        self.document_owner
    }

    pub(crate) const fn runtime_generation(self) -> u64 {
        self.runtime_generation
    }

    #[cfg(test)]
    pub(crate) const fn new_for_test(
        root_document: RendererDocumentToken,
        document_owner: FrameDocumentTaskOwner,
        runtime_generation: u64,
    ) -> Self {
        Self::new(root_document, document_owner, runtime_generation)
    }
}
