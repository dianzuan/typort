#![warn(clippy::pedantic)]

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A journal preset loaded from a TOML file.
#[derive(Debug, Clone, Deserialize)]
pub struct Preset {
    pub journal: JournalInfo,
    pub page: Option<PagePreset>,
    pub font: Option<FontPreset>,
    pub footnote: Option<FootnotePreset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JournalInfo {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PagePreset {
    pub margin_top_cm: Option<f64>,
    pub margin_bottom_cm: Option<f64>,
    pub margin_left_cm: Option<f64>,
    pub margin_right_cm: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FontPreset {
    pub body_size_pt: Option<f64>,
    pub heading1_size_pt: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FootnotePreset {
    /// "decimal" (1,2,3) or "circled" (①②③)
    pub format: Option<String>,
}

/// Convert centimeters to OOXML twips (1 cm = 567 twips).
#[must_use]
pub fn cm_to_twips(cm: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let twips = (cm * 567.0).round() as u32;
    twips
}

/// Load a preset by name from the given presets directory.
///
/// Looks for `<presets_dir>/<name>.toml`.
///
/// # Errors
/// Returns an error if the file cannot be read or parsed.
pub fn load_preset(presets_dir: &Path, name: &str) -> Result<Preset, String> {
    let path = presets_dir.join(format!("{name}.toml"));
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read preset '{}': {e}", path.display()))?;
    let preset: Preset =
        toml::from_str(&content).map_err(|e| format!("invalid preset TOML: {e}"))?;
    Ok(preset)
}

/// Load a preset from the built-in presets directory.
///
/// Searches, in order: a `presets/` (or sibling `../presets/`) directory next
/// to the running executable — so an installed binary finds its presets
/// regardless of the working directory — then `presets/`, `../presets/`,
/// `../../presets/` relative to the current directory, which keeps `cargo run`
/// from the repo working during development.
///
/// # Errors
/// Returns an error if no preset directory is found or the preset cannot be loaded.
pub fn load_builtin_preset(name: &str) -> Result<Preset, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // Next to the installed executable (CWD-independent).
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        candidates.push(exe_dir.join("presets"));
        candidates.push(exe_dir.join("../presets"));
    }

    // Relative to the current directory (development from the repo).
    candidates.push(PathBuf::from("presets"));
    candidates.push(PathBuf::from("../presets"));
    candidates.push(PathBuf::from("../../presets"));

    for dir in &candidates {
        if dir.join(format!("{name}.toml")).exists() {
            return load_preset(dir, name);
        }
    }
    Err(format!(
        "preset '{name}' not found in any known presets directory"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_preset_from_toml() {
        let toml_content = r#"
[journal]
name = "管理世界"

[page]
margin_top_cm = 2.54
margin_bottom_cm = 2.54
margin_left_cm = 3.17
margin_right_cm = 3.17

[font]
body_size_pt = 10.5
heading1_size_pt = 15.0
"#;
        let preset: Preset = toml::from_str(toml_content).unwrap();
        assert_eq!(preset.journal.name, "管理世界");
        assert_eq!(preset.page.as_ref().unwrap().margin_top_cm, Some(2.54));
        assert_eq!(preset.font.as_ref().unwrap().body_size_pt, Some(10.5));
    }

    #[test]
    fn cm_to_twips_converts_correctly() {
        assert_eq!(cm_to_twips(2.54), 1440);
        assert_eq!(cm_to_twips(3.17), 1797);
    }

    #[test]
    fn load_preset_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let preset_path = dir.path().join("test_journal.toml");
        let mut f = std::fs::File::create(&preset_path).unwrap();
        writeln!(
            f,
            r#"
[journal]
name = "Test Journal"

[page]
margin_top_cm = 2.0
margin_bottom_cm = 2.0
margin_left_cm = 2.5
margin_right_cm = 2.5
"#
        )
        .unwrap();

        let preset = load_preset(dir.path(), "test_journal").unwrap();
        assert_eq!(preset.journal.name, "Test Journal");
        assert_eq!(preset.page.as_ref().unwrap().margin_top_cm, Some(2.0));
    }

    #[test]
    fn load_preset_missing_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_preset(dir.path(), "nonexistent");
        assert!(result.is_err());
    }
}
