use fallout_core::core_api::{CharacterExport, Engine, Game as CoreGame, Session};
use fallout_render::{
    JsonStyle, TextRenderOptions, render_classic_sheet_with_inventory,
    render_json_full_with_inventory,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct WebRenderOptions {
    pub game_hint: Option<String>,
    pub json_output: bool,
    pub metadata: Option<MetadataOptions>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MetadataOptions {
    pub mode: String,
    pub payload: Option<serde_json::Value>,
}

impl Default for MetadataOptions {
    fn default() -> Self {
        Self {
            mode: String::new(),
            payload: None,
        }
    }
}

#[derive(Debug, Clone)]
struct WebError {
    code: &'static str,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct WebErrorPayload {
    code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct ApplySavePayload {
    updated_bytes: Vec<u8>,
    filename_hint: String,
}

impl WebError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn to_js_value(&self) -> JsValue {
        let payload = WebErrorPayload {
            code: self.code.to_string(),
            message: self.message.clone(),
        };
        serde_wasm_bindgen::to_value(&payload).unwrap_or_else(|_| {
            JsValue::from_str(&format!("{}: {}", payload.code, payload.message))
        })
    }
}

#[wasm_bindgen]
pub fn render_save_text(save_bytes: &[u8], options: JsValue) -> Result<String, JsValue> {
    let parsed_options = parse_options(options).map_err(|err| err.to_js_value())?;
    render_save_text_impl(save_bytes, &parsed_options).map_err(|err| err.to_js_value())
}

#[wasm_bindgen]
pub fn export_save_json(save_bytes: &[u8], options: JsValue) -> Result<String, JsValue> {
    let parsed_options = parse_options(options).map_err(|err| err.to_js_value())?;
    export_save_json_impl(save_bytes, &parsed_options).map_err(|err| err.to_js_value())
}

#[wasm_bindgen]
pub fn apply_json_to_save(
    save_bytes: &[u8],
    edited_json: String,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let parsed_options = parse_options(options).map_err(|err| err.to_js_value())?;
    let payload = apply_json_to_save_impl(save_bytes, &edited_json, &parsed_options)
        .map_err(|err| err.to_js_value())?;
    serde_wasm_bindgen::to_value(&payload).map_err(|err| {
        WebError::new(
            "render_failed",
            format!("Failed to serialize apply result for web output: {err}"),
        )
        .to_js_value()
    })
}

fn render_save_text_impl(
    save_bytes: &[u8],
    options: &WebRenderOptions,
) -> Result<String, WebError> {
    let session = open_session(save_bytes, options)?;
    let resolved_inventory = session.inventory_resolved_builtin();

    if options.json_output {
        let value = render_json_full_with_inventory(
            &session,
            JsonStyle::CanonicalV1,
            Some(&resolved_inventory),
        );
        return serde_json::to_string_pretty(&value).map_err(|err| {
            WebError::new(
                "render_failed",
                format!("failed to serialize rendered JSON output: {err}"),
            )
        });
    }

    Ok(render_classic_sheet_with_inventory(
        &session,
        TextRenderOptions { verbose: false },
        Some(&resolved_inventory),
        None,
    ))
}

fn export_save_json_impl(
    save_bytes: &[u8],
    options: &WebRenderOptions,
) -> Result<String, WebError> {
    let session = open_session(save_bytes, options)?;
    let exported = session.export_character();
    serde_json::to_string_pretty(&exported).map_err(|err| {
        WebError::new(
            "render_failed",
            format!("failed to serialize CharacterExport JSON output: {err}"),
        )
    })
}

fn apply_json_to_save_impl(
    save_bytes: &[u8],
    edited_json: &str,
    options: &WebRenderOptions,
) -> Result<ApplySavePayload, WebError> {
    let mut session = open_session(save_bytes, options)?;
    let trimmed = edited_json.trim();
    if trimmed.is_empty() {
        return Err(WebError::new(
            "invalid_json",
            "Editor JSON is empty. Provide a full CharacterExport payload.",
        ));
    }

    let edited: CharacterExport = serde_json::from_str(trimmed).map_err(|err| {
        WebError::new(
            "invalid_json",
            format!("Failed to parse edited CharacterExport JSON: {err}"),
        )
    })?;

    session
        .apply_character(&edited)
        .map_err(|err| WebError::new("apply_failed", err.to_string()))?;

    let updated_bytes = session
        .to_bytes_modified()
        .map_err(|err| WebError::new("emit_failed", err.to_string()))?;

    Ok(ApplySavePayload {
        updated_bytes,
        filename_hint: format!(
            "{}_edited.SAVE.DAT",
            sanitize_filename_component(&session.snapshot().character_name)
        ),
    })
}

fn open_session(save_bytes: &[u8], options: &WebRenderOptions) -> Result<Session, WebError> {
    if save_bytes.is_empty() {
        return Err(WebError::new(
            "unsupported_file",
            "The uploaded file is empty. Please provide a SAVE.DAT file.",
        ));
    }

    let game_hint = parse_game_hint(options.game_hint.as_deref())?;

    // Reserved for future metadata loaders. v1 ignores metadata payload.
    let _reserved_metadata = options.metadata.as_ref();

    Engine::new()
        .open_bytes(save_bytes, game_hint)
        .map_err(|err| WebError::new("parse_failed", err.to_string()))
}

fn sanitize_filename_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('_');
        }
    }

    let collapsed = out
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if collapsed.is_empty() {
        "SAVE".to_string()
    } else {
        collapsed
    }
}

