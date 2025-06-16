//! Manifest parsing and validation for quick-add functionality.
//!
//! This module handles parsing of manifest files in different formats (.txt, .yaml, .json)
//! and provides validation of rule file paths within a repository tree.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use super::{RepoLocator, RepoTree};

/// Error types for manifest parsing and validation
#[derive(Error, Debug)]
pub enum ManifestError {
    #[error("Invalid file format: {0}")]
    InvalidFormat(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("File not found: {0}")]
    FileNotFound(String),
}

/// Supported manifest file formats
#[derive(Debug, Clone, PartialEq)]
pub enum ManifestFormat {
    Txt,
    Yaml,
    Json,
}

impl ManifestFormat {
    /// Get the priority of this format (lower is higher priority)
    pub fn priority(&self) -> u8 {
        match self {
            ManifestFormat::Txt => 1,
            ManifestFormat::Yaml => 2,
            ManifestFormat::Json => 3,
        }
    }

    /// Determine format from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "txt" => Some(ManifestFormat::Txt),
            "yaml" | "yml" => Some(ManifestFormat::Yaml),
            "json" => Some(ManifestFormat::Json),
            _ => None,
        }
    }
}

/// Schema for YAML/JSON manifests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSchema {
    pub name: String,
    pub description: Option<String>,
    pub rules: Vec<String>,
}

/// A parsed and validated manifest
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Friendly name of the manifest
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// List of valid rule file paths
    pub entries: Vec<String>,
    /// Validation errors encountered
    pub errors: Vec<String>,
    /// Validation warnings encountered
    pub warnings: Vec<String>,
}

/// Parse a .txt manifest (one rule path per line)
pub fn parse_txt_manifest(content: &str) -> Result<Vec<String>, ManifestError> {
    let entries = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect();

    Ok(entries)
}

/// Parse a YAML manifest with standardized schema
pub fn parse_yaml_manifest(content: &str) -> Result<ManifestSchema, ManifestError> {
    serde_yaml::from_str(content).map_err(|e| ManifestError::ParseError(e.to_string()))
}

/// Parse a JSON manifest with standardized schema
pub fn parse_json_manifest(content: &str) -> Result<ManifestSchema, ManifestError> {
    serde_json::from_str(content).map_err(|e| ManifestError::ParseError(e.to_string()))
}

/// Find manifest files in quick-add directory and resolve priority
pub async fn find_manifests_in_quickadd(
    repo_tree: &mut RepoTree,
    locator: &RepoLocator,
) -> anyhow::Result<HashMap<String, (ManifestFormat, String)>> {
    let mut manifests: HashMap<String, (ManifestFormat, String)> = HashMap::new();

    // Get children of quick-add directory
    let quickadd_children = repo_tree.children(locator, "quick-add").await?;

    for child in quickadd_children {
        if let Some(format) = get_manifest_format(&child.name) {
            let basename = get_basename(&child.name);

            // Apply priority resolution: .txt > .yaml > .json
            if let Some((existing_format, _)) = manifests.get(&basename) {
                if format.priority() < existing_format.priority() {
                    manifests.insert(basename, (format, child.path.clone()));
                }
            } else {
                manifests.insert(basename, (format, child.path.clone()));
            }
        }
    }

    Ok(manifests)
}

/// Validate manifest entries against repository tree
pub async fn validate_manifest_entries(
    entries: &[String],
    repo_tree: &mut RepoTree,
    locator: &RepoLocator,
) -> anyhow::Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut valid_entries = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        // Check if it's a .mdc file
        if !entry.ends_with(".mdc") {
            warnings.push(format!("Non-.mdc file ignored: {}", entry));
            continue;
        }

        // Check if file exists in repository tree
        if file_exists_in_repo(entry, repo_tree, locator).await? {
            valid_entries.push(entry.to_string());
        } else {
            errors.push(format!("File not found in repository: {}", entry));
        }
    }

    Ok((valid_entries, errors, warnings))
}

