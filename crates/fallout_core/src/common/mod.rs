pub mod blob_emission;
pub mod blob_patching;
pub mod layout_management;

/// Derived character values retain their existing i32 API. Calculate in i128
/// (including products and cancellation), then saturate only the final result.
/// This is a presentation policy: it never clamps raw save fields or bytes.
pub(crate) fn derived_i32(value: i128) -> i32 {
    value.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
}
