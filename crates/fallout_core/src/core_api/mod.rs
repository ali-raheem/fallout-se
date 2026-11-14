#[allow(dead_code)]
pub(crate) mod byte_document;
// Immutable F2 DAT extraction only; no resource precedence or production use.
#[allow(dead_code)]
pub(crate) mod dat_bytes;
mod engine;
mod error;
// Narrow CLI-only facade. Private wire/patch/authorization types never escape.
#[cfg(feature = "experimental-rpu-cli")]
pub mod experimental_rpu;
// Conditional cursor discovery, never a quantity-patch authorization source.
#[allow(dead_code)]
pub(crate) mod inventory_cursor;
// Quantity-only byte patches remain independent of Session setters.
#[allow(dead_code)]
pub(crate) mod inventory_quantity_patch;
// Read-only verification remains separate from Session parsing and editing.
#[allow(dead_code)]
pub(crate) mod inventory_verifier;
mod item_catalog;
// Internal metadata contracts; no production parser or setter integration.
#[allow(dead_code)]
pub(crate) mod item_prototypes;
// Pure supplied-resource interpretation; all source provenance stays asserted.
#[allow(dead_code)]
pub(crate) mod resource_bytes;
mod roundtrip;
// Asserted RPU resource-plan diagnostics only; never a patch authorization path.
#[allow(dead_code)]
pub(crate) mod rpu_diagnostic;
// Explicitly consented in-memory copies; no Session, export or runtime authority.
#[allow(dead_code)]
pub(crate) mod rpu_experiment;
// Opt-in new-file export only. Never production editing authority.
#[allow(dead_code)]
pub(crate) mod rpu_export;
// Conditional prefix diagnostics only; no production parser/editing integration.
#[allow(dead_code)]
pub(crate) mod save_prefix;
mod snapshot_identity;
mod trait_catalog;
mod types;
pub mod well_known_items;

pub use engine::{Engine, Session};
pub use error::{CoreError, CoreErrorCode};
pub use item_catalog::{ItemCatalog, detect_install_dir_from_save_path};
pub use roundtrip::{
    DiagnosticValue, RoundtripDiagnostic, RoundtripDiagnosticKind, RoundtripReport,
};
pub use trait_catalog::TraitCatalog;
pub use types::{
    Capabilities, CapabilityIssue, CharacterExport, DateParts, Game, InventoryEntry,
    ItemCatalogEntry, KillCountEntry, PerkEntry, ResolvedInventoryEntry, SkillEntry, Snapshot,
    StatEntry, TraitEntry,
};

// Uploaded immutable resources; no filesystem or legacy Session editing.
#[cfg(feature = "experimental-rpu-browser")]
pub mod browser_rpu;