fn parse_options(options: JsValue) -> Result<WebRenderOptions, WebError> {
    if options.is_null() || options.is_undefined() {
        return Ok(WebRenderOptions::default());
    }

    serde_wasm_bindgen::from_value(options).map_err(|err| {
        WebError::new(
            "invalid_options",
            format!("Failed to parse web render options: {err}"),
        )
    })
}

fn parse_game_hint(raw_hint: Option<&str>) -> Result<Option<CoreGame>, WebError> {
    let Some(raw_hint) = raw_hint else {
        return Ok(None);
    };

    let normalized = raw_hint.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(None);
    }

    match normalized.as_str() {
        "1" | "fo1" | "fallout1" => Ok(Some(CoreGame::Fallout1)),
        "2" | "fo2" | "fallout2" => Ok(Some(CoreGame::Fallout2)),
        _ => Err(WebError::new(
            "invalid_options",
            format!(
                "Invalid game_hint '{raw_hint}'. Expected one of: 1, 2, fo1, fo2, fallout1, fallout2"
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use fallout_core::core_api::Engine;
    use serde_json::Value;

    use super::{
        CharacterExport, CoreGame, WebRenderOptions, apply_json_to_save_impl,
        export_save_json_impl, parse_game_hint, render_save_text_impl,
    };

    #[test]
    fn parse_game_hint_accepts_aliases() {
        assert_eq!(
            parse_game_hint(Some("fo1")).expect("fo1 should parse"),
            Some(CoreGame::Fallout1)
        );
        assert_eq!(
            parse_game_hint(Some("2")).expect("2 should parse"),
            Some(CoreGame::Fallout2)
        );
        assert_eq!(
            parse_game_hint(Some("   ")).expect("blank is allowed"),
            None
        );
        assert_eq!(parse_game_hint(None).expect("none is allowed"), None);
    }

    #[test]
    fn parse_game_hint_rejects_invalid_values() {
        let err = parse_game_hint(Some("fallout3")).expect_err("invalid hint should fail");
        assert_eq!(err.code, "invalid_options");
        assert!(err.message.contains("Invalid game_hint"));
    }

    #[test]
    fn render_save_text_impl_renders_fallout1_fixture() {
        let bytes = fixture_bytes("tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT");
        let rendered = render_save_text_impl(&bytes, &WebRenderOptions::default())
            .expect("fallout1 fixture should render");
        assert!(rendered.contains("FALLOUT"));
        assert!(rendered.contains("VAULT-13 PERSONNEL RECORD"));
        assert!(rendered.contains("::: Inventory :::"));
    }

    #[test]
    fn render_save_text_impl_renders_fallout2_fixture() {
        let bytes = fixture_bytes("tests/fallout2_examples/SLOT01/SAVE.DAT");
        let rendered = render_save_text_impl(&bytes, &WebRenderOptions::default())
            .expect("fallout2 fixture should render");
        assert!(rendered.contains("FALLOUT II"));
        assert!(rendered.contains("PERSONNEL RECORD"));
    }

    #[test]
    fn render_save_text_impl_rejects_empty_payload() {
        let err = render_save_text_impl(&[], &WebRenderOptions::default())
            .expect_err("empty payload should fail");
        assert_eq!(err.code, "unsupported_file");
    }

    #[test]
    fn render_save_text_impl_can_emit_json() {
        let bytes = fixture_bytes("tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT");
        let options = WebRenderOptions {
            json_output: true,
            ..WebRenderOptions::default()
        };
        let rendered = render_save_text_impl(&bytes, &options).expect("json output should render");

        let parsed: serde_json::Value =
            serde_json::from_str(&rendered).expect("json output should parse");
        assert_eq!(parsed["game"], "Fallout1");
        assert!(parsed.get("stats").is_some());
        assert!(parsed.get("skills").is_some());
    }

    #[test]
    fn export_save_json_impl_emits_character_export_json() {
        let bytes = fixture_bytes("tests/fallout2_examples/SLOT01/SAVE.DAT");
        let rendered = export_save_json_impl(&bytes, &WebRenderOptions::default())
            .expect("json should render");

        let parsed: CharacterExport =
            serde_json::from_str(&rendered).expect("json should parse as CharacterExport");
        assert_eq!(parsed.game, CoreGame::Fallout2);
        assert!(!parsed.skills.is_empty());
        assert!(!parsed.stats.is_empty());
    }

    #[test]
    fn apply_json_to_save_impl_applies_changes_and_emits_updated_bytes() {
        let bytes = fixture_bytes("tests/fallout2_examples/SLOT01/SAVE.DAT");
        let exported = export_save_json_impl(&bytes, &WebRenderOptions::default())
            .expect("json should render");
        let mut edited: CharacterExport =
            serde_json::from_str(&exported).expect("json should parse as CharacterExport");
        edited.level = 5;
        edited.xp = 4_321;

        let edited_json =
            serde_json::to_string_pretty(&edited).expect("edited export should serialize");
        let payload = apply_json_to_save_impl(&bytes, &edited_json, &WebRenderOptions::default())
            .expect("apply should succeed");
        assert!(payload.updated_bytes.len() > 0);
        assert!(payload.filename_hint.ends_with("_edited.SAVE.DAT"));

        let reparsed = Engine::new()
            .open_bytes(&payload.updated_bytes, None)
            .expect("updated bytes should parse");
        assert_eq!(reparsed.snapshot().level, 5);
        assert_eq!(reparsed.snapshot().experience, 4_321);
    }

    #[test]
    fn apply_json_to_save_impl_rejects_invalid_json() {
        let bytes = fixture_bytes("tests/fallout2_examples/SLOT01/SAVE.DAT");
        let err = apply_json_to_save_impl(&bytes, "{ not valid json", &WebRenderOptions::default())
            .expect_err("invalid json should fail");
        assert_eq!(err.code, "invalid_json");
    }

    #[test]
    fn apply_json_to_save_impl_rejects_unknown_top_level_field() {
        let bytes = fixture_bytes("tests/fallout2_examples/SLOT01/SAVE.DAT");
        let exported = export_save_json_impl(&bytes, &WebRenderOptions::default())
            .expect("json should render");
        let mut value: Value = serde_json::from_str(&exported).expect("json should parse");
        value["unknown_editor_field"] = Value::from(1);
        let edited_json = serde_json::to_string_pretty(&value).expect("value should serialize");

        let err = apply_json_to_save_impl(&bytes, &edited_json, &WebRenderOptions::default())
            .expect_err("unknown field should fail in strict mode");
        assert_eq!(err.code, "invalid_json");
    }

    fn fixture_bytes(relative_path: &str) -> Vec<u8> {
        let full_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative_path);
        fs::read(full_path).expect("fixture bytes should be readable")
    }
}
