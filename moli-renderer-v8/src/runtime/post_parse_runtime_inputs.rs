use super::page_vm::PageVm;
use crate::{
    DocumentOwnedBlockingStylesheetDiscoveryInput,
    document_runtime::{
        parser_prepared_script_page_owned_work, parser_script_preparation_failure_page_owned_work,
    },
    document_script_scheduler::DocumentScriptScheduler,
    frame_owner_model::FrameDocumentTaskOwner,
    host::ScriptHandleSource,
    network::ResourceRequestClient,
    page_task_queue::PostParsePageOwnedWork,
    parser::{ParserDocumentHandoff, ParserScriptHandoff},
    types::ScriptKind,
};
use anyhow::{Result, anyhow};

pub(in crate::runtime) struct InitialDocumentScriptOwnerInput {
    owner: FrameDocumentTaskOwner,
    scheduler: DocumentScriptScheduler,
    page_owned_work: Vec<PostParsePageOwnedWork>,
    blocking_stylesheet_inputs: Vec<DocumentOwnedBlockingStylesheetDiscoveryInput>,
}

impl InitialDocumentScriptOwnerInput {
    pub(in crate::runtime) fn accept(
        page_vm: &mut PageVm,
        loader: &ResourceRequestClient,
        handoffs: Vec<ParserDocumentHandoff>,
        blocking_stylesheet_inputs: Vec<DocumentOwnedBlockingStylesheetDiscoveryInput>,
    ) -> Result<Self> {
        let owner = page_vm
            .vm()
            .current_main_document_task_owner()
            .ok_or_else(|| anyhow!("initial document script acceptance requires a main owner"))?;
        let document_character_set = page_vm
            .vm()
            .document_runtime
            .document_character_set()
            .to_owned();
        let mut scheduler = DocumentScriptScheduler::new();
        let mut page_owned_work = Vec::new();

        for handoff in handoffs {
            let ParserDocumentHandoff::Script(handoff) = handoff else {
                let ParserDocumentHandoff::ModulepreloadLink(link_handle) = handoff else {
                    unreachable!("parser document handoff variants are exhaustive")
                };
                page_vm
                    .vm_mut()
                    .accept_parser_discovered_native_modulepreloads(std::iter::once(link_handle));
                continue;
            };
            match *handoff {
                ParserScriptHandoff::BlockingClassic {
                    node_id,
                    start_line,
                    start_column,
                    blocking_signatures_before,
                    mut script,
                } => {
                    bind_initial_parser_script(
                        page_vm,
                        node_id,
                        start_line,
                        start_column,
                        &mut script,
                    );
                    page_owned_work.push(parser_prepared_script_page_owned_work(
                        script,
                        blocking_signatures_before,
                    ));
                }
                ParserScriptHandoff::AsyncPostParse {
                    node_id,
                    start_line,
                    start_column,
                    mut script,
                } => {
                    bind_initial_parser_script(
                        page_vm,
                        node_id,
                        start_line,
                        start_column,
                        &mut script,
                    );
                    if script.kind == ScriptKind::Module {
                        let accepted = page_vm
                            .vm_mut()
                            .accept_main_parser_async_module_script(owner, &script)?;
                        if !accepted {
                            return Err(anyhow!(
                                "initial parser async module could not bind its document owner"
                            ));
                        }
                        continue;
                    }
                    let resource_task_runner = page_vm.resource_task_runner();
                    if !scheduler
                        .accept_parser_discovered_async_candidate(
                            script.clone(),
                            loader,
                            resource_task_runner,
                            None,
                            Some(&document_character_set),
                            |_| {
                                page_vm
                                    .vm_mut()
                                    .accept_main_document_script_load_delay_binding(
                                        owner,
                                        crate::frame_owner_model::MainDocumentScriptLoadDelayKind::Classic,
                                    )
                                    .expect("initial parser async classic must bind lifecycle ownership")
                            },
                        )
                        || !scheduler.claim_existing_parse_time_async_handoff(script.node_id)
                    {
                        return Err(anyhow!(
                            "initial document async parser handoff could not establish discovery ownership"
                        ));
                    }
                }
                ParserScriptHandoff::NonAsyncPostParse {
                    node_id,
                    start_line,
                    start_column,
                    mut script,
                    blocking_signatures_before,
                } => {
                    bind_initial_parser_script(
                        page_vm,
                        node_id,
                        start_line,
                        start_column,
                        &mut script,
                    );
                    let accepted = page_vm.vm_mut().claim_main_parser_deferred_script(
                        owner,
                        script,
                        None,
                        Some(&document_character_set),
                        blocking_signatures_before,
                    )?;
                    if !accepted {
                        return Err(anyhow!(
                            "initial parser-deferred handoff could not bind its document owner"
                        ));
                    }
                }
                ParserScriptHandoff::ImportMap {
                    node_id,
                    start_line,
                    start_column,
                    import_map,
                } => crate::module_runtime::accept_parser_owned_import_map_handoff(
                    page_vm.vm_mut(),
                    node_id,
                    start_line,
                    start_column,
                    import_map,
                ),
                ParserScriptHandoff::NoExecution {
                    node_id,
                    start_line,
                    start_column,
                    outcome,
                } => {
                    crate::host::apply_parser_script_element_state_transition(
                        page_vm.vm_mut().document_runtime.dom_host_mut(),
                        node_id,
                        outcome.element_state_transition(),
                    );
                    page_vm
                        .vm_mut()
                        .document_runtime
                        .note_parser_script_start_position(node_id, start_line, start_column);
                    if let (_, _, Some(run)) = outcome.into_parts() {
                        page_vm.report.runs.push(run);
                    }
                }
                ParserScriptHandoff::PreparationFailure {
                    node_id,
                    start_line,
                    start_column,
                    failure,
                } => {
                    crate::host::apply_parser_script_element_state_transition(
                        page_vm.vm_mut().document_runtime.dom_host_mut(),
                        node_id,
                        failure.element_state_transition(),
                    );
                    page_vm
                        .vm_mut()
                        .document_runtime
                        .note_parser_script_start_position(node_id, start_line, start_column);
                    page_owned_work
                        .push(parser_script_preparation_failure_page_owned_work(failure));
                }
            }
        }

        Ok(Self {
            owner,
            scheduler,
            page_owned_work,
            blocking_stylesheet_inputs,
        })
    }

