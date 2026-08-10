use super::super::{
    BridgeHandle, ComputedStyleDescriptor, DomTokenListKind, JsContextHost, ReflectorId,
};
use super::NativeDomBridge;
use crate::document_runtime::DomHandle;

impl NativeDomBridge {
    pub(crate) fn install_default_world_wrapper_cache(&self, context: v8::Local<'_, v8::Context>) {
        self.identity.install_default_world_wrapper_cache(context);
    }

    pub(crate) fn wrap_handle<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        host_ptr: *mut JsContextHost,
        handle: DomHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let generation = unsafe { &*host_ptr }.runtime_reset_generation();
        self.wrap_bridge_handle(scope, host_ptr, BridgeHandle::Node(handle, generation))
    }

    pub(crate) fn cached_handle_wrapper<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        host_ptr: *mut JsContextHost,
        handle: DomHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let generation = unsafe { &*host_ptr }.runtime_reset_generation();
        let reflector_id = self
            .identity
            .existing_reflector_id(BridgeHandle::Node(handle, generation))?;
        self.identity.cached_wrapper(scope, reflector_id)
    }

    pub(crate) fn retire_default_world_wrappers_for_realm(
        &self,
        realm_token: crate::native_bridge::RuntimeObservableContextToken,
    ) {
        self.identity
            .retire_default_world_wrappers_for_realm(realm_token);
    }

    pub(crate) fn rebind_wrapper_generation(
        &mut self,
        old_generation: u64,
        new_generation: u64,
    ) -> Option<usize> {
        self.identity
            .rebind_generation(old_generation, new_generation)
    }

    pub(crate) fn wrap_window<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        host_ptr: *mut JsContextHost,
    ) -> Option<v8::Local<'s, v8::Object>> {
        self.wrap_bridge_handle(scope, host_ptr, BridgeHandle::Window)
    }

    pub(super) fn wrap_bridge_handle<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        host_ptr: *mut JsContextHost,
        handle: BridgeHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let reflector_id = self.identity.reflector_id(handle.clone());
        if let Some(wrapper) = self.identity.cached_wrapper(scope, reflector_id) {
            if !matches!(&handle, BridgeHandle::Window) {
                self.bindings
                    .sync_wrapper_owner_realm_prototype(scope, host_ptr, &handle, wrapper);
            }
            return Some(wrapper);
        }

        let wrapper = self
            .bindings
            .instantiate_wrapper(scope, host_ptr, handle, reflector_id);
        self.identity.cache_wrapper(scope, reflector_id, wrapper);
        Some(wrapper)
    }

    pub(crate) fn wrap_handle_for_receiver<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        host_ptr: *mut JsContextHost,
        receiver: v8::Local<'s, v8::Object>,
        handle: DomHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let creation_context = receiver.get_creation_context(scope)?;
        let generation = unsafe { &*host_ptr }.runtime_reset_generation();
        let bridge_handle = BridgeHandle::Node(handle, generation);
        if creation_context == scope.get_current_context() {
            return self.wrap_bridge_handle(scope, host_ptr, bridge_handle);
        }

        let wrapper = {
            let target_scope = &mut v8::ContextScope::new(scope, creation_context);
            let wrapper = self.wrap_bridge_handle(target_scope, host_ptr, bridge_handle)?;
            v8::Global::new(target_scope, wrapper)
        };
        Some(v8::Local::new(scope, &wrapper))
    }

    pub(crate) fn wrap_class_list<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        runtime_ptr: *mut JsContextHost,
        handle: DomHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let generation = unsafe { &*runtime_ptr }.runtime_reset_generation();
        self.wrap_bridge_handle(
            scope,
            runtime_ptr,
            BridgeHandle::ClassList(handle, generation, DomTokenListKind::Class),
        )
    }

    pub(crate) fn wrap_part_list<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        runtime_ptr: *mut JsContextHost,
        handle: DomHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let generation = unsafe { &*runtime_ptr }.runtime_reset_generation();
        self.wrap_bridge_handle(
            scope,
            runtime_ptr,
            BridgeHandle::ClassList(handle, generation, DomTokenListKind::Part),
        )
    }

    pub(crate) fn wrap_rel_list<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        runtime_ptr: *mut JsContextHost,
        handle: DomHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let generation = unsafe { &*runtime_ptr }.runtime_reset_generation();
        self.wrap_bridge_handle(
            scope,
            runtime_ptr,
            BridgeHandle::ClassList(handle, generation, DomTokenListKind::Rel),
        )
    }

    pub(crate) fn wrap_dataset<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        runtime_ptr: *mut JsContextHost,
        handle: DomHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let generation = unsafe { &*runtime_ptr }.runtime_reset_generation();
        self.wrap_bridge_handle(
            scope,
            runtime_ptr,
            BridgeHandle::Dataset(handle, generation),
        )
    }

    pub(crate) fn wrap_style<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        runtime_ptr: *mut JsContextHost,
        handle: DomHandle,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let generation = unsafe { &*runtime_ptr }.runtime_reset_generation();
        self.wrap_bridge_handle(scope, runtime_ptr, BridgeHandle::Style(handle, generation))
    }

    pub(crate) fn wrap_computed_style<'s, 'i>(
        &mut self,
        scope: &mut v8::PinScope<'s, 'i>,
        runtime_ptr: *mut JsContextHost,
        handle: DomHandle,
        descriptor: ComputedStyleDescriptor,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let generation = unsafe { &*runtime_ptr }.runtime_reset_generation();
        self.wrap_bridge_handle(
            scope,
            runtime_ptr,
            BridgeHandle::ComputedStyle(handle, generation, descriptor),
        )
    }

    pub(crate) fn resolve_node_handle(&self, reflector_id: ReflectorId) -> Option<DomHandle> {
        match self.bridge_handle(reflector_id) {
            Some(BridgeHandle::Node(handle, _)) => Some(handle),
            Some(
                BridgeHandle::Window
                | BridgeHandle::ClassList(_, _, _)
                | BridgeHandle::Dataset(_, _)
                | BridgeHandle::Style(_, _)
                | BridgeHandle::ComputedStyle(_, _, _),
            )
            | None => None,
        }
    }
}
