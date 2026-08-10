use anyhow::{Result, anyhow};
use moli_encoding::HtmlDocumentStreamingDecoder;
use std::time::Instant;

use super::streaming::{
    RawDocumentBodySource, enqueue_streaming_raw_chunk, sync_document_character_set_from_decoder,
};
use super::streaming_input::{StreamingDocumentInputEvent, StreamingDocumentInputSource};
use super::{
    ConcurrentParseTimeRuntime, PageVm, ParseTimePageVmCreationOutcome,
    ParseTimePageVmStreamingProgress, PendingPhaseOneResidence, ServiceWorkerScriptPreloadContext,
};

/// Exact, parkable continuation for an open main-Document response.
///
/// The parser runtime, decoder, preload state and raw-input residence move as
/// one value into the stable Page slot. Page task wakes and body-input wakes
/// can then resume the same continuation without keeping an owner-local future
/// borrowed across network input.
pub(in crate::runtime) struct PendingStreamingPhaseOneContinuation {
    runtime: Box<ConcurrentParseTimeRuntime>,
    input: StreamingDocumentInputSource,
    decoder: HtmlDocumentStreamingDecoder,
    service_worker_preload_context: Option<ServiceWorkerScriptPreloadContext>,
    started: Instant,
}

impl PendingStreamingPhaseOneContinuation {
    pub(super) fn bridge(
        runtime: ConcurrentParseTimeRuntime,
        raw_body: RawDocumentBodySource,
        decoder: HtmlDocumentStreamingDecoder,
        service_worker_preload_context: Option<ServiceWorkerScriptPreloadContext>,
        started: Instant,
    ) -> Result<Self> {
        let parser_continuation = runtime
            .page_vm()
            .vm()
            .document_runtime
            .main_parser_continuation_producer()
            .ok_or_else(|| {
                anyhow!("open streaming parser continuation requires an active Networking producer")
            })?;
        let task_runner = runtime.page_vm().resource_task_runner();
        Ok(Self {
            runtime: Box::new(runtime),
            input: StreamingDocumentInputSource::bridge(raw_body, parser_continuation, task_runner),
            decoder,
            service_worker_preload_context,
            started,
        })
    }

    pub(in crate::runtime) fn page_vm(&self) -> &PageVm {
        self.runtime.page_vm()
    }

    pub(in crate::runtime) fn page_vm_mut(&mut self) -> &mut PageVm {
        self.runtime.page_vm_mut()
    }

    pub(in crate::runtime) fn owner_wake_token(&self) -> Option<crate::runtime::RendererPageToken> {
        self.runtime.owner_wake_token()
    }

    pub(in crate::runtime) fn has_ready_input(&mut self) -> bool {
        self.input.has_ready_input()
    }

    pub(in crate::runtime) async fn resume(self) -> Result<ParseTimePageVmCreationOutcome> {
        let Self {
            runtime,
            mut input,
            mut decoder,
            service_worker_preload_context,
            started,
        } = self;
        let mut runtime = *runtime;
        let mut input_finished = false;
        let mut observed_input = false;
        while let Some(event) = input.try_next()? {
            observed_input = true;
            match event {
                StreamingDocumentInputEvent::Chunks(chunks) => {
                    for chunk in chunks {
                        enqueue_streaming_raw_chunk(
                            &mut runtime,
                            &mut decoder,
                            chunk,
                            service_worker_preload_context.as_ref(),
                        );
                    }
                }
                StreamingDocumentInputEvent::Finished(result) => {
                    result.map_err(|message| anyhow!(message))?;
                    if let Some(tail) = decoder.finish() {
                        runtime.enqueue_streaming_html_chunk(tail);
                    }
                    sync_document_character_set_from_decoder(&mut runtime, &decoder);
                    runtime.close_streaming_html_input();
                    input_finished = true;
                    break;
                }
            }
        }

        // Body input and parse-time script terminals wake the same parked
        // continuation, but they do not grant the same owner transition.
        // Input (including EOF) belongs to the parser and must reach its fresh
        // owner/cutoff decision before an unrelated async terminal that merely
        // happened to be resident at the same time. Only a wake with no input
        // fact may materialize the exact parse-time Document task here.
        if !observed_input {
            let _ = runtime.admit_ready_open_stream_document_work();
        }

        let progress = runtime
            .continue_streaming_creation_on_execution_context(started)
            .await?;
        match progress {
            ParseTimePageVmStreamingProgress::NeedMoreInput(runtime) if !input_finished => {
                let continuation = Self {
                    runtime,
                    input,
                    decoder,
                    service_worker_preload_context,
                    started,
                };
                Ok(ParseTimePageVmCreationOutcome::PendingPhaseOne(
                    PendingPhaseOneResidence::open_streaming(Box::new(continuation)),
                ))
            }
            ParseTimePageVmStreamingProgress::PendingPageTask(runtime) if !input_finished => {
                let continuation = Self {
                    runtime,
                    input,
                    decoder,
                    service_worker_preload_context,
                    started,
                };
                Ok(ParseTimePageVmCreationOutcome::PendingPhaseOne(
                    PendingPhaseOneResidence::open_streaming(Box::new(continuation)),
                ))
            }
            ParseTimePageVmStreamingProgress::NeedMoreInput(runtime)
                if runtime.has_pending_parser_blocking_source_load() =>
            {
                Ok(ParseTimePageVmCreationOutcome::PendingPhaseOne(
                    PendingPhaseOneResidence::parser_blocking_source_load(runtime, started),
                ))
            }
            ParseTimePageVmStreamingProgress::PendingPageTask(runtime) => {
                Ok(ParseTimePageVmCreationOutcome::PendingPhaseOne(
                    PendingPhaseOneResidence::closed_input_page_work(runtime, started),
                ))
            }
            ParseTimePageVmStreamingProgress::NeedMoreInput(_) => Err(anyhow!(
                "closed streaming html input should not stall waiting for more input"
            )),
            ParseTimePageVmStreamingProgress::TriggeredNavigation { page_vm, stage } => {
                Ok(ParseTimePageVmCreationOutcome::TriggeredNavigation { page_vm, stage })
            }
            ParseTimePageVmStreamingProgress::ContinuePhaseTwo {
                page_vm,
                page_tasks,
                stage,
                started,
            } => Ok(ParseTimePageVmCreationOutcome::ContinuePhaseTwo {
                page_vm,
                page_tasks,
                stage,
                started,
            }),
        }
    }
}
