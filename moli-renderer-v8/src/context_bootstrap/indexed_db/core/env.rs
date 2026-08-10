use super::*;

mod dom_string_list;
mod errors;
mod origin;
mod storage;

pub(in crate::context_bootstrap::indexed_db) use self::dom_string_list::{
    idb_dom_string_list_backing_values, install_dom_string_list_template_bindings,
    new_idb_dom_string_list,
};
pub(in crate::context_bootstrap::indexed_db) use self::errors::{
    dom_exception_value, request_error_object,
};
pub(in crate::context_bootstrap::indexed_db) use self::origin::{
    current_storage_scope, origin_allows_indexed_db, storage_scope_for_current_partition,
    storage_scope_for_window_execution_context,
};
pub(in crate::context_bootstrap) use self::storage::indexed_db_usage_bytes_for_storage_key;
pub(crate) use self::storage::set_indexed_db_manager_for_context;
#[cfg(test)]
pub(crate) use self::storage::{
    indexed_db_manager_context_slot_present_for_test,
    indexed_db_manager_isolate_slot_present_for_test,
};
pub(in crate::context_bootstrap::indexed_db) use self::storage::{
    storage_bucket_quota_check_for_object_store, storage_bucket_quota_check_for_transaction,
    validate_storage_bucket_indexed_db_context, with_indexed_db_manager,
};
