use super::parser_blocking_owner::MainParserBlockingSourceLoadWaitOwner;
use super::*;

pub(super) struct ParseTimeDriverState {
    pub(super) final_url: Url,
    pub(super) document_character_set: String,
    pub(super) parser_session: DocumentParserSession,
    pub(super) scheduler: DocumentScriptScheduler,
    pub(super) pending_parsing_blocking_script: PendingParsingBlockingClassicScriptRunner,
    pub(super) buffered_document_preloads: Box<BufferedDocumentPreloadState>,
    pub(super) service_worker_preload_context: Option<ServiceWorkerScriptPreloadContext>,
    pub(super) input_closed: bool,
}

impl ParseTimeDriverState {
    pub(super) fn new(final_url: Url) -> Self {
        Self {
            parser_session: DocumentParserSession::start_main_document(final_url.clone()),
            final_url,
            document_character_set: "UTF-8".to_owned(),
            scheduler: DocumentScriptScheduler::new(),
            pending_parsing_blocking_script: PendingParsingBlockingClassicScriptRunner::empty(),
            buffered_document_preloads: Box::default(),
            service_worker_preload_context: None,
            input_closed: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParseTimeOwner {
    Parser,
    Document,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingParsingBlockingWait {
    None,
    LegacyDocumentProcessing,
    PageTaskBlockingStylesheet,
    PageNetworkingDocumentWriteExternalScript,
}

impl PendingParsingBlockingWait {
    pub(super) const fn is_pending(self) -> bool {
        !matches!(self, Self::None)
    }

    pub(super) const fn waits_for_legacy_document_processing(self) -> bool {
        matches!(self, Self::LegacyDocumentProcessing)
    }

    pub(super) const fn waits_for_page_task(self) -> bool {
        matches!(
            self,
            Self::PageTaskBlockingStylesheet | Self::PageNetworkingDocumentWriteExternalScript
        )
    }
}

pub(in crate::runtime) struct ConcurrentParseTimeRuntime {
    pub(super) loader: ResourceRequestClient,
    pub(super) stage: PageVmInitStage,
    pub(super) state: ParseTimeDriverState,
    pub(super) page_vm: PageVm,
    pub(super) parser_document_owner: crate::frame_owner_model::FrameDocumentTaskOwner,
    pub(super) owner: ParseTimeOwner,
    pub(super) parser_step_ready: bool,
    pub(super) pending_parsing_blocking_wait: PendingParsingBlockingWait,
}

impl ConcurrentParseTimeRuntime {
    pub(in crate::runtime) fn page_vm(&self) -> &PageVm {
        &self.page_vm
    }

    pub(in crate::runtime) fn page_vm_mut(&mut self) -> &mut PageVm {
        &mut self.page_vm
    }

    pub(super) fn new_parser_owner(
        loader: ResourceRequestClient,
        stage: PageVmInitStage,
        state: ParseTimeDriverState,
        mut page_vm: PageVm,
    ) -> Self {
        page_vm.set_target_stage(stage);
        let parser_document_owner = page_vm
            .vm()
            .current_main_document_task_owner()
            .expect("phase-one parser runtime requires an installed main document owner");
        page_vm
            .vm_mut()
            .document_runtime
            .activate_main_parser_continuation(parser_document_owner);
        Self {
            loader,
            stage,
            state,
            page_vm,
            parser_document_owner,
            owner: ParseTimeOwner::Parser,
            parser_step_ready: false,
            pending_parsing_blocking_wait: PendingParsingBlockingWait::None,
        }
    }

    pub(super) fn has_pending_parser_blocking_source_load(&self) -> bool {
        self.pending_parser_blocking_source_load().is_some()
    }

    /// Consume the permission granted by one selected Networking continuation.
    ///
    /// The task never carries parser state. It merely lets this sole runtime
    /// re-enter the parser after the producer has committed input/resource
    /// state in its authoritative store.
    pub(super) fn admit_selected_main_parser_continuation(&mut self) -> bool {
        if !self
            .page_vm
            .vm_mut()
            .document_runtime
            .take_main_parser_continuation_admission()
        {
            return false;
        }
        self.owner = ParseTimeOwner::Parser;
        self.parser_step_ready = true;
        self.pending_parsing_blocking_wait = PendingParsingBlockingWait::None;
        true
    }

    pub(super) fn pending_parser_blocking_source_load(
        &self,
    ) -> Option<crate::planning::SharedScriptSourceLoad> {
        let mut owner = MainParserBlockingSourceLoadWaitOwner;
        self.state
            .pending_parsing_blocking_script
            .current_parser_blocking_source_load_wait_action_with_owner(&mut owner)
    }

    pub(super) fn has_unready_pending_parser_blocking_source_load(&self) -> bool {
        self.pending_parser_blocking_source_load()
            .is_some_and(|load| load.try_outcome().is_none())
    }

    pub(super) async fn run_one_page_creation_event_loop_turn(
        &mut self,
    ) -> Result<PageTaskTurnResult> {
        let ConcurrentParseTimeRuntime { state, page_vm, .. } = self;
        let mut context = DocumentTurnContext {
            scheduler: &mut state.scheduler,
            stream: state.parser_session.stream_handle(),
        };
        context.run_parse_time_turn(page_vm).await
    }

    pub(in crate::runtime) fn owner_wake_token(&self) -> Option<crate::runtime::RendererPageToken> {
        self.page_vm
            .runtime_hooks
            .owner_wake()
            .map(|owner_wake| owner_wake.token())
    }
}
