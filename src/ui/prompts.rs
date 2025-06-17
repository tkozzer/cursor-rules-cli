//! Interactive prompts for conflict resolution during file copying.
//!
//! This module provides a trait-based prompt service that can be used
//! for interactive conflict resolution when copying files with potential
//! overwrites.

use anyhow::Result;
use inquire::Select;
use is_terminal::IsTerminal;

/// Represents the user's choice for handling a file conflict
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    /// Overwrite the existing file
    Overwrite,
    /// Skip this file (don't copy)
    Skip,
    /// Rename the new file to avoid conflict
    Rename,
    /// Apply overwrite to all remaining conflicts
    OverwriteAll,
    /// Skip all remaining conflicts
    SkipAll,
    /// Rename all remaining conflicts
    RenameAll,
    /// Cancel the entire operation
    Cancel,
}

/// Trait for prompting users about file conflicts
///
/// This trait allows for dependency injection and easier testing
/// by providing mock implementations.
pub trait PromptService: Send + Sync {
    /// Prompt the user for how to handle a file conflict
    ///
    /// # Arguments
    /// * `filename` - The name of the conflicting file
    /// * `source_path` - The source path in the repository
    /// * `dest_path` - The destination path on the filesystem
    ///
    /// # Returns
    /// The user's choice for handling the conflict
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    fn prompt_conflict(
        &self,
        filename: &str,
        source_path: &str,
        dest_path: &str,
    ) -> Result<ConflictChoice>;

    /// Check if prompting is available (e.g., terminal is interactive)
    fn can_prompt(&self) -> bool;
}

/// Interactive prompt service using inquire
pub struct InteractivePromptService;

impl InteractivePromptService {
    /// Create a new interactive prompt service
    pub fn new() -> Self {
        Self
    }
}

impl Default for InteractivePromptService {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptService for InteractivePromptService {
    fn prompt_conflict(
        &self,
        filename: &str,
        source_path: &str,
        _dest_path: &str,
    ) -> Result<ConflictChoice> {
        if !self.can_prompt() {
            // Non-interactive fallback: skip by default
            return Ok(ConflictChoice::Skip);
        }

        let message = format!(
            "File '{}' already exists. What would you like to do?",
            filename
        );

        let options = vec![
            "Overwrite",
            "Skip",
            "Rename",
            "Overwrite All",
            "Skip All",
            "Rename All",
            "Cancel",
        ];

        let help_message = format!(
            "Source: {}\nChoose how to handle this conflict:",
            source_path
        );

        let ans = Select::new(&message, options)
            .with_help_message(&help_message)
            .prompt()?;

        let choice = match ans {
            "Overwrite" => ConflictChoice::Overwrite,
            "Skip" => ConflictChoice::Skip,
            "Rename" => ConflictChoice::Rename,
            "Overwrite All" => ConflictChoice::OverwriteAll,
            "Skip All" => ConflictChoice::SkipAll,
            "Rename All" => ConflictChoice::RenameAll,
            "Cancel" => ConflictChoice::Cancel,
            _ => ConflictChoice::Cancel,
        };

        Ok(choice)
    }

    fn can_prompt(&self) -> bool {
        std::io::stdin().is_terminal()
    }
}

/// Non-interactive prompt service that always returns a default choice
pub struct NonInteractivePromptService {
    default_choice: ConflictChoice,
}

impl NonInteractivePromptService {
    /// Create a new non-interactive prompt service with a default choice
    pub fn new(default_choice: ConflictChoice) -> Self {
        Self { default_choice }
    }

    /// Create a service that always skips conflicts
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    pub fn skip_all() -> Self {
        Self::new(ConflictChoice::SkipAll)
    }

    /// Create a service that always overwrites
    pub fn overwrite_all() -> Self {
        Self::new(ConflictChoice::OverwriteAll)
    }

