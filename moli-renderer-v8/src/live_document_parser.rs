use crate::{
    DocumentOwnedBlockingStylesheetDiscoveryInput,
    dom::native::NativeNodeId,
    parser::{
        DocumentStream, HtmlParser, ParserBlockingStylesheetPause,
        ParserCustomElementConstructionHandoff, ParserDomMutationConsumer, ParserDomReadConsumer,
        ParserElementCreationConsumer, ParserMutationEffectConsumer, ParserPumpOutcome,
        ParserPumpStep, ParserScriptHandoff, ParserYield, PreparedScript,
    },
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use url::Url;

pub(crate) type DocumentParserStreamHandle = Rc<RefCell<DocumentStream>>;

fn new_document_parser_stream_handle(stream: DocumentStream) -> DocumentParserStreamHandle {
    Rc::new(RefCell::new(stream))
}

pub(crate) trait LiveDocumentParserOwner:
    ParserDomReadConsumer
    + ParserDomMutationConsumer
    + ParserMutationEffectConsumer
    + ParserElementCreationConsumer
{
}

fn pump_live_document_parser_step(
    stream: &mut DocumentStream,
    chunk: &str,
    owner: &mut impl LiveDocumentParserOwner,
) -> ParserPumpOutcome {
    stream.pump_parser_step_with_runtime_dom_consumer(chunk, owner)
}

pub(crate) enum LiveDocumentParserStepOutcome {
    /// The current parser input was consumed without producing a synchronous
    /// parser lifecycle handoff.
    Continue,
    /// The tree builder needs author-defined custom element construction before
    /// parser insertion can continue in the same document parser.
    CustomElementConstructionHandoff(Box<ParserCustomElementConstructionHandoff>),
    /// A parser-created blocking stylesheet in the body pauses token
    /// consumption while the owner keeps the same parser and buffered input.
    BlockingStylesheetPause(ParserBlockingStylesheetPause),
    /// The tree builder reached a parser-connected script boundary. The owner
    /// decides whether this executes immediately or blocks on source/resources.
    ScriptHandoff(Box<ParserScriptHandoff>),
}

struct LiveDocumentParserStepAdvance {
    outcome: LiveDocumentParserStepOutcome,
    discovery_signals: LiveDocumentParserDiscoverySignals,
}

#[derive(Debug, Default)]
pub(crate) struct LiveDocumentParserDiscoverySignals {
    pub(crate) async_prefetch_scripts: Vec<PreparedScript>,
    pub(crate) modulepreload_link_candidates: Vec<NativeNodeId>,
    pub(crate) parser_meta_csp_candidates: Vec<NativeNodeId>,
    pub(crate) blocking_stylesheet_inputs: Vec<DocumentOwnedBlockingStylesheetDiscoveryInput>,
}

pub(crate) struct DocumentParserFinishSignals {
    pub(crate) parser_created_null_registry_elements: Vec<NativeNodeId>,
    pub(crate) discovery_signals: LiveDocumentParserDiscoverySignals,
}

impl LiveDocumentParserDiscoverySignals {
    pub(crate) fn extend(&mut self, other: Self) {
        self.async_prefetch_scripts
            .extend(other.async_prefetch_scripts);
        self.modulepreload_link_candidates
            .extend(other.modulepreload_link_candidates);
        self.parser_meta_csp_candidates
            .extend(other.parser_meta_csp_candidates);
        self.blocking_stylesheet_inputs
            .extend(other.blocking_stylesheet_inputs);
    }
}

impl LiveDocumentParserStepAdvance {
    fn split(
        self,
    ) -> (
        LiveDocumentParserStepOutcome,
        LiveDocumentParserDiscoverySignals,
    ) {
        (self.outcome, self.discovery_signals)
    }
}

fn advance_live_document_parser_step<Driver>(
    stream: &mut DocumentStream,
    parser_step: &str,
    driver: &mut Driver,
) -> LiveDocumentParserStepAdvance
where
    Driver: LiveDocumentParserOwner,
{
    let ParserPumpOutcome {
        result,
        discovered_async_prefetch_scripts,
        discovered_modulepreload_link_candidates,
        discovered_blocking_stylesheet_inputs,
    } = pump_live_document_parser_step(stream, parser_step, driver);
    let discovered_parser_meta_csp_candidates =
        stream.drain_discovered_parser_meta_csp_candidates();
    let discovery_signals = LiveDocumentParserDiscoverySignals {
        async_prefetch_scripts: discovered_async_prefetch_scripts,
        modulepreload_link_candidates: discovered_modulepreload_link_candidates,
        parser_meta_csp_candidates: discovered_parser_meta_csp_candidates,
        blocking_stylesheet_inputs: discovered_blocking_stylesheet_inputs,
    };
    let outcome = match result {
        ParserPumpStep::InputDrained => LiveDocumentParserStepOutcome::Continue,
        ParserPumpStep::Yield(ParserYield::CustomElementConstruction(handoff)) => {
            LiveDocumentParserStepOutcome::CustomElementConstructionHandoff(handoff)
        }
        ParserPumpStep::Yield(ParserYield::BlockingStylesheet(pause)) => {
            LiveDocumentParserStepOutcome::BlockingStylesheetPause(pause)
        }
        ParserPumpStep::Yield(ParserYield::Script(handoff)) => {
            LiveDocumentParserStepOutcome::ScriptHandoff(handoff)
        }
    };
    LiveDocumentParserStepAdvance {
        outcome,
        discovery_signals,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentParserLifetime {
    /// The parser consumes a finite navigation or `srcdoc` input.
    Finite,
    /// `document.open()` created a parser that remains open across writes.
    Open,
    /// `document.close()` requested EOF; finish after the active blocker releases.
    Closing,
}

/// The canonical runtime owner of one live document parser.
///
/// Root and child documents store this same type. A `ParserInsertionController`
/// is only a temporary reentrant capability derived from its stream handle; it
/// does not own parser lifetime or suspension state.
pub(crate) struct DocumentParserSession {
    stream: DocumentParserStreamHandle,
    input: ParserInputBuffer,
    discovery_signals: LiveDocumentParserDiscoverySignals,
    lifetime: DocumentParserLifetime,
    waiting_for_blocking_stylesheet: bool,
    waiting_for_parser_script: bool,
}

impl std::fmt::Debug for DocumentParserSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DocumentParserSession")
            .field("lifetime", &self.lifetime)
            .field(
                "waiting_for_blocking_stylesheet",
                &self.waiting_for_blocking_stylesheet,
            )
            .field("waiting_for_parser_script", &self.waiting_for_parser_script)
            .finish_non_exhaustive()
    }
}

pub(crate) struct ParserInputBuffer {
    queued_chunks: VecDeque<String>,
    current_chunk: Option<String>,
}

impl ParserInputBuffer {
    pub(crate) fn new() -> Self {
        Self {
            queued_chunks: VecDeque::new(),
            current_chunk: None,
        }
    }

    pub(crate) fn queue_arrived_chunk(&mut self, html: String) {
        if !html.is_empty() {
            self.queued_chunks.push_back(html);
        }
    }

    pub(crate) fn ensure_current_chunk(&mut self, stream: &DocumentStream) {
        if let Some(script_input_html) = DocumentParserDriver::take_next_script_input(stream) {
            if let Some(network_chunk) = self.current_chunk.take()
                && !network_chunk.is_empty()
            {
                self.queued_chunks.push_front(network_chunk);
            }
            self.current_chunk = Some(script_input_html);
            return;
        }

        if self.current_chunk.is_none() {
            self.current_chunk = self.queued_chunks.pop_front();
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.current_chunk.is_none() && self.queued_chunks.is_empty()
    }

    pub(crate) fn current_chunk_len(&self) -> usize {
        self.current_chunk.as_ref().map_or(0, |chunk| chunk.len())
    }

    pub(crate) fn current_chunk_is_none(&self) -> bool {
        self.current_chunk.is_none()
    }

    pub(crate) fn current_chunk_is_empty(&self) -> bool {
        self.current_chunk
            .as_ref()
            .is_some_and(|chunk| chunk.is_empty())
    }

    pub(crate) fn take_current_chunk(&mut self) -> String {
        self.current_chunk.take().unwrap_or_default()
    }

    pub(crate) fn set_current_chunk(&mut self, chunk: Option<String>) {
        self.current_chunk = chunk;
    }

    #[cfg(test)]
    pub(crate) fn set_current_chunk_for_testing(&mut self, chunk: Option<String>) {
        self.current_chunk = chunk;
    }

    #[cfg(test)]
    pub(crate) fn current_chunk_for_testing(&self) -> Option<&str> {
        self.current_chunk.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn current_chunk_is_non_empty_for_testing(&self) -> bool {
        self.current_chunk
            .as_ref()
            .is_some_and(|chunk| !chunk.is_empty())
    }

    #[cfg(test)]
    pub(crate) fn queued_chunk_count_for_testing(&self) -> usize {
        self.queued_chunks.len()
    }

    #[cfg(test)]
    pub(crate) fn queued_front_for_testing(&self) -> Option<&str> {
        self.queued_chunks.front().map(String::as_str)
    }
}

struct DocumentParserDriver;

impl DocumentParserDriver {
    fn advance_step(
        stream: &mut DocumentStream,
        parser_step: &str,
        owner: &mut impl LiveDocumentParserOwner,
    ) -> LiveDocumentParserStepAdvance {
        advance_live_document_parser_step(stream, parser_step, owner)
    }

    fn note_defined_autonomous_custom_elements(
        stream: &mut DocumentStream,
        names: impl IntoIterator<Item = String>,
    ) {
        for name in names {
            stream.note_defined_autonomous_custom_element(&name);
        }
    }

    fn take_next_insertion_preload_input(stream: &DocumentStream) -> Option<String> {
        stream.take_next_insertion_preload_input()
    }

    fn take_processed_insertion_meta_csp_count(stream: &DocumentStream) -> usize {
        stream.take_processed_insertion_meta_csp_count()
    }

    fn take_next_script_input(stream: &DocumentStream) -> Option<String> {
        stream.take_next_script_input()
    }

    fn has_script_input(stream: &DocumentStream) -> bool {
        stream.has_script_input()
    }

    fn take_buffered_input(stream: &mut DocumentStream) -> String {
        stream.take_buffered_input()
    }

    fn take_buffered_input_with_tail(
        stream: &mut DocumentStream,
        tail: Option<String>,
    ) -> Option<String> {
        let mut input = Self::take_buffered_input(stream);
        if let Some(tail) = tail {
            input.push_str(&tail);
        }
        (!input.is_empty()).then_some(input)
    }

    fn peek_buffered_input(stream: &mut DocumentStream) -> String {
        stream.peek_buffered_input()
    }

    fn take_null_custom_element_registry_elements(
        stream: &mut DocumentStream,
    ) -> Vec<NativeNodeId> {
        stream.take_parser_stream_null_custom_element_registry_elements()
    }

    fn finish(
        stream: DocumentStream,
        owner: &mut impl LiveDocumentParserOwner,
    ) -> DocumentParserFinishSignals {
        finish_live_document_parser(stream, owner)
    }
}

impl DocumentParserSession {
    pub(crate) fn start_main_document(document_url: Url) -> Self {
        Self::new(
            HtmlParser.start_document(document_url),
            DocumentParserLifetime::Finite,
        )
    }

    pub(crate) fn start_finite_live_document(
        document_url: Url,
        document_handle: NativeNodeId,
    ) -> Self {
        Self::new(
            HtmlParser.start_live_document_root(document_url, document_handle),
            DocumentParserLifetime::Finite,
        )
    }

    pub(crate) fn start_open_live_document(
        document_url: Url,
        document_handle: NativeNodeId,
    ) -> Self {
        Self::new(
            HtmlParser.start_live_document_root(document_url, document_handle),
            DocumentParserLifetime::Open,
        )
    }

    fn new(stream: DocumentStream, lifetime: DocumentParserLifetime) -> Self {
        Self {
            stream: new_document_parser_stream_handle(stream),
            input: ParserInputBuffer::new(),
            discovery_signals: LiveDocumentParserDiscoverySignals::default(),
            lifetime,
            waiting_for_blocking_stylesheet: false,
            waiting_for_parser_script: false,
        }
    }

    pub(crate) fn stream_handle(&self) -> DocumentParserStreamHandle {
        self.stream.clone()
    }

    pub(crate) fn lifetime(&self) -> DocumentParserLifetime {
        self.lifetime
    }

    pub(crate) fn request_close(&mut self) -> bool {
        if self.lifetime == DocumentParserLifetime::Closing {
            return false;
        }
        self.lifetime = DocumentParserLifetime::Closing;
        true
    }

    pub(crate) fn finishes_when_drained(&self) -> bool {
        matches!(
            self.lifetime,
            DocumentParserLifetime::Finite | DocumentParserLifetime::Closing
        )
    }

    pub(crate) fn wait_for_blocking_stylesheet(&mut self) {
        self.waiting_for_blocking_stylesheet = true;
    }

    pub(crate) fn take_blocking_stylesheet_wait(&mut self) -> bool {
        std::mem::take(&mut self.waiting_for_blocking_stylesheet)
    }

    pub(crate) fn is_waiting_for_blocking_stylesheet(&self) -> bool {
        self.waiting_for_blocking_stylesheet
    }

    pub(crate) fn wait_for_parser_script(&mut self) {
        self.waiting_for_parser_script = true;
    }

    pub(crate) fn take_parser_script_wait(&mut self) -> bool {
        std::mem::take(&mut self.waiting_for_parser_script)
    }

    pub(crate) fn is_waiting_for_parser_script(&self) -> bool {
        self.waiting_for_parser_script
    }

    pub(crate) fn has_exclusive_stream_handle(&self) -> bool {
        Rc::strong_count(&self.stream) == 1
    }

    pub(crate) fn note_defined_autonomous_custom_elements(
        &mut self,
        names: impl IntoIterator<Item = String>,
    ) {
        DocumentParserDriver::note_defined_autonomous_custom_elements(
            &mut self.stream.borrow_mut(),
            names,
        );
    }

    pub(crate) fn queue_arrived_chunk(&mut self, html: String) {
        self.input.queue_arrived_chunk(html);
    }

    pub(crate) fn ensure_current_chunk(&mut self) {
        self.input.ensure_current_chunk(&self.stream.borrow());
    }

    pub(crate) fn has_script_input(&self) -> bool {
        DocumentParserDriver::has_script_input(&self.stream.borrow())
    }

    pub(crate) fn input_is_empty(&self) -> bool {
        self.input.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.input_is_empty()
    }

    pub(crate) fn current_chunk_len(&self) -> usize {
        self.input.current_chunk_len()
    }

    pub(crate) fn current_chunk_is_none(&self) -> bool {
        self.input.current_chunk_is_none()
    }

    pub(crate) fn current_chunk_is_empty(&self) -> bool {
        self.input.current_chunk_is_empty()
    }

    pub(crate) fn take_current_chunk(&mut self) -> String {
        self.input.take_current_chunk()
    }

    pub(crate) fn set_current_chunk(&mut self, chunk: Option<String>) {
        self.input.set_current_chunk(chunk);
    }

    pub(crate) fn take_next_insertion_preload_input(&self) -> Option<String> {
        DocumentParserDriver::take_next_insertion_preload_input(&self.stream.borrow())
    }

    pub(crate) fn take_processed_insertion_meta_csp_count(&self) -> usize {
        DocumentParserDriver::take_processed_insertion_meta_csp_count(&self.stream.borrow())
    }

    pub(crate) fn take_buffered_input_with_tail(&mut self, tail: Option<String>) -> Option<String> {
        DocumentParserDriver::take_buffered_input_with_tail(&mut self.stream.borrow_mut(), tail)
    }

    pub(crate) fn peek_buffered_input(&mut self) -> String {
        DocumentParserDriver::peek_buffered_input(&mut self.stream.borrow_mut())
    }

    pub(crate) fn advance_queued_or_resume_step(
        &mut self,
        owner: &mut impl LiveDocumentParserOwner,
    ) -> LiveDocumentParserStepOutcome {
        self.ensure_current_chunk();
        let parser_step = self.input.take_current_chunk();
        self.advance_step(&parser_step, owner)
    }

    pub(crate) fn advance_step_and_take_null_custom_element_registry_elements(
        &mut self,
        parser_step: &str,
        owner: &mut impl LiveDocumentParserOwner,
    ) -> (LiveDocumentParserStepOutcome, Vec<NativeNodeId>) {
        let (advance, null_custom_element_registry_elements) =
            self.with_reentrant_stream_step(|stream| {
                let advance = DocumentParserDriver::advance_step(stream, parser_step, owner);
                let null_custom_element_registry_elements =
                    DocumentParserDriver::take_null_custom_element_registry_elements(stream);
                (advance, null_custom_element_registry_elements)
            });
        let (outcome, discovery_signals) = advance.split();
        self.discovery_signals.extend(discovery_signals);
        (outcome, null_custom_element_registry_elements)
    }

    fn advance_step(
        &mut self,
        parser_step: &str,
        owner: &mut impl LiveDocumentParserOwner,
    ) -> LiveDocumentParserStepOutcome {
        let advance =
            DocumentParserDriver::advance_step(&mut self.stream.borrow_mut(), parser_step, owner);
        self.discovery_signals.extend(advance.discovery_signals);
        advance.outcome
    }

    pub(crate) fn take_discovery_signals(&mut self) -> LiveDocumentParserDiscoverySignals {
        std::mem::take(&mut self.discovery_signals)
    }

    pub(crate) fn finish(
        mut self,
        owner: &mut impl LiveDocumentParserOwner,
    ) -> DocumentParserFinishSignals {
        let mut discovery_signals = std::mem::take(&mut self.discovery_signals);
        let stream = match Rc::try_unwrap(self.stream) {
            Ok(stream) => stream.into_inner(),
            Err(_) => {
                panic!(
                    "live document parser session must not retain cloned stream handles at finish"
                )
            }
        };
        let mut finish_signals = DocumentParserDriver::finish(stream, owner);
        discovery_signals.extend(finish_signals.discovery_signals);
        finish_signals.discovery_signals = discovery_signals;
        finish_signals
    }

    fn with_reentrant_stream_step<R>(&self, op: impl FnOnce(&mut DocumentStream) -> R) -> R {
        let stream_ptr = self.stream.as_ref().as_ptr();
        // SAFETY: The Rc keeps the DocumentStream allocation alive for this
        // synchronous parser step, and phase-one parser turns run on the
        // renderer owner thread. We intentionally avoid a RefCell guard here
        // because TreeSink structural mutations synchronously deliver effects
        // to a runtime mutation owner. Holding RefMut<DocumentStream> across
        // that boundary would make parser-created custom element construction
        // fail before the DOM-specific reentry rules can run.
        //
        // While this operation is active, callbacks must not reenter the same
        // parser stream. Parser-connected scripts still yield through
        // ParserScriptHandoff and are not run by the parser-tree-sink mutation
        // owner. Custom element construction must enter the dynamic markup
        // insertion guard before invoking page JS, so document.write/open/close
        // throw before they can borrow this stream.
        unsafe { op(&mut *stream_ptr) }
    }

    #[cfg(test)]
    pub(crate) fn queued_chunk_count_for_testing(&self) -> usize {
        self.input.queued_chunk_count_for_testing()
    }

    #[cfg(test)]
    pub(crate) fn current_chunk_is_non_empty_for_testing(&self) -> bool {
        self.input.current_chunk_is_non_empty_for_testing()
    }
}

fn finish_live_document_parser(
    stream: DocumentStream,
    owner: &mut impl LiveDocumentParserOwner,
) -> DocumentParserFinishSignals {
    let crate::parser::ParserFinishDiscoverySignals {
        parser_created_null_registry_elements,
        discovered_modulepreload_link_candidates,
        discovered_parser_meta_csp_candidates,
        discovered_blocking_stylesheet_inputs,
    } = stream.finish_with_runtime_dom_consumer(owner);
    DocumentParserFinishSignals {
        parser_created_null_registry_elements,
        discovery_signals: LiveDocumentParserDiscoverySignals {
            modulepreload_link_candidates: discovered_modulepreload_link_candidates,
            parser_meta_csp_candidates: discovered_parser_meta_csp_candidates,
            blocking_stylesheet_inputs: discovered_blocking_stylesheet_inputs,
            ..LiveDocumentParserDiscoverySignals::default()
        },
    }
}
