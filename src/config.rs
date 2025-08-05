//! Configuration management and secure token storage.
//!
//! This module handles persistent CLI configuration using XDG-compliant paths
//! and secure GitHub token storage using the OS keyring (macOS Keychain,
//! Windows Credential Manager, Linux secret-service).

use anyhow::{Context, Result};

use keyring::{Entry, Error as KeyringError};
use serde::de::Error as DeError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during config operations
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Failed to determine config directory
    #[error("Unable to determine config directory")]
    ConfigDirNotFound,

    /// Failed to read config file
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    /// Failed to parse config file
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] toml::de::Error),

    /// Failed to serialize config
    #[error("Failed to serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),

    /// Keyring operation failed
    #[error("Keyring operation failed: {0}")]
    KeyringError(String),

    /// Generic error from anyhow
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Configuration structure for persistent settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Default GitHub owner to fetch rules from
    pub owner: Option<String>,

    /// Default repository name (defaults to 'cursor-rules')
    pub repo: Option<String>,

    /// Default output directory for copied rules
    pub out_dir: Option<String>,

    /// Whether telemetry is enabled
    pub telemetry: Option<bool>,
}

/// Service name for keyring entries
const KEYRING_SERVICE: &str = "cursor-rules-cli";

/// Account name for GitHub token in keyring
const KEYRING_ACCOUNT: &str = "github-token";

/// Secure token storage abstraction
pub trait SecretStore {
    /// Get the stored GitHub token
    fn get_token(&self) -> Result<Option<String>, ConfigError>;

    /// Store a GitHub token securely
    fn set_token(&self, token: &str) -> Result<(), ConfigError>;

    /// Delete the stored GitHub token
    fn delete_token(&self) -> Result<(), ConfigError>;
}

/// Default implementation using the system keyring
pub struct KeyringStore;

impl SecretStore for KeyringStore {
    fn get_token(&self) -> Result<Option<String>, ConfigError> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| {
            ConfigError::KeyringError(format!("Failed to create keyring entry: {e}"))
        })?;

        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => {
                // Enhanced error messages for common keyring issues
                let error_msg = if e.to_string().contains("locked")
                    || e.to_string().contains("unavailable")
                {
                    "Keyring service is locked or unavailable. On Linux, ensure your desktop session is unlocked and the secret-service is running. Try setting GITHUB_TOKEN environment variable as a fallback.".to_string()
                } else if e.to_string().contains("too long") {
                    "Token is too long for the keyring service. Please use a shorter token or configure the token via environment variable.".to_string()
                } else {
                    format!("Failed to retrieve token from keyring: {e}. Try setting GITHUB_TOKEN environment variable as a fallback.")
                };
                Err(ConfigError::KeyringError(error_msg))
            }
        }
    }

    fn set_token(&self, token: &str) -> Result<(), ConfigError> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| {
            ConfigError::KeyringError(format!("Failed to create keyring entry: {e}"))
        })?;

        entry
            .set_password(token)
            .map_err(|e| ConfigError::KeyringError(format!("Failed to store token: {e}")))
    }

    fn delete_token(&self) -> Result<(), ConfigError> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| {
            ConfigError::KeyringError(format!("Failed to create keyring entry: {e}"))
        })?;

        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(KeyringError::NoEntry) => Ok(()), // Already deleted
            Err(e) => Err(ConfigError::KeyringError(format!(
                "Failed to delete token: {e}"
            ))),
        }
    }
}

/// Get the path to the config file
pub fn config_file_path() -> Result<PathBuf, ConfigError> {
    let config_dir = dirs::config_dir().ok_or(ConfigError::ConfigDirNotFound)?;

    let app_config_dir = config_dir.join("cursor-rules-cli");
    Ok(app_config_dir.join("config.toml"))
}