/// Check if a file exists in the repository tree
async fn file_exists_in_repo(
    file_path: &str,
    repo_tree: &mut RepoTree,
    locator: &RepoLocator,
) -> anyhow::Result<bool> {
    // Extract directory path from file path
    let dir_path = if let Some(pos) = file_path.rfind('/') {
        &file_path[..pos]
    } else {
        "" // root directory
    };

    // Get children of the directory
    let children = repo_tree.children(locator, dir_path).await?;

    // Check if the file exists in the directory
    let _file_name = file_path.split('/').last().unwrap_or("");
    for child in children {
        if child.path == file_path {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Parse manifest content based on format
pub async fn parse_manifest_content(
    content: &str,
    format: ManifestFormat,
    filename: &str,
    repo_tree: &mut RepoTree,
    locator: &RepoLocator,
) -> Result<Manifest, ManifestError> {
    let (entries, name, description) = match format {
        ManifestFormat::Txt => {
            let entries = parse_txt_manifest(content)?;
            let name = get_basename(filename);
            (entries, name, None)
        }
        ManifestFormat::Yaml => {
            let schema = parse_yaml_manifest(content)?;
            (schema.rules, schema.name, schema.description)
        }
        ManifestFormat::Json => {
            let schema = parse_json_manifest(content)?;
            (schema.rules, schema.name, schema.description)
        }
    };

    let (valid_entries, errors, warnings) = validate_manifest_entries(&entries, repo_tree, locator)
        .await
        .map_err(|e| ManifestError::ValidationError(e.to_string()))?;

    Ok(Manifest {
        name,
        description,
        entries: valid_entries,
        errors,
        warnings,
    })
}

/// Helper functions

fn get_manifest_format(filename: &str) -> Option<ManifestFormat> {
    if let Some(ext) = filename.split('.').last() {
        ManifestFormat::from_extension(ext)
    } else {
        None
    }
}

fn get_basename(filename: &str) -> String {
    if let Some(dot_pos) = filename.rfind('.') {
        filename[..dot_pos].to_string()
    } else {
        filename.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_txt_manifest_success() {
        let content = "frontend/react.mdc\n# Comment line\n\nbackend/rust.mdc";
        let result = parse_txt_manifest(content).unwrap();
        assert_eq!(result, vec!["frontend/react.mdc", "backend/rust.mdc"]);
    }

    #[test]
    fn test_parse_txt_manifest_ignores_blank_lines() {
        let content = "frontend/react.mdc\n\n\n\nbackend/rust.mdc";
        let result = parse_txt_manifest(content).unwrap();
        assert_eq!(result, vec!["frontend/react.mdc", "backend/rust.mdc"]);
    }

    #[test]
    fn test_parse_txt_manifest_ignores_comments() {
        let content = "frontend/react.mdc\n# This is a comment\n#Another comment\nbackend/rust.mdc";
        let result = parse_txt_manifest(content).unwrap();
        assert_eq!(result, vec!["frontend/react.mdc", "backend/rust.mdc"]);
    }

    #[test]
    fn test_parse_txt_manifest_empty_content() {
        let content = "\n\n# Only comments\n# More comments\n\n";
        let result = parse_txt_manifest(content).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_txt_manifest_with_whitespace() {
        let content = "  frontend/react.mdc  \n\t# Comment\t\n  backend/rust.mdc\t";
        let result = parse_txt_manifest(content).unwrap();
        assert_eq!(result, vec!["frontend/react.mdc", "backend/rust.mdc"]);
    }

    #[test]
    fn test_parse_yaml_manifest_success() {
        let content = r#"
name: "Frontend Rules"
description: "React and Vue rules"
rules:
  - "frontend/react.mdc"
  - "frontend/vue.mdc"
"#;
        let result = parse_yaml_manifest(content).unwrap();
        assert_eq!(result.name, "Frontend Rules");
        assert_eq!(result.description, Some("React and Vue rules".to_string()));
        assert_eq!(result.rules, vec!["frontend/react.mdc", "frontend/vue.mdc"]);
    }

    #[test]
    fn test_parse_yaml_manifest_minimal() {
        let content = r#"
name: "Minimal"
rules:
  - "test.mdc"
"#;
        let result = parse_yaml_manifest(content).unwrap();
        assert_eq!(result.name, "Minimal");
        assert_eq!(result.description, None);
        assert_eq!(result.rules, vec!["test.mdc"]);
    }

    #[test]
    fn test_parse_json_manifest_success() {
        let content = r#"
{
  "name": "Backend Rules",
  "description": "Rust and Python rules",
  "rules": [
    "backend/rust.mdc",
    "backend/python.mdc"
  ]
}
"#;
        let result = parse_json_manifest(content).unwrap();
        assert_eq!(result.name, "Backend Rules");
        assert_eq!(
            result.description,
            Some("Rust and Python rules".to_string())
        );
        assert_eq!(result.rules, vec!["backend/rust.mdc", "backend/python.mdc"]);
    }

    #[test]
    fn test_parse_json_manifest_minimal() {
        let content = r#"
{
  "name": "Test",
  "rules": ["test.mdc"]
}
"#;
        let result = parse_json_manifest(content).unwrap();
        assert_eq!(result.name, "Test");
        assert_eq!(result.description, None);
        assert_eq!(result.rules, vec!["test.mdc"]);
    }

    // Note: validate_manifest_entries tests require GitHub API access
    // These are covered by integration tests with real repositories

    // Note: This test requires GitHub API access, so we skip it in unit tests
    // It's covered by integration tests with real repositories instead

    // Note: parse_manifest_content tests require GitHub API access for validation
    // These are covered by integration tests with real repositories

    #[test]
    fn test_get_basename() {
        assert_eq!(get_basename("fullstack.txt"), "fullstack");
        assert_eq!(get_basename("config.yaml"), "config");
        assert_eq!(get_basename("noextension"), "noextension");
        assert_eq!(
            get_basename("complex.name.with.dots.json"),
            "complex.name.with.dots"
        );
        assert_eq!(get_basename(""), "");
    }

    #[test]
    fn test_get_manifest_format() {
        assert_eq!(get_manifest_format("test.txt"), Some(ManifestFormat::Txt));
        assert_eq!(get_manifest_format("test.yaml"), Some(ManifestFormat::Yaml));
        assert_eq!(get_manifest_format("test.yml"), Some(ManifestFormat::Yaml));
        assert_eq!(get_manifest_format("test.json"), Some(ManifestFormat::Json));
        assert_eq!(get_manifest_format("test.mdc"), None);
        assert_eq!(get_manifest_format("noextension"), None);
        assert_eq!(get_manifest_format(""), None);
    }

    #[test]
    fn test_manifest_format_from_extension() {
        assert_eq!(
            ManifestFormat::from_extension("txt"),
            Some(ManifestFormat::Txt)
        );
        assert_eq!(
            ManifestFormat::from_extension("TXT"),
            Some(ManifestFormat::Txt)
        );
        assert_eq!(
            ManifestFormat::from_extension("yaml"),
            Some(ManifestFormat::Yaml)
        );
        assert_eq!(
            ManifestFormat::from_extension("YAML"),
            Some(ManifestFormat::Yaml)
        );
        assert_eq!(
            ManifestFormat::from_extension("yml"),
            Some(ManifestFormat::Yaml)
        );
        assert_eq!(
            ManifestFormat::from_extension("YML"),
            Some(ManifestFormat::Yaml)
        );
        assert_eq!(
            ManifestFormat::from_extension("json"),
            Some(ManifestFormat::Json)
        );
        assert_eq!(
            ManifestFormat::from_extension("JSON"),
            Some(ManifestFormat::Json)
        );
        assert_eq!(ManifestFormat::from_extension("mdc"), None);
        assert_eq!(ManifestFormat::from_extension(""), None);
    }

    #[test]
    fn test_manifest_format_priority() {
        assert!(ManifestFormat::Txt.priority() < ManifestFormat::Yaml.priority());
        assert!(ManifestFormat::Yaml.priority() < ManifestFormat::Json.priority());
        assert_eq!(ManifestFormat::Txt.priority(), 1);
        assert_eq!(ManifestFormat::Yaml.priority(), 2);
        assert_eq!(ManifestFormat::Json.priority(), 3);
    }

    #[test]
    fn test_parse_yaml_manifest_invalid_schema() {
        let content = r#"
invalid_field: "test"
rules:
  - "frontend/react.mdc"
"#;
        let result = parse_yaml_manifest(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing field `name`"));
    }

    #[test]
    fn test_parse_yaml_manifest_invalid_syntax() {
        let content = r#"
name: "test
invalid yaml: [unclosed
"#;
        let result = parse_yaml_manifest(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_manifest_invalid_syntax() {
        let content = r#"
{
  "name": "test"
  "rules": [
    "frontend/react.mdc"
  ]
  // missing comma and invalid comment
}
"#;
        let result = parse_json_manifest(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_manifest_invalid_schema() {
        let content = r#"
{
  "invalid_field": "test",
  "rules": ["test.mdc"]
}
"#;
        let result = parse_json_manifest(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing field `name`"));
    }

    #[test]
    fn test_file_priority_resolution_yaml_over_json() {
        assert!(ManifestFormat::Yaml.priority() < ManifestFormat::Json.priority());

        // Ensure YAML has higher priority (lower number) than JSON
        let yaml_priority = ManifestFormat::Yaml.priority();
        let json_priority = ManifestFormat::Json.priority();
        assert!(yaml_priority < json_priority);
    }

    #[test]
    fn test_manifest_error_display() {
        let error1 = ManifestError::InvalidFormat("test".to_string());
        assert_eq!(error1.to_string(), "Invalid file format: test");

        let error2 = ManifestError::ParseError("parse failed".to_string());
        assert_eq!(error2.to_string(), "Parse error: parse failed");

        let error3 = ManifestError::ValidationError("validation failed".to_string());
        assert_eq!(error3.to_string(), "Validation error: validation failed");

        let error4 = ManifestError::FileNotFound("missing.mdc".to_string());
        assert_eq!(error4.to_string(), "File not found: missing.mdc");
    }

    #[test]
    fn test_manifest_error_variants() {
        // Test InvalidFormat error variant
        let invalid_format_err = ManifestError::InvalidFormat("unsupported format".to_string());
        assert!(invalid_format_err
            .to_string()
            .contains("Invalid file format"));
        assert!(invalid_format_err
            .to_string()
            .contains("unsupported format"));

        // Test FileNotFound error variant
        let file_not_found_err = ManifestError::FileNotFound("missing/file.mdc".to_string());
        assert!(file_not_found_err.to_string().contains("File not found"));
        assert!(file_not_found_err.to_string().contains("missing/file.mdc"));

        // Test error trait implementations
        let parse_err = ManifestError::ParseError("invalid syntax".to_string());
        assert!(format!("{:?}", parse_err).contains("ParseError"));

        let validation_err = ManifestError::ValidationError("validation issue".to_string());
        assert!(format!("{:?}", validation_err).contains("ValidationError"));
    }

    #[test]
    fn test_manifest_format_edge_cases() {
        // Test all format priority combinations
        let formats = vec![
            ManifestFormat::Txt,
            ManifestFormat::Yaml,
            ManifestFormat::Json,
        ];

        for (i, format1) in formats.iter().enumerate() {
            for (j, format2) in formats.iter().enumerate() {
                if i < j {
                    assert!(
                        format1.priority() < format2.priority(),
                        "Priority ordering failed for {:?} vs {:?}",
                        format1,
                        format2
                    );
                }
            }
        }
    }

    #[test]
    fn test_get_manifest_format_edge_cases() {
        // Test files with multiple dots
        assert_eq!(
            get_manifest_format("config.backup.yaml"),
            Some(ManifestFormat::Yaml)
        );
        assert_eq!(
            get_manifest_format("data.old.json"),
            Some(ManifestFormat::Json)
        );
        assert_eq!(
            get_manifest_format("rules.legacy.txt"),
            Some(ManifestFormat::Txt)
        );

        // Test files with no extension after dot
        assert_eq!(get_manifest_format("filename."), None);

        // Test files with only dots
        assert_eq!(get_manifest_format("..."), None);
        assert_eq!(get_manifest_format(".hidden"), None);
    }

    #[test]
    fn test_get_basename_edge_cases() {
        // Test files with multiple extensions
        assert_eq!(get_basename("config.backup.yaml"), "config.backup");
        assert_eq!(get_basename("data.old.json"), "data.old");

        // Test files with dots at start
        assert_eq!(get_basename(".hidden.txt"), ".hidden");
        assert_eq!(get_basename("..double.yaml"), "..double");

        // Test very long filenames
        let long_name = "very_long_filename_that_exceeds_normal_length.txt";
        assert_eq!(
            get_basename(long_name),
            "very_long_filename_that_exceeds_normal_length"
        );

        // Test filename that is just an extension
        assert_eq!(get_basename(".txt"), "");
    }

    #[test]
    fn test_manifest_format_cloning_and_equality() {
        let format1 = ManifestFormat::Txt;
        let format2 = format1.clone();
        assert_eq!(format1, format2);

        let format3 = ManifestFormat::Yaml;
        let format4 = format3.clone();
        assert_eq!(format3, format4);

        assert_ne!(format1, format3);
    }

    #[test]
    fn test_manifest_schema_serialization() {
        let schema = ManifestSchema {
            name: "Test Schema".to_string(),
            description: Some("A test schema".to_string()),
            rules: vec!["rule1.mdc".to_string(), "rule2.mdc".to_string()],
        };

        // Test JSON serialization round-trip
        let json_str = serde_json::to_string(&schema).unwrap();
        let deserialized: ManifestSchema = serde_json::from_str(&json_str).unwrap();

        assert_eq!(schema.name, deserialized.name);
        assert_eq!(schema.description, deserialized.description);
        assert_eq!(schema.rules, deserialized.rules);
    }

    #[test]
    fn test_manifest_creation_and_cloning() {
        let manifest = Manifest {
            name: "Test Manifest".to_string(),
            description: Some("Description".to_string()),
            entries: vec!["entry1.mdc".to_string()],
            errors: vec!["Error 1".to_string()],
            warnings: vec!["Warning 1".to_string()],
        };

        let cloned = manifest.clone();
        assert_eq!(manifest.name, cloned.name);
        assert_eq!(manifest.description, cloned.description);
        assert_eq!(manifest.entries, cloned.entries);
        assert_eq!(manifest.errors, cloned.errors);
        assert_eq!(manifest.warnings, cloned.warnings);

        // Test debug formatting
        let debug_str = format!("{:?}", manifest);
        assert!(debug_str.contains("Test Manifest"));
        assert!(debug_str.contains("entry1.mdc"));
    }

    #[test]
    fn test_parse_txt_manifest_extreme_whitespace() {
        // Test tabs, spaces, carriage returns
        let content =
            "\r\n\t   \n\r\t frontend/react.mdc \t\r\n\n\t\r   backend/rust.mdc\t\t\r\n\r\n";
        let result = parse_txt_manifest(content).unwrap();
        assert_eq!(result, vec!["frontend/react.mdc", "backend/rust.mdc"]);
    }

    #[test]
    fn test_parse_txt_manifest_mixed_comment_styles() {
        let content = "# Comment 1\nfrontend/react.mdc\n#Comment2\n # Comment 3\n   # Comment 4   \nbackend/rust.mdc\n#Final comment";
        let result = parse_txt_manifest(content).unwrap();
        assert_eq!(result, vec!["frontend/react.mdc", "backend/rust.mdc"]);
    }

    #[test]
    fn test_parse_txt_manifest_only_whitespace_and_comments() {
        let content = "\n\t\r\n#comment\n   \t  \n# another comment\n\r\n\t";
        let result = parse_txt_manifest(content).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_manifest_format_no_split_possible() {
        // Test filename with no dots at all
        assert_eq!(get_manifest_format("manifest"), None);

        // Test empty string edge case in split
        assert_eq!(get_manifest_format(""), None);

        // Test single character
        assert_eq!(get_manifest_format("a"), None);
        assert_eq!(get_manifest_format("a.txt"), Some(ManifestFormat::Txt));
    }

    #[test]
    fn test_get_basename_no_dots() {
        // Test files with no dots - should return entire filename
        assert_eq!(get_basename("manifest"), "manifest");
        assert_eq!(get_basename("MANIFEST"), "MANIFEST");
        assert_eq!(get_basename("123"), "123");
        assert_eq!(
            get_basename("file_with_underscores"),
            "file_with_underscores"
        );
    }

    #[test]
    fn test_manifest_format_debug_and_clone() {
        // Test Debug trait implementation
        let txt_format = ManifestFormat::Txt;
        let debug_str = format!("{:?}", txt_format);
        assert!(debug_str.contains("Txt"));

        let yaml_format = ManifestFormat::Yaml;
        let debug_str = format!("{:?}", yaml_format);
        assert!(debug_str.contains("Yaml"));

        let json_format = ManifestFormat::Json;
        let debug_str = format!("{:?}", json_format);
        assert!(debug_str.contains("Json"));

        // Test Clone trait
        let format1 = ManifestFormat::Txt;
        let format2 = format1.clone();
        assert_eq!(format1, format2);
    }

    #[test]
    fn test_parse_yaml_manifest_empty_rules() {
        let content = r#"
name: "Empty Rules"
description: "No rules"
rules: []
"#;
        let result = parse_yaml_manifest(content).unwrap();
        assert_eq!(result.name, "Empty Rules");
        assert_eq!(result.description, Some("No rules".to_string()));
        assert!(result.rules.is_empty());
    }

    #[test]
    fn test_parse_json_manifest_empty_rules() {
        let content = r#"
{
  "name": "Empty JSON",
  "description": "No rules here",
  "rules": []
}
"#;
        let result = parse_json_manifest(content).unwrap();
        assert_eq!(result.name, "Empty JSON");
        assert_eq!(result.description, Some("No rules here".to_string()));
        assert!(result.rules.is_empty());
    }

    // Note: find_manifests_in_quickadd tests require GitHub API access
    // These are covered by integration tests with real repositories

    // Note: Full integration test for file validation requires GitHub API access
    // This functionality is tested via CLI integration tests instead
}
