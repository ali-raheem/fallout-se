//! Opt-in editor fidelity diagnostics. These neither enforce vanilla limits nor
//! change parsing, editing, or emission. Raw values avoid CharacterExport's
//! derived arithmetic. Opaque bytes remain in the document's existing blobs.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::layout::{ByteRange, FileLayout, SectionId};
use crate::object::GameObject;
use crate::{fallout1, fallout2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundtripDiagnosticKind {
    ParseFailure,
    SemanticMismatch,
    LayoutMismatch,
    UnsupportedInterpretation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticValue {
    Integer(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundtripDiagnostic {
    pub kind: RoundtripDiagnosticKind,
    pub field_path: String,
    pub expected: Option<DiagnosticValue>,
    pub actual: Option<DiagnosticValue>,
    /// Enclosing section ranges in the emitted file; not exact field offsets.
    pub expected_range: Option<ByteRange>,
    pub actual_range: Option<ByteRange>,
    pub message: String,
}

/// Findings about this editor's interpretation, not a game-validity certificate.
/// Derived calculations and opaque data semantics are outside this report's scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoundtripReport {
    pub diagnostics: Vec<RoundtripDiagnostic>,
}

#[derive(Clone)]
struct Observed {
    value: DiagnosticValue,
    range: Option<ByteRange>,
}

struct RawState {
    fields: BTreeMap<String, Observed>,
    layout: FileLayout,
}

impl RawState {
    fn new(layout: &FileLayout) -> Self {
        Self {
            fields: BTreeMap::new(),
            layout: layout.clone(),
        }
    }

    fn record(&mut self, path: &str, value: DiagnosticValue, section: SectionId) {
        let range = self
            .layout
            .sections
            .iter()
            .find(|s| s.id == section)
            .map(|s| s.range);
        self.fields
            .insert(path.to_owned(), Observed { value, range });
    }

    fn integer(&mut self, path: &str, value: i32, section: SectionId) {
        self.record(path, DiagnosticValue::Integer(i64::from(value)), section);
    }

    fn integers(&mut self, path: &str, values: &[i32], section: SectionId) {
        for (index, &value) in values.iter().enumerate() {
            self.integer(&format!("{path}[{index}]"), value, section);
        }
    }

    fn object(&mut self, root: &GameObject) {
        // Walk inventory iteratively rather than recursively in diagnostic code.
        let mut pending = vec![("player_object".to_owned(), root)];
        while let Some((path, object)) = pending.pop() {
            macro_rules! scalar {
                ($($field:ident),+ $(,)?) => {$(
                    self.integer(&format!("{path}.{}", stringify!($field)), object.$field, SectionId::Handler(5));
                )+};
            }
            scalar!(
                id,
                tile,
                x,
                y,
                sx,
                sy,
                frame,
                rotation,
                fid,
                flags,
                elevation,
                pid,
                cid,
                light_distance,
                light_intensity,
                outline,
                sid,
                script_index
            );
            // Storage length/capacity can be normalized by emission; compare the
            // actual vector length instead of stale allocation bookkeeping.
            self.record(
                &format!("{path}.inventory.len"),
                DiagnosticValue::Text(object.inventory.len().to_string()),
                SectionId::Handler(5),
            );
            self.record(
                &format!("{path}.object_data"),
                DiagnosticValue::Text(format!("{:?}", object.object_data)),
                SectionId::Handler(5),
            );
            for (index, item) in object.inventory.iter().enumerate() {
                let item_path = format!("{path}.inventory[{index}]");
                self.integer(
                    &format!("{item_path}.quantity"),
                    item.quantity,
                    SectionId::Handler(5),
                );
                pending.push((format!("{item_path}.object"), &item.object));
            }
        }
    }
}

// Both save types expose these raw fields. Keeping the comparison independent
// of export/query methods avoids skill, stat, and next-level-XP arithmetic.
macro_rules! record_fields {
    ($state:ident, $source:expr, $prefix:literal, $section:expr; $($field:ident),+ $(,)?) => {$(
        $state.integer(concat!($prefix, stringify!($field)), $source.$field, $section);
    )+};
}

macro_rules! raw_state {
    ($doc:expr, $traits:expr) => {{
        let save = &$doc.save;
        let mut state = RawState::new($doc.layout());
        state.record("header", DiagnosticValue::Text(format!("{:?}", save.header)), SectionId::Header);
        state.integer("player_combat_id", save.player_combat_id, SectionId::Handler(1));
        state.integer("center_tile", save.center_tile, SectionId::Handler(5));
        state.object(&save.player_object);
        record_fields!(state, save.critter_data, "critter_data.", SectionId::Handler(6);
            sneak_working, flags, body_type, experience, kill_type);
        state.integers("critter_data.base_stats", &save.critter_data.base_stats, SectionId::Handler(6));
        state.integers("critter_data.bonus_stats", &save.critter_data.bonus_stats, SectionId::Handler(6));
        state.integers("critter_data.skills", &save.critter_data.skills, SectionId::Handler(6));
        state.integers("kill_counts", &save.kill_counts, SectionId::Handler(7));
        state.integers("tagged_skills", &save.tagged_skills, SectionId::Handler(8));
        state.integers("perks", &save.perks, SectionId::Handler(10));
        state.record("combat_state", DiagnosticValue::Text(format!("{:?}", save.combat_state)), SectionId::Handler(11));
        record_fields!(state, save.pc_stats, "pc_stats.", SectionId::Handler(13);
            unspent_skill_points, level, experience, reputation, karma);
        state.integers("selected_traits", &save.selected_traits, SectionId::Handler($traits));
        state
    }};
}

pub(super) fn inspect_fallout1(doc: &fallout1::Document) -> RoundtripReport {
    inspect(raw_state!(doc, 16), doc.to_bytes_modified(), |bytes| {
        let reopened = fallout1::Document::parse_with_layout(Cursor::new(bytes))?;
        Ok(raw_state!(reopened, 16))
    })
}

pub(super) fn inspect_fallout2(doc: &fallout2::Document) -> RoundtripReport {
    fn state(doc: &fallout2::Document) -> RawState {
        let mut state = raw_state!(doc, 15);
        state.integer(
            "critter_data.damage_type",
            doc.save.critter_data.damage_type,
            SectionId::Handler(6),
        );
        state.integer(
            "game_difficulty",
            doc.save.game_difficulty,
            SectionId::Handler(17),
        );
        state
    }
    inspect(state(doc), doc.to_bytes_modified(), |bytes| {
        let reopened = fallout2::Document::parse_with_layout(Cursor::new(bytes))?;
        Ok(state(&reopened))
    })
}

fn finding(kind: RoundtripDiagnosticKind, path: &str, message: String) -> RoundtripDiagnostic {
    RoundtripDiagnostic {
        kind,
        field_path: path.to_owned(),
        expected: None,
        actual: None,
        expected_range: None,
        actual_range: None,
        message,
    }
}

fn inspect(
    expected: RawState,
    bytes: io::Result<Vec<u8>>,
    parse: impl FnOnce(&[u8]) -> io::Result<RawState>,
) -> RoundtripReport {
    use RoundtripDiagnosticKind::*;
    let mut report = RoundtripReport::default();
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(error) => {
            report.diagnostics.push(finding(
                UnsupportedInterpretation,
                "serialization",
                error.to_string(),
            ));
            return report;
        }
    };
    let actual = match catch_unwind(AssertUnwindSafe(|| parse(&bytes))) {
        Ok(Ok(actual)) => actual,
        Ok(Err(error)) => {
            report
                .diagnostics
                .push(finding(ParseFailure, "document", error.to_string()));
            return report;
        }
        Err(_) => {
            report.diagnostics.push(finding(
                UnsupportedInterpretation,
                "document",
                "parser panicked while inspecting emitted bytes".to_owned(),
            ));
            return report;
        }
    };
    compare(&expected, &actual, &mut report);
    report
}

fn compare(expected: &RawState, actual: &RawState, report: &mut RoundtripReport) {
    use RoundtripDiagnosticKind::*;
    if expected.layout.sections != actual.layout.sections
        || expected.layout.file_len != actual.layout.file_len
    {
        let mut diagnostic = finding(
            LayoutMismatch,
            "layout",
            "reopened section boundaries or identities differ".to_owned(),
        );
        diagnostic.expected = Some(DiagnosticValue::Text(format!("{:?}", expected.layout)));
        diagnostic.actual = Some(DiagnosticValue::Text(format!("{:?}", actual.layout)));
        report.diagnostics.push(diagnostic);
    }
    let paths: BTreeSet<_> = expected.fields.keys().chain(actual.fields.keys()).collect();
    for path in paths {
        let before = expected.fields.get(path);
        let after = actual.fields.get(path);
        if before.map(|v| &v.value) != after.map(|v| &v.value) {
            let mut diagnostic = finding(
                SemanticMismatch,
                path,
                "raw interpreted values differ".to_owned(),
            );
            diagnostic.expected = before.map(|v| v.value.clone());
            diagnostic.actual = after.map(|v| v.value.clone());
            diagnostic.expected_range = before.and_then(|v| v.range);
            diagnostic.actual_range = after.and_then(|v| v.range);
            report.diagnostics.push(diagnostic);
        }
    }
    // Fallout 1 can return a successful document after failing to interpret traits.
    // Do not mistake its sentinel model for evidence of an empty trait selection.
    if actual
        .fields
        .get("selected_traits[0]")
        .is_some_and(|v| v.range.is_none())
    {
        report.diagnostics.push(finding(
            UnsupportedInterpretation,
            "selected_traits",
            "traits section was not interpreted; bytes remain opaque".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> fallout1::Document {
        fallout1::Document::parse_with_layout(Cursor::new(include_bytes!(
            "../../../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT"
        )))
        .unwrap()
    }

    #[test]
    fn layout_drift_is_distinct_from_semantic_drift() {
        let doc = document();
        let expected = raw_state!(doc, 16);
        let mut actual = raw_state!(doc, 16);
        actual.layout.sections[0].range.end += 1;
        let mut report = RoundtripReport::default();
        compare(&expected, &actual, &mut report);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].kind,
            RoundtripDiagnosticKind::LayoutMismatch
        );
        assert!(report.diagnostics[0].expected.is_some());
        assert!(report.diagnostics[0].actual.is_some());
    }

    #[test]
    fn malformed_emission_reports_parse_failure() {
        let doc = document();
        let report = inspect(raw_state!(doc, 16), Ok(vec![0; 8]), |bytes| {
            let reopened = fallout1::Document::parse_with_layout(Cursor::new(bytes))?;
            Ok(raw_state!(reopened, 16))
        });
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].kind,
            RoundtripDiagnosticKind::ParseFailure
        );
    }

    #[test]
    fn parser_panic_is_reported_as_unsupported_interpretation() {
        let doc = document();
        let report = inspect(raw_state!(doc, 16), Ok(vec![]), |_| {
            panic!("injected parser panic")
        });
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].kind,
            RoundtripDiagnosticKind::UnsupportedInterpretation
        );
        assert!(report.diagnostics[0].message.contains("parser panicked"));
    }
}
