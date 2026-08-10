/// Maximum encoded image payload retained for protocol or resource handoff.
pub const MAX_ENCODED_IMAGE_BYTES: usize = 128 * 1024 * 1024;

/// Maximum materialized straight-RGBA8 buffer owned by one codec operation.
///
/// Paint applies a tighter screenshot pixel budget before reaching this
/// boundary. This independent limit also protects future network image decode.
pub const MAX_DECODED_RGBA_BYTES: usize = 128 * 1024 * 1024;