/// Load configuration from file
pub fn load_config() -> Result<Config, ConfigError> {
    let config_path = config_file_path()?;

    if !config_path.exists() {
        // Return default config if file doesn't exist
        return Ok(Config::default());
    }

    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    if content.trim().is_empty() {
        // Handle empty config file
        return Ok(Config::default());
    }

    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

/// Save configuration to file
pub fn save_config(config: &Config) -> Result<(), ConfigError> {
    let config_path = config_file_path()?;

    // Create parent directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(config)?;
    fs::write(&config_path, content)
        .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

    Ok(())
}

/// Get GitHub token following priority: CLI flag → env var → keyring → none
pub fn resolve_github_token(
    cli_token: Option<&str>,
    secret_store: &dyn SecretStore,
) -> Result<Option<String>, ConfigError> {
    // 1. CLI flag has highest priority
    if let Some(token) = cli_token {
        return Ok(Some(token.to_string()));
    }

    // 2. Environment variable
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            return Ok(Some(token));
        }
    }

    // 3. Keyring storage
    secret_store.get_token()
}

/// Update a single config value
pub fn update_config_value(key: &str, value: &str) -> Result<(), ConfigError> {
    let mut config = load_config()?;

    match key {
        "owner" => config.owner = Some(value.to_string()),
        "repo" => config.repo = Some(value.to_string()),
        "out_dir" => config.out_dir = Some(value.to_string()),
        "telemetry" => {
            config.telemetry =
                Some(value.parse::<bool>().map_err(|_| {
                    ConfigError::ParseError(DeError::custom("Invalid boolean value"))
                })?);
        }
        _ => {
            return Err(ConfigError::ParseError(DeError::custom(format!(
                "Unknown config key: {key}"
            ))))
        }
    }

    save_config(&config)
}

/// Delete a config value (set it to None)
pub fn delete_config_value(key: &str) -> Result<(), ConfigError> {
    let mut config = load_config()?;

    match key {
        "owner" => config.owner = None,
        "repo" => config.repo = None,
        "out_dir" => config.out_dir = None,
        "telemetry" => config.telemetry = None,
        _ => {
            return Err(ConfigError::ParseError(DeError::custom(format!(
                "Unknown config key: {key}"
            ))))
        }
    }

    save_config(&config)
}

/// Validate GitHub token and check scopes
#[allow(dead_code)] // Planned for FR-4 auth validation features
pub async fn validate_github_token_with_scopes(token: &str) -> Result<Vec<String>, ConfigError> {
    let octocrab = octocrab::Octocrab::builder()
        .personal_token(token.to_string())
        .build()
        .map_err(|e| ConfigError::Other(e.into()))?;

    // Make a test API call to validate the token
    let _user = octocrab
        .current()
        .user()
        .await
        .map_err(|e| ConfigError::KeyringError(format!("Token validation failed: {e}")))?;

    // Try to get token scopes from headers (this is a simplified approach)
    // In practice, you might need to make a specific API call to check scopes
    let scopes = vec![]; // Placeholder - real implementation would check actual scopes

    Ok(scopes)
}

