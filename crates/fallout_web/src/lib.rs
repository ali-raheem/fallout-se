use fallout_core::core_api::{Engine, Game as CoreGame};
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

fn render_save_text_impl(
    save_bytes: &[u8],
    options: &WebRenderOptions,
) -> Result<String, WebError> {
    if save_bytes.is_empty() {
        return Err(WebError::new(
            "unsupported_file",
            "The uploaded file is empty. Please provide a SAVE.DAT file.",
        ));
    }

    let game_hint = parse_game_hint(options.game_hint.as_deref())?;

    // Reserved for future metadata loaders. v1 ignores metadata payload.
    let _reserved_metadata = options.metadata.as_ref();

    let engine = Engine::new();
    let session = engine
        .open_bytes(save_bytes, game_hint)
        .map_err(|err| WebError::new("parse_failed", err.to_string()))?;
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

    use super::{CoreGame, WebRenderOptions, parse_game_hint, render_save_text_impl};

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
        let rendered =
            render_save_text_impl(&bytes, &options).expect("json output should render");

        let parsed: serde_json::Value =
            serde_json::from_str(&rendered).expect("json output should parse");
        assert_eq!(parsed["game"], "Fallout1");
        assert!(parsed.get("stats").is_some());
        assert!(parsed.get("skills").is_some());
    }

    fn fixture_bytes(relative_path: &str) -> Vec<u8> {
        let full_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative_path);
        fs::read(full_path).expect("fixture bytes should be readable")
    }
}