    pub(in crate::runtime) async fn finalize(
        self,
        page_vm: &mut PageVm,
    ) -> Vec<PostParsePageOwnedWork> {
        if page_vm.vm().current_main_document_task_owner() != Some(self.owner) {
            return Vec::new();
        }
        let marker = page_vm.seal_main_parser_deferred_scripts(self.owner);
        if page_vm.vm().current_main_document_task_owner() != Some(self.owner) {
            return Vec::new();
        }
        let handoff = self
            .scheduler
            .finalize_parser_prepared_post_parse_handoff(self.blocking_stylesheet_inputs)
            .await;
        let mut work = self.page_owned_work;
        work.extend(handoff.into_page_owned_work());
        if let Some(marker) = marker {
            work.push(marker);
        }
        work
    }
}

fn bind_initial_parser_script(
    page_vm: &mut PageVm,
    node_id: crate::dom::native::NativeNodeId,
    start_line: u64,
    start_column: u64,
    script: &mut crate::planning::PreparedScript,
) {
    page_vm
        .vm_mut()
        .document_runtime
        .note_parser_script_start_position(node_id, start_line, start_column);
    page_vm
        .vm_mut()
        .bind_prepared_script_if_needed(script, ScriptHandleSource::ParserOwned);
    let _ = page_vm
        .vm_mut()
        .document_runtime
        .dom_host_mut()
        .set_script_already_started(node_id, true);
}