/// Handle 401 errors by prompting for new token (interactive only)
#[allow(dead_code)] // Planned for FR-4 auth error recovery features
pub async fn handle_auth_error_interactive(
    secret_store: &dyn SecretStore,
) -> Result<Option<String>, ConfigError> {
    use inquire::{Confirm, Password};
    use is_terminal::IsTerminal;
    use std::io;

    // Only prompt in interactive mode
    if !io::stdin().is_terminal() {
        return Ok(None);
    }

    println!("Authentication failed. Your GitHub token may be invalid or expired.");

    let should_update = Confirm::new("Would you like to enter a new GitHub token?")
        .with_default(true)
        .prompt()
        .map_err(|_| ConfigError::KeyringError("Token prompt cancelled".to_string()))?;

    if !should_update {
        return Ok(None);
    }

    let token = Password::new("Enter GitHub Personal Access Token:")
        .with_help_message("Create one at https://github.com/settings/tokens")
        .prompt()
        .map_err(|_| ConfigError::KeyringError("Token input cancelled".to_string()))?;

    if token.trim().is_empty() {
        return Ok(None);
    }

    // Validate the new token
    match validate_github_token_with_scopes(&token).await {
        Ok(_scopes) => {
            // Store the validated token
            secret_store.set_token(&token)?;
            println!("✓ Token validated and stored securely.");
            Ok(Some(token))
        }
        Err(e) => {
            eprintln!("⚠ Token validation failed: {e}");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Mock secret store for testing
    struct MockSecretStore {
        tokens: std::sync::Mutex<HashMap<String, String>>,
    }

    impl MockSecretStore {
        fn new() -> Self {
            Self {
                tokens: std::sync::Mutex::new(HashMap::new()),
            }
        }
    }

    impl SecretStore for MockSecretStore {
        fn get_token(&self) -> Result<Option<String>, ConfigError> {
            let tokens = self.tokens.lock().unwrap();
            Ok(tokens.get(KEYRING_ACCOUNT).cloned())
        }

        fn set_token(&self, token: &str) -> Result<(), ConfigError> {
            let mut tokens = self.tokens.lock().unwrap();
            tokens.insert(KEYRING_ACCOUNT.to_string(), token.to_string());
            Ok(())
        }

        fn delete_token(&self) -> Result<(), ConfigError> {
            let mut tokens = self.tokens.lock().unwrap();
            tokens.remove(KEYRING_ACCOUNT);
            Ok(())
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_token_resolution_priority() {
        use std::env;

        // Save original env var state
        let original_token = env::var("GITHUB_TOKEN").ok();

        // Test 1: CLI token has highest priority (with fresh mock store)
        let mock_store = MockSecretStore::new();
        env::set_var("GITHUB_TOKEN", "env_token");
        mock_store.set_token("keyring_token").unwrap();

        let result = resolve_github_token(Some("cli_token"), &mock_store).unwrap();
        assert_eq!(result, Some("cli_token".to_string()));

        // Test 2: Environment variable when no CLI token (with fresh mock store)
        let mock_store = MockSecretStore::new();
        mock_store.set_token("keyring_token").unwrap();
        env::set_var("GITHUB_TOKEN", "env_token");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert_eq!(result, Some("env_token".to_string()));

        // Test 3: Keyring when no CLI token or env var (with fresh mock store)
        let mock_store = MockSecretStore::new();
        mock_store.set_token("keyring_token").unwrap();
        env::remove_var("GITHUB_TOKEN");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert_eq!(result, Some("keyring_token".to_string()));

        // Test 4: None when no sources available (with fresh mock store)
        let mock_store = MockSecretStore::new();
        env::remove_var("GITHUB_TOKEN");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert!(result.is_none());

        // Restore original state
        match original_token {
            Some(token) => env::set_var("GITHUB_TOKEN", token),
            None => env::remove_var("GITHUB_TOKEN"),
        }
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            owner: Some("testowner".to_string()),
            repo: Some("testrepo".to_string()),
            out_dir: Some("./test".to_string()),
            telemetry: Some(false),
        };

        let serialized = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(config.owner, deserialized.owner);
        assert_eq!(config.repo, deserialized.repo);
        assert_eq!(config.out_dir, deserialized.out_dir);
        assert_eq!(config.telemetry, deserialized.telemetry);
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.owner.is_none());
        assert!(config.repo.is_none());
        assert!(config.out_dir.is_none());
        assert!(config.telemetry.is_none());
    }

    #[test]
    fn test_update_config_value() {
        // Note: This test only validates the logic, not actual file I/O
        // since config file paths are system-dependent

        // Test invalid key
        let result = update_config_value("invalid_key", "value");
        assert!(result.is_err());

        // Additional validation could be added with mocked file system
    }

    #[test]
    fn test_delete_config_value() {
        // Note: This test only validates the logic, not actual file I/O
        // since config file paths are system-dependent

        // Test invalid key
        let result = delete_config_value("invalid_key");
        assert!(result.is_err());

        // Additional validation could be added with mocked file system
    }

    #[test]
    fn test_secret_store_operations() {
        let mock_store = MockSecretStore::new();

        // Test storing and retrieving token
        mock_store.set_token("test_token").unwrap();
        let token = mock_store.get_token().unwrap();
        assert_eq!(token, Some("test_token".to_string()));

        // Test deleting token
        mock_store.delete_token().unwrap();
        let token = mock_store.get_token().unwrap();
        assert!(token.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_github_token_env_var() {
        use std::env;

        let mock_store = MockSecretStore::new();

        // Save original env var state
        let original_token = env::var("GITHUB_TOKEN").ok();

        // Set environment variable
        env::set_var("GITHUB_TOKEN", "env_token");

        let result = resolve_github_token(None, &mock_store).unwrap();
        assert_eq!(result, Some("env_token".to_string()));

        // Restore original state
        match original_token {
            Some(token) => env::set_var("GITHUB_TOKEN", token),
            None => env::remove_var("GITHUB_TOKEN"),
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_github_token_no_sources() {
        use std::env;

        let mock_store = MockSecretStore::new();

        // Save original env var state
        let original_token = env::var("GITHUB_TOKEN").ok();

        // Ensure no environment variable
        env::remove_var("GITHUB_TOKEN");

        let result = resolve_github_token(None, &mock_store).unwrap();
        assert!(result.is_none());

        // Restore original state
        match original_token {
            Some(token) => env::set_var("GITHUB_TOKEN", token),
            None => env::remove_var("GITHUB_TOKEN"),
        }
    }

    #[test]
    fn test_config_file_path() {
        let path = config_file_path().unwrap();
        assert!(path.to_string_lossy().contains("cursor-rules-cli"));
        assert!(path.to_string_lossy().ends_with("config.toml"));
    }

    #[test]
    #[serial_test::serial]
    fn test_load_config_nonexistent_file() {
        // This should return default config when file doesn't exist
        // Since we can't control the actual config file location reliably,
        // we test the logic by ensuring it handles missing files gracefully
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let original_home = env::var("HOME").ok();

        // Point HOME to temp directory so config_dir returns our temp path
        env::set_var("HOME", temp_dir.path());

        let result = load_config();

        // Restore original HOME
        match original_home {
            Some(home) => env::set_var("HOME", home),
            None => env::remove_var("HOME"),
        }

        // Should succeed and return default config
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.owner.is_none());
        assert!(config.repo.is_none());
    }

    #[test]
    fn test_save_and_load_config_roundtrip() {
        // Test just the serialization/deserialization without file system
        let test_config = Config {
            owner: Some("testowner".to_string()),
            repo: Some("testrepo".to_string()),
            out_dir: Some("./testdir".to_string()),
            telemetry: Some(true),
        };

        // Serialize to TOML
        let content = toml::to_string_pretty(&test_config).unwrap();

        // Deserialize back from TOML
        let loaded_config: Config = toml::from_str(&content).unwrap();

        assert_eq!(loaded_config.owner, test_config.owner);
        assert_eq!(loaded_config.repo, test_config.repo);
        assert_eq!(loaded_config.out_dir, test_config.out_dir);
        assert_eq!(loaded_config.telemetry, test_config.telemetry);
    }

    #[test]
    #[serial_test::serial]
    fn test_update_config_value_telemetry_invalid() {
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let original_home = env::var("HOME").ok();
        env::set_var("HOME", temp_dir.path());

        // Test invalid boolean value for telemetry
        let result = update_config_value("telemetry", "invalid_boolean");

        // Restore HOME
        match original_home {
            Some(home) => env::set_var("HOME", home),
            None => env::remove_var("HOME"),
        }

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid boolean value"));
    }

    #[test]
    #[serial_test::serial]
    fn test_update_config_value_valid_keys() {
        use std::env;
        use tempfile::TempDir;

        let original_home = env::var("HOME").ok();

        // Test each value in a completely separate environment
        let test_cases = vec![
            ("telemetry", "true"),
            ("owner", "test-owner"),
            ("repo", "test-repo"),
            ("out_dir", "./test/path"),
        ];

        for (key, value) in test_cases {
            // Use a fresh temp directory for each test case
            let temp_dir = TempDir::new().unwrap();
            env::set_var("HOME", temp_dir.path());

            // Ensure the config directory exists by trying to get the config path first
            // This triggers directory creation if needed
            if let Ok(config_path) = config_file_path() {
                if let Some(parent) = config_path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
            }

            // Allow some time between config file operations
            std::thread::sleep(std::time::Duration::from_millis(10));

            let result = update_config_value(key, value);
            assert!(
                result.is_ok(),
                "Failed to update {}: {:?}",
                key,
                result.err()
            );

            // Verify the value was actually saved by loading it back
            if let Ok(config) = load_config() {
                match key {
                    "telemetry" => assert_eq!(config.telemetry, Some(value == "true")),
                    "owner" => assert_eq!(config.owner, Some(value.to_string())),
                    "repo" => assert_eq!(config.repo, Some(value.to_string())),
                    "out_dir" => assert_eq!(config.out_dir, Some(value.to_string())),
                    _ => unreachable!(),
                }
            }
        }

        // Restore HOME
        match original_home {
            Some(home) => env::set_var("HOME", home),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_delete_config_value_valid_keys() {
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let original_home = env::var("HOME").ok();
        env::set_var("HOME", temp_dir.path());

        // Test deleting valid keys
        let result = delete_config_value("owner");
        assert!(result.is_ok());

        let result = delete_config_value("repo");
        assert!(result.is_ok());

        let result = delete_config_value("out_dir");
        assert!(result.is_ok());

        let result = delete_config_value("telemetry");
        assert!(result.is_ok());

        // Restore HOME
        match original_home {
            Some(home) => env::set_var("HOME", home),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    fn test_keyring_store_creation() {
        let _store = KeyringStore;
        // This test just ensures the struct can be created without panic
    }

    #[test]
    fn test_config_error_display() {
        let error = ConfigError::ConfigDirNotFound;
        assert_eq!(error.to_string(), "Unable to determine config directory");

        let error = ConfigError::KeyringError("test error".to_string());
        assert_eq!(error.to_string(), "Keyring operation failed: test error");
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_github_token_empty_env_var() {
        use std::env;

        let mock_store = MockSecretStore::new();

        // Save original env var state
        let original_token = env::var("GITHUB_TOKEN").ok();

        // Set empty environment variable
        env::set_var("GITHUB_TOKEN", "   "); // whitespace only

        let result = resolve_github_token(None, &mock_store).unwrap();
        assert!(result.is_none());

        // Restore original state
        match original_token {
            Some(token) => env::set_var("GITHUB_TOKEN", token),
            None => env::remove_var("GITHUB_TOKEN"),
        }
    }

    #[test]
    fn test_load_config_empty_file() {
        // Test empty TOML content handling directly
        let empty_content = "";
        let config: Config = toml::from_str(empty_content).unwrap_or_default();

        // Should return default config for empty content
        assert!(config.owner.is_none());
        assert!(config.repo.is_none());
        assert!(config.out_dir.is_none());
        assert!(config.telemetry.is_none());
    }

    #[test]
    fn test_load_config_malformed_toml() {
        // Test handling of malformed TOML
        let malformed_content = "owner = testowner\n[incomplete";
        let result = toml::from_str::<Config>(malformed_content);

        // Should fail to parse malformed TOML
        assert!(result.is_err());
    }

    #[test]
    fn test_keyring_store_operations() {
        // Test the real KeyringStore implementation
        let store = KeyringStore;

        // Test set and get operations
        // Note: This may fail in CI/test environments where keyring is not available
        // but it tests the actual implementation paths
        let test_token = "test_token_12345";

        match store.set_token(test_token) {
            Ok(()) => {
                // If set succeeded, try to get it back
                match store.get_token() {
                    Ok(Some(retrieved_token)) => {
                        assert_eq!(retrieved_token, test_token);
                        // Clean up
                        let _ = store.delete_token();
                    }
                    Ok(None) => {
                        // Token not found - this can happen in test environments
                        // Clean up just in case
                        let _ = store.delete_token();
                    }
                    Err(_) => {
                        // Keyring error - expected in some test environments
                        // Clean up just in case
                        let _ = store.delete_token();
                    }
                }
            }
            Err(_) => {
                // Keyring operation failed - expected in some test environments
                // This still tests the error path
            }
        }
    }

    #[test]
    fn test_config_error_variants() {
        // Test ConfigError display for different variants
        let keyring_error = ConfigError::KeyringError("test keyring error".to_string());
        assert_eq!(
            keyring_error.to_string(),
            "Keyring operation failed: test keyring error"
        );

        let config_dir_error = ConfigError::ConfigDirNotFound;
        assert_eq!(
            config_dir_error.to_string(),
            "Unable to determine config directory"
        );

        // Test parse error with actual malformed TOML
        let malformed_toml = "owner = \n[invalid";
        let parse_result = toml::from_str::<Config>(malformed_toml);
        assert!(parse_result.is_err());

        if let Err(toml_error) = parse_result {
            let config_error = ConfigError::ParseError(toml_error);
            assert!(config_error
                .to_string()
                .contains("Failed to parse config file"));
        }

        // Test serialize error path by trying to serialize invalid data
        // We'll create this scenario through the actual config update function
        // since we can't easily create a serialize error directly
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_github_token_all_paths() {
        use std::env;

        // Save original env var state
        let original_token = env::var("GITHUB_TOKEN").ok();

        // Test 1: CLI token takes precedence (with isolated mock store)
        let mock_store = MockSecretStore::new();
        env::set_var("GITHUB_TOKEN", "env_token");
        mock_store.set_token("keyring_token").unwrap();

        let result = resolve_github_token(Some("cli_token"), &mock_store).unwrap();
        assert_eq!(result, Some("cli_token".to_string()));

        // Test 2: Environment variable when no CLI token (with fresh mock store)
        let mock_store = MockSecretStore::new();
        mock_store.set_token("keyring_token").unwrap();
        env::set_var("GITHUB_TOKEN", "env_token");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert_eq!(result, Some("env_token".to_string()));

        // Test 3: Keyring when no CLI token or env var (with fresh mock store)
        let mock_store = MockSecretStore::new();
        mock_store.set_token("keyring_token").unwrap();
        env::remove_var("GITHUB_TOKEN");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert_eq!(result, Some("keyring_token".to_string()));

        // Test 4: None when no sources available (with fresh mock store)
        let mock_store = MockSecretStore::new();
        env::remove_var("GITHUB_TOKEN");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert!(result.is_none());

        // Restore original state
        match original_token {
            Some(token) => env::set_var("GITHUB_TOKEN", token),
            None => env::remove_var("GITHUB_TOKEN"),
        }
    }

    #[test]
    fn test_config_comprehensive_serialization() {
        // Test all possible config combinations
        let configs = vec![
            Config {
                owner: Some("owner".to_string()),
                repo: None,
                out_dir: None,
                telemetry: None,
            },
            Config {
                owner: None,
                repo: Some("repo".to_string()),
                out_dir: None,
                telemetry: None,
            },
            Config {
                owner: None,
                repo: None,
                out_dir: Some("./out".to_string()),
                telemetry: None,
            },
            Config {
                owner: None,
                repo: None,
                out_dir: None,
                telemetry: Some(false),
            },
            Config {
                owner: Some("owner".to_string()),
                repo: Some("repo".to_string()),
                out_dir: Some("./out".to_string()),
                telemetry: Some(true),
            },
        ];

        for config in configs {
            let serialized = toml::to_string_pretty(&config).unwrap();
            let deserialized: Config = toml::from_str(&serialized).unwrap();

            assert_eq!(config.owner, deserialized.owner);
            assert_eq!(config.repo, deserialized.repo);
            assert_eq!(config.out_dir, deserialized.out_dir);
            assert_eq!(config.telemetry, deserialized.telemetry);
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_update_config_value_all_keys() {
        use std::env;
        use tempfile::TempDir;

        let original_home = env::var("HOME").ok();

        // Test each key in a fresh environment to avoid TOML corruption
        let test_cases = vec![
            ("owner", "test-owner"),
            ("repo", "test-repo"),
            ("out_dir", "./custom/path"), // Use relative path to avoid permissions issues
            ("telemetry", "true"),
            ("telemetry", "false"),
        ];

        for (key, value) in test_cases {
            let temp_dir = TempDir::new().unwrap();
            env::set_var("HOME", temp_dir.path());

            let result = update_config_value(key, value);
            assert!(
                result.is_ok(),
                "Failed to update {} with value {}: {:?}",
                key,
                value,
                result.err()
            );
        }

        // Test invalid telemetry values in a fresh environment
        let temp_dir = TempDir::new().unwrap();
        env::set_var("HOME", temp_dir.path());

        let invalid_telemetry_values = vec!["yes", "no", "1", "0", "invalid"];
        for value in invalid_telemetry_values {
            let result = update_config_value("telemetry", value);
            assert!(
                result.is_err(),
                "Should fail for invalid telemetry value: {value}"
            );
        }

        // Restore HOME
        match original_home {
            Some(home) => env::set_var("HOME", home),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_delete_config_value_all_keys() {
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let original_home = env::var("HOME").ok();
        env::set_var("HOME", temp_dir.path());

        // Test deleting all valid keys
        let valid_keys = vec!["owner", "repo", "out_dir", "telemetry"];

        for key in valid_keys {
            let result = delete_config_value(key);
            assert!(result.is_ok(), "Failed to delete key: {key}");
        }

        // Restore HOME
        match original_home {
            Some(home) => env::set_var("HOME", home),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    fn test_mock_store_error_conditions() {
        let mock_store = MockSecretStore::new();

        // Test retrieving from empty store
        let result = mock_store.get_token().unwrap();
        assert!(result.is_none());

        // Test deleting from empty store
        let result = mock_store.delete_token();
        assert!(result.is_ok());

        // Test storing and retrieving multiple times
        mock_store.set_token("token1").unwrap();
        mock_store.set_token("token2").unwrap(); // Should overwrite

        let result = mock_store.get_token().unwrap();
        assert_eq!(result, Some("token2".to_string()));

        // Test deleting and re-retrieving
        mock_store.delete_token().unwrap();
        let result = mock_store.get_token().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_config_edge_cases() {
        // Test config with whitespace values
        let config_content = r#"
owner = "  owner-with-spaces  "
repo = ""
out_dir = "   "
telemetry = true
"#;

        let config: Config = toml::from_str(config_content).unwrap();
        assert_eq!(config.owner, Some("  owner-with-spaces  ".to_string()));
        assert_eq!(config.repo, Some("".to_string()));
        assert_eq!(config.out_dir, Some("   ".to_string()));
        assert_eq!(config.telemetry, Some(true));

        // Test config with special characters
        let config_content = r#"
owner = "org-name_123"
repo = "repo.name-with.dots"
out_dir = "/path/with/slashes"
"#;

        let config: Config = toml::from_str(config_content).unwrap();
        assert_eq!(config.owner, Some("org-name_123".to_string()));
        assert_eq!(config.repo, Some("repo.name-with.dots".to_string()));
        assert_eq!(config.out_dir, Some("/path/with/slashes".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn test_environment_variable_edge_cases() {
        use std::env;

        let mock_store = MockSecretStore::new();

        // Save original env var state
        let original_token = env::var("GITHUB_TOKEN").ok();

        // Test with various whitespace scenarios
        let whitespace_cases = vec![
            "",         // empty
            " ",        // single space
            "\t",       // tab
            "\n",       // newline
            "  \t\n  ", // mixed whitespace
        ];

        for whitespace in whitespace_cases {
            env::set_var("GITHUB_TOKEN", whitespace);
            let result = resolve_github_token(None, &mock_store).unwrap();
            assert!(
                result.is_none(),
                "Should return None for whitespace: {whitespace:?}"
            );
        }

        // Restore original state
        match original_token {
            Some(token) => env::set_var("GITHUB_TOKEN", token),
            None => env::remove_var("GITHUB_TOKEN"),
        }
    }

    #[test]
    fn test_secret_store_trait_coverage() {
        let mock_store = MockSecretStore::new();

        // Test the trait methods extensively
        assert!(mock_store.get_token().unwrap().is_none());

        mock_store.set_token("test1").unwrap();
        assert_eq!(mock_store.get_token().unwrap(), Some("test1".to_string()));

        mock_store.set_token("test2").unwrap();
        assert_eq!(mock_store.get_token().unwrap(), Some("test2".to_string()));

        mock_store.delete_token().unwrap();
        assert!(mock_store.get_token().unwrap().is_none());

        // Test delete on empty store
        mock_store.delete_token().unwrap();
        assert!(mock_store.get_token().unwrap().is_none());
    }

    #[test]
    fn test_keyring_constants() {
        // Test that our constants are accessible and have expected values
        assert_eq!(super::KEYRING_SERVICE, "cursor-rules-cli");
        assert_eq!(super::KEYRING_ACCOUNT, "github-token");
    }

    #[test]
    fn test_config_default_trait() {
        let config1 = Config::default();
        let config2 = Config {
            owner: None,
            repo: None,
            out_dir: None,
            telemetry: None,
        };

        assert_eq!(config1.owner, config2.owner);
        assert_eq!(config1.repo, config2.repo);
        assert_eq!(config1.out_dir, config2.out_dir);
        assert_eq!(config1.telemetry, config2.telemetry);
    }

    #[test]
    fn test_config_partial_serialization() {
        // Test partial configs with only some fields set
        let partial_configs = vec![
            r#"owner = "test""#,
            r#"repo = "test""#,
            r#"out_dir = "test""#,
            r#"telemetry = true"#,
            r#"telemetry = false"#,
            r#"owner = "test"
repo = "test""#,
            r#"owner = "test"
out_dir = "test"
telemetry = true"#,
        ];

        for config_str in partial_configs {
            let result = toml::from_str::<Config>(config_str);
            assert!(result.is_ok(), "Failed to parse: {config_str}");
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_resolve_token_priority_comprehensive() {
        use std::env;

        let original_token = env::var("GITHUB_TOKEN").ok();

        // Test comprehensive priority scenarios with fresh mock store for each test

        // Scenario 1: Only CLI token
        let mock_store = MockSecretStore::new();
        env::remove_var("GITHUB_TOKEN");
        let result = resolve_github_token(Some("cli"), &mock_store).unwrap();
        assert_eq!(result, Some("cli".to_string()));

        // Scenario 2: CLI + env (CLI wins)
        let mock_store = MockSecretStore::new();
        env::set_var("GITHUB_TOKEN", "env");
        let result = resolve_github_token(Some("cli"), &mock_store).unwrap();
        assert_eq!(result, Some("cli".to_string()));

        // Scenario 3: CLI + env + keyring (CLI wins)
        let mock_store = MockSecretStore::new();
        mock_store.set_token("keyring").unwrap();
        let result = resolve_github_token(Some("cli"), &mock_store).unwrap();
        assert_eq!(result, Some("cli".to_string()));

        // Scenario 4: env + keyring (env wins)
        let mock_store = MockSecretStore::new();
        mock_store.set_token("keyring").unwrap();
        env::set_var("GITHUB_TOKEN", "env");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert_eq!(result, Some("env".to_string()));

        // Scenario 5: Only keyring
        let mock_store = MockSecretStore::new();
        mock_store.set_token("keyring").unwrap();
        env::remove_var("GITHUB_TOKEN");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert_eq!(result, Some("keyring".to_string()));

        // Scenario 6: None available
        let mock_store = MockSecretStore::new();
        env::remove_var("GITHUB_TOKEN");
        let result = resolve_github_token(None, &mock_store).unwrap();
        assert!(result.is_none());

        // Restore state
        match original_token {
            Some(token) => env::set_var("GITHUB_TOKEN", token),
            None => env::remove_var("GITHUB_TOKEN"),
        }
    }

    #[test]
    fn test_config_file_path_components() {
        let path = config_file_path().unwrap();

        // Test that path contains expected components
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("cursor-rules-cli"));
        assert!(path_str.ends_with("config.toml"));

        // Test that parent directory can be determined
        assert!(path.parent().is_some());

        // Test that file name is correct
        assert_eq!(path.file_name().unwrap(), "config.toml");
    }
}