    /// Create a service that always renames
    #[allow(dead_code)] // Forward-looking feature for CLI integration
    pub fn rename_all() -> Self {
        Self::new(ConflictChoice::RenameAll)
    }
}

impl PromptService for NonInteractivePromptService {
    fn prompt_conflict(
        &self,
        _filename: &str,
        _source_path: &str,
        _dest_path: &str,
    ) -> Result<ConflictChoice> {
        Ok(self.default_choice)
    }

    fn can_prompt(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_choice_variants() {
        let choices = [
            ConflictChoice::Overwrite,
            ConflictChoice::Skip,
            ConflictChoice::Rename,
            ConflictChoice::OverwriteAll,
            ConflictChoice::SkipAll,
            ConflictChoice::RenameAll,
            ConflictChoice::Cancel,
        ];

        // Test that all variants can be created and compared
        for choice in &choices {
            assert_eq!(*choice, *choice);
        }
    }

    #[test]
    fn test_non_interactive_prompt_service() {
        let service = NonInteractivePromptService::skip_all();
        assert!(!service.can_prompt());

        let choice = service
            .prompt_conflict("test.mdc", "src/test.mdc", "dest/test.mdc")
            .unwrap();
        assert_eq!(choice, ConflictChoice::SkipAll);
    }

    #[test]
    fn test_non_interactive_prompt_service_overwrite() {
        let service = NonInteractivePromptService::overwrite_all();
        let choice = service
            .prompt_conflict("test.mdc", "src/test.mdc", "dest/test.mdc")
            .unwrap();
        assert_eq!(choice, ConflictChoice::OverwriteAll);
    }

    #[test]
    fn test_non_interactive_prompt_service_rename() {
        let service = NonInteractivePromptService::rename_all();
        let choice = service
            .prompt_conflict("test.mdc", "src/test.mdc", "dest/test.mdc")
            .unwrap();
        assert_eq!(choice, ConflictChoice::RenameAll);
    }

    #[test]
    fn test_interactive_prompt_service_creation() {
        let service = InteractivePromptService::new();
        let default_service = InteractivePromptService;

        // Both should behave the same way for can_prompt
        assert_eq!(service.can_prompt(), default_service.can_prompt());
    }

    /// Mock prompt service for testing
    pub struct MockPromptService {
        responses: Vec<ConflictChoice>,
        call_count: std::sync::RwLock<usize>,
    }

    impl MockPromptService {
        pub fn new(responses: Vec<ConflictChoice>) -> Self {
            Self {
                responses,
                call_count: std::sync::RwLock::new(0),
            }
        }

        pub fn call_count(&self) -> usize {
            *self.call_count.read().unwrap()
        }
    }

    impl PromptService for MockPromptService {
        fn prompt_conflict(
            &self,
            _filename: &str,
            _source_path: &str,
            _dest_path: &str,
        ) -> Result<ConflictChoice> {
            let mut count = self.call_count.write().unwrap();
            let response = self
                .responses
                .get(*count)
                .copied()
                .unwrap_or(ConflictChoice::Cancel);
            *count += 1;
            Ok(response)
        }

        fn can_prompt(&self) -> bool {
            true
        }
    }

    #[test]
    fn test_mock_prompt_service() {
        let responses = vec![
            ConflictChoice::Overwrite,
            ConflictChoice::Skip,
            ConflictChoice::RenameAll,
        ];
        let service = MockPromptService::new(responses);

        assert!(service.can_prompt());
        assert_eq!(service.call_count(), 0);

        let choice1 = service
            .prompt_conflict("file1.mdc", "src1", "dest1")
            .unwrap();
        assert_eq!(choice1, ConflictChoice::Overwrite);
        assert_eq!(service.call_count(), 1);

        let choice2 = service
            .prompt_conflict("file2.mdc", "src2", "dest2")
            .unwrap();
        assert_eq!(choice2, ConflictChoice::Skip);
        assert_eq!(service.call_count(), 2);

        let choice3 = service
            .prompt_conflict("file3.mdc", "src3", "dest3")
            .unwrap();
        assert_eq!(choice3, ConflictChoice::RenameAll);
        assert_eq!(service.call_count(), 3);

        // Should return Cancel when out of responses
        let choice4 = service
            .prompt_conflict("file4.mdc", "src4", "dest4")
            .unwrap();
        assert_eq!(choice4, ConflictChoice::Cancel);
        assert_eq!(service.call_count(), 4);
    }

    #[test]
    fn test_conflict_choice_equality() {
        // Test all conflict choice variants
        assert_eq!(ConflictChoice::Overwrite, ConflictChoice::Overwrite);
        assert_eq!(ConflictChoice::Skip, ConflictChoice::Skip);
        assert_eq!(ConflictChoice::Rename, ConflictChoice::Rename);
        assert_eq!(ConflictChoice::OverwriteAll, ConflictChoice::OverwriteAll);
        assert_eq!(ConflictChoice::SkipAll, ConflictChoice::SkipAll);
        assert_eq!(ConflictChoice::RenameAll, ConflictChoice::RenameAll);
        assert_eq!(ConflictChoice::Cancel, ConflictChoice::Cancel);

        // Test inequality
        assert_ne!(ConflictChoice::Overwrite, ConflictChoice::Skip);
        assert_ne!(ConflictChoice::Skip, ConflictChoice::Rename);
        assert_ne!(ConflictChoice::OverwriteAll, ConflictChoice::SkipAll);
    }

    #[test]
    fn test_non_interactive_prompt_service_constructors() {
        let skip_service = NonInteractivePromptService::skip_all();
        let overwrite_service = NonInteractivePromptService::overwrite_all();
        let rename_service = NonInteractivePromptService::rename_all();

        // Test that they return the expected choices
        assert_eq!(
            skip_service
                .prompt_conflict("test.mdc", "src", "dest")
                .unwrap(),
            ConflictChoice::SkipAll
        );
        assert_eq!(
            overwrite_service
                .prompt_conflict("test.mdc", "src", "dest")
                .unwrap(),
            ConflictChoice::OverwriteAll
        );
        assert_eq!(
            rename_service
                .prompt_conflict("test.mdc", "src", "dest")
                .unwrap(),
            ConflictChoice::RenameAll
        );

        // All should return false for can_prompt
        assert!(!skip_service.can_prompt());
        assert!(!overwrite_service.can_prompt());
        assert!(!rename_service.can_prompt());
    }

    #[test]
    fn test_non_interactive_service_with_custom_choice() {
        let service = NonInteractivePromptService::new(ConflictChoice::Rename);

        assert_eq!(
            service.prompt_conflict("test.mdc", "src", "dest").unwrap(),
            ConflictChoice::Rename
        );
        assert!(!service.can_prompt());
    }

    #[test]
    fn test_mock_prompt_service_empty_responses() {
        let service = MockPromptService::new(vec![]);

        // Should return Cancel when no responses available
        let choice = service.prompt_conflict("test.mdc", "src", "dest").unwrap();
        assert_eq!(choice, ConflictChoice::Cancel);
        assert_eq!(service.call_count(), 1);
    }

    #[test]
    fn test_mock_prompt_service_single_response() {
        let service = MockPromptService::new(vec![ConflictChoice::Overwrite]);

        // First call should return the response
        let choice1 = service
            .prompt_conflict("test1.mdc", "src1", "dest1")
            .unwrap();
        assert_eq!(choice1, ConflictChoice::Overwrite);
        assert_eq!(service.call_count(), 1);

        // Second call should return Cancel
        let choice2 = service
            .prompt_conflict("test2.mdc", "src2", "dest2")
            .unwrap();
        assert_eq!(choice2, ConflictChoice::Cancel);
        assert_eq!(service.call_count(), 2);
    }
}
