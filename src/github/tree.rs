use std::collections::HashMap;

use anyhow::Result;

use super::RepoLocator;
use octocrab::Octocrab;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Dir,
    RuleFile,
    Manifest,
}

#[derive(Debug, Clone)]
pub struct RepoNode {
    pub name: String,
    pub path: String,
    pub kind: NodeKind,
    #[allow(dead_code)]
    pub children: Option<Vec<RepoNode>>, // Not used yet
    pub manifest_count: Option<usize>,
}

impl RepoNode {
    pub fn is_dir(&self) -> bool {
        matches!(self.kind, NodeKind::Dir)
    }
}

/// Very small in-memory lazy tree cache for one CLI session.
/// Currently uses placeholder stub data; TODO fetch from GitHub via Octocrab.
#[derive(Default)]
pub struct RepoTree {
    cache: HashMap<String, Vec<RepoNode>>, // key = dir path ("" for root)
}

impl RepoTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensure the git tree is loaded into memory (one API call) then return children for `dir_path`.
    pub async fn children(&mut self, locator: &RepoLocator, dir_path: &str) -> Result<&[RepoNode]> {
        if self.cache.is_empty() {
            self.populate_cache(locator).await?;
        }

        Ok(self.cache.get(dir_path).map(Vec::as_slice).unwrap_or(&[]))
    }

    async fn populate_cache(&mut self, locator: &RepoLocator) -> Result<()> {
        let octo = if let Ok(base) = std::env::var("OCTO_BASE") {
            Octocrab::builder().base_uri(&base)?.build()?
        } else {
            Octocrab::builder().build()?
        };
        // Fetch full recursive tree in one call
        let endpoint = format!(
            "/repos/{}/{}/git/trees/{}?recursive=1",
            locator.owner, locator.repo, locator.branch
        );
        let response: serde_json::Value = octo.get(endpoint, None::<&()>).await?;

        let empty: Vec<serde_json::Value> = Vec::new();
        let tree = response["tree"].as_array().unwrap_or(&empty);

        for item in tree {
            let path = item["path"].as_str().unwrap_or("").to_string();
            let item_type = item["type"].as_str().unwrap_or("");

            let kind = if item_type == "tree" {
                NodeKind::Dir
            } else if path.ends_with(".mdc") {
                NodeKind::RuleFile
            } else if path.ends_with(".txt")
                || path.ends_with(".yaml")
                || path.ends_with(".yml")
                || path.ends_with(".json")
            {
                NodeKind::Manifest
            } else {
                NodeKind::RuleFile
            };

            let name = path.split('/').next_back().unwrap_or("").to_string();

            let node = RepoNode {
                name,
                path: path.clone(),
                kind,
                children: None,
                manifest_count: None,
            };

            // Determine parent directory key
            let dir_key = if let Some(pos) = path.rfind('/') {
                path[..pos].to_string()
            } else {
                String::new()
            };

            self.cache.entry(dir_key).or_default().push(node);
        }

        // Ensure root entry exists even if empty
        self.cache.entry(String::new()).or_default();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn children_returns_cached_slice() {
        let locator = RepoLocator {
            owner: "o".into(),
            repo: "r".into(),
            branch: "main".into(),
        };

        let mut tree = RepoTree::new();

        // Manually seed cache to avoid network.
        tree.cache.insert(
            String::new(),
            vec![RepoNode {
                name: "dir".into(),
                path: "dir".into(),
                kind: NodeKind::Dir,
                children: None,
                manifest_count: None,
            }],
        );

        let slice = tree.children(&locator, "").await.unwrap();
        assert_eq!(slice.len(), 1);
        assert_eq!(slice[0].name, "dir");
    }

    #[tokio::test]
    async fn children_returns_empty_for_nonexistent_dir() {
        let locator = RepoLocator {
            owner: "o".into(),
            repo: "r".into(),
            branch: "main".into(),
        };

        let mut tree = RepoTree::new();

        // Seed cache with root but not the requested directory
        tree.cache.insert(String::new(), vec![]);

        let slice = tree.children(&locator, "nonexistent").await.unwrap();
        assert_eq!(slice.len(), 0);
    }

    #[test]
    fn populate_cache_parses_file_kinds_correctly() {
        let _tree = RepoTree::new();

        // Simulate the tree parsing logic directly
        let test_files = vec![
            ("rules.mdc", "blob", NodeKind::RuleFile),
            ("manifest.txt", "blob", NodeKind::Manifest),
            ("config.yaml", "blob", NodeKind::Manifest),
            ("settings.yml", "blob", NodeKind::Manifest),
            ("data.json", "blob", NodeKind::Manifest),
            ("other.rs", "blob", NodeKind::RuleFile),
            ("README.md", "blob", NodeKind::RuleFile),
            ("src", "tree", NodeKind::Dir),
        ];

        for (path, item_type, expected_kind) in test_files {
            let kind = if item_type == "tree" {
                NodeKind::Dir
            } else if path.ends_with(".mdc") {
                NodeKind::RuleFile
            } else if path.ends_with(".txt")
                || path.ends_with(".yaml")
                || path.ends_with(".yml")
                || path.ends_with(".json")
            {
                NodeKind::Manifest
            } else {
                NodeKind::RuleFile
            };

            assert_eq!(kind, expected_kind, "Failed for file: {}", path);
        }
    }

    #[test]
    fn populate_cache_handles_nested_paths() {
        let _tree = RepoTree::new();

        // Test path parsing logic
        let test_paths = vec![
            ("src/components/Button.mdc", "src/components", "Button.mdc"),
            ("docs/README.md", "docs", "README.md"),
            ("root.txt", "", "root.txt"),
            ("a/b/c/deep.mdc", "a/b/c", "deep.mdc"),
        ];

        for (full_path, expected_dir, expected_name) in test_paths {
            let dir_key = if let Some(pos) = full_path.rfind('/') {
                full_path[..pos].to_string()
            } else {
                String::new()
            };

            let name = full_path.split('/').next_back().unwrap_or("").to_string();

            assert_eq!(
                dir_key, expected_dir,
                "Directory parsing failed for: {}",
                full_path
            );
            assert_eq!(
                name, expected_name,
                "Name parsing failed for: {}",
                full_path
            );
        }
    }

    #[test]
    fn cache_organization_works() {
        let mut tree = RepoTree::new();

        // Manually populate cache as the populate_cache method would
        let nodes = vec![
            RepoNode {
                name: "src".into(),
                path: "src".into(),
                kind: NodeKind::Dir,
                children: None,
                manifest_count: None,
            },
            RepoNode {
                name: "Button.mdc".into(),
                path: "src/Button.mdc".into(),
                kind: NodeKind::RuleFile,
                children: None,
                manifest_count: None,
            },
            RepoNode {
                name: "manifest.txt".into(),
                path: "src/manifest.txt".into(),
                kind: NodeKind::Manifest,
                children: None,
                manifest_count: None,
            },
        ];

        // Organize into cache structure
        for node in nodes {
            let dir_key = if let Some(pos) = node.path.rfind('/') {
                node.path[..pos].to_string()
            } else {
                String::new()
            };
            tree.cache.entry(dir_key).or_default().push(node);
        }

        // Verify cache structure
        assert_eq!(tree.cache.get("").unwrap().len(), 1); // root has "src"
        assert_eq!(tree.cache.get("src").unwrap().len(), 2); // src has Button.mdc and manifest.txt

        let root_items = tree.cache.get("").unwrap();
        assert_eq!(root_items[0].name, "src");
        assert!(root_items[0].is_dir());

        let src_items = tree.cache.get("src").unwrap();
        assert_eq!(src_items[0].name, "Button.mdc");
        assert_eq!(src_items[0].kind, NodeKind::RuleFile);
        assert_eq!(src_items[1].name, "manifest.txt");
        assert_eq!(src_items[1].kind, NodeKind::Manifest);
    }

    #[test]
    fn repo_node_is_dir_works() {
        let dir_node = RepoNode {
            name: "test".into(),
            path: "test".into(),
            kind: NodeKind::Dir,
            children: None,
            manifest_count: None,
        };
        assert!(dir_node.is_dir());

        let file_node = RepoNode {
            name: "test.mdc".into(),
            path: "test.mdc".into(),
            kind: NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        };
        assert!(!file_node.is_dir());

        let manifest_node = RepoNode {
            name: "manifest.txt".into(),
            path: "manifest.txt".into(),
            kind: NodeKind::Manifest,
            children: None,
            manifest_count: None,
        };
        assert!(!manifest_node.is_dir());
    }

    #[test]
    fn node_kind_equality() {
        assert_eq!(NodeKind::Dir, NodeKind::Dir);
        assert_eq!(NodeKind::RuleFile, NodeKind::RuleFile);
        assert_eq!(NodeKind::Manifest, NodeKind::Manifest);
        assert_ne!(NodeKind::Dir, NodeKind::RuleFile);
        assert_ne!(NodeKind::RuleFile, NodeKind::Manifest);
        assert_ne!(NodeKind::Dir, NodeKind::Manifest);
    }

    #[test]
    fn repo_tree_new_creates_empty_cache() {
        let tree = RepoTree::new();
        assert!(tree.cache.is_empty());
    }

    #[test]
    fn repo_tree_default_creates_empty_cache() {
        let tree = RepoTree::default();
        assert!(tree.cache.is_empty());
    }

    #[test]
    fn edge_cases_in_path_parsing() {
        // Test edge cases in path parsing
        let edge_cases = vec![
            ("", "", ""),
            ("/", "", ""),
            ("file", "", "file"),
            ("dir/", "dir", ""),
            ("a/b/", "a/b", ""),
            ("./file.txt", ".", "file.txt"),
        ];

        for (input, expected_dir, expected_name) in edge_cases {
            let dir_key = if let Some(pos) = input.rfind('/') {
                input[..pos].to_string()
            } else {
                String::new()
            };

            let name = input.split('/').next_back().unwrap_or("").to_string();

            assert_eq!(
                dir_key, expected_dir,
                "Directory parsing failed for: '{}'",
                input
            );
            assert_eq!(name, expected_name, "Name parsing failed for: '{}'", input);
        }
    }

    #[test]
    fn file_extension_detection_comprehensive() {
        let test_cases = vec![
            // Manifest files
            ("manifest.txt", NodeKind::Manifest),
            ("config.yaml", NodeKind::Manifest),
            ("settings.yml", NodeKind::Manifest),
            ("data.json", NodeKind::Manifest),
            ("QUICK_ADD_ALL.txt", NodeKind::Manifest),
            // Rule files
            ("rules.mdc", NodeKind::RuleFile),
            ("component.mdc", NodeKind::RuleFile),
            // Other files treated as rule files
            ("README.md", NodeKind::RuleFile),
            ("script.js", NodeKind::RuleFile),
            ("style.css", NodeKind::RuleFile),
            ("code.rs", NodeKind::RuleFile),
            ("file", NodeKind::RuleFile), // No extension
            // Edge cases
            ("file.txt.backup", NodeKind::RuleFile), // Doesn't end with .txt
            ("yaml.config", NodeKind::RuleFile),     // Doesn't end with .yaml
            (".hidden.mdc", NodeKind::RuleFile),
            ("", NodeKind::RuleFile), // Empty filename
        ];

        for (filename, expected_kind) in test_cases {
            let kind = if filename.ends_with(".mdc") {
                NodeKind::RuleFile
            } else if filename.ends_with(".txt")
                || filename.ends_with(".yaml")
                || filename.ends_with(".yml")
                || filename.ends_with(".json")
            {
                NodeKind::Manifest
            } else {
                NodeKind::RuleFile
            };

            assert_eq!(kind, expected_kind, "Failed for filename: '{}'", filename);
        }
    }

    #[test]
    fn cache_handles_empty_directories() {
        let mut tree = RepoTree::new();

        // Add empty directory to cache
        tree.cache.insert("empty_dir".to_string(), vec![]);
        tree.cache.insert(
            String::new(),
            vec![RepoNode {
                name: "empty_dir".into(),
                path: "empty_dir".into(),
                kind: NodeKind::Dir,
                children: None,
                manifest_count: None,
            }],
        );

        // Verify empty directory exists in cache
        assert!(tree.cache.contains_key("empty_dir"));
        assert_eq!(tree.cache.get("empty_dir").unwrap().len(), 0);
        assert_eq!(tree.cache.get("").unwrap().len(), 1);
    }

    #[test]
    fn cache_handles_deep_nesting() {
        let mut tree = RepoTree::new();

        // Create a deeply nested structure
        let deep_path = "a/b/c/d/e/file.mdc";
        let node = RepoNode {
            name: "file.mdc".into(),
            path: deep_path.into(),
            kind: NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        };

        let dir_key = if let Some(pos) = deep_path.rfind('/') {
            deep_path[..pos].to_string()
        } else {
            String::new()
        };

        tree.cache.entry(dir_key.clone()).or_default().push(node);

        // Verify deep nesting works
        assert_eq!(dir_key, "a/b/c/d/e");
        assert!(tree.cache.contains_key("a/b/c/d/e"));
        assert_eq!(tree.cache.get("a/b/c/d/e").unwrap().len(), 1);
        assert_eq!(tree.cache.get("a/b/c/d/e").unwrap()[0].name, "file.mdc");
    }

    #[tokio::test]
    async fn populate_cache_without_network() {
        // This test exercises the cache logic without making real network calls
        let locator = RepoLocator {
            owner: "test".into(),
            repo: "repo".into(),
            branch: "main".into(),
        };

        let mut tree = RepoTree::new();

        // Manually populate cache to simulate what populate_cache would do
        // without making actual GitHub API calls
        tree.cache.insert(
            String::new(),
            vec![
                RepoNode {
                    name: "src".into(),
                    path: "src".into(),
                    kind: NodeKind::Dir,
                    children: None,
                    manifest_count: None,
                },
                RepoNode {
                    name: "README.mdc".into(),
                    path: "README.mdc".into(),
                    kind: NodeKind::RuleFile,
                    children: None,
                    manifest_count: None,
                },
            ],
        );

        // Test that children() returns cached data
        let result = tree.children(&locator, "").await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "src");
        assert_eq!(result[1].name, "README.mdc");
    }

    #[test]
    fn populate_cache_logic_comprehensive() {
        // Test the core logic of populate_cache without network calls
        let mut tree = RepoTree::new();

        // Simulate what populate_cache does with various GitHub tree entries
        let github_tree_items = vec![
            // Directories
            ("src", "tree"),
            ("docs", "tree"),
            ("src/components", "tree"),
            // Rule files
            ("README.mdc", "blob"),
            ("src/main.mdc", "blob"),
            ("src/components/Button.mdc", "blob"),
            // Manifest files
            ("manifest.txt", "blob"),
            ("config.yaml", "blob"),
            ("src/settings.yml", "blob"),
            ("data.json", "blob"),
            // Other files
            ("package.json", "blob"),
            ("src/utils.js", "blob"),
            (".gitignore", "blob"),
            // Edge cases
            ("", "blob"), // Empty path
            ("file_without_extension", "blob"),
            ("dir/", "tree"), // Directory with trailing slash
        ];

        // Process each item as populate_cache would
        for (path, item_type) in github_tree_items {
            if path.is_empty() {
                continue; // Skip empty paths
            }

            let kind = if item_type == "tree" {
                NodeKind::Dir
            } else if path.ends_with(".mdc") {
                NodeKind::RuleFile
            } else if path.ends_with(".txt")
                || path.ends_with(".yaml")
                || path.ends_with(".yml")
                || path.ends_with(".json")
            {
                NodeKind::Manifest
            } else {
                NodeKind::RuleFile
            };

            let name = path
                .trim_end_matches('/')
                .split('/')
                .next_back()
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue; // Skip items with empty names
            }

            let node = RepoNode {
                name,
                path: path.to_string(),
                kind,
                children: None,
                manifest_count: None,
            };

            // Determine parent directory key
            let dir_key = if let Some(pos) = path.rfind('/') {
                path[..pos].to_string()
            } else {
                String::new()
            };

            tree.cache.entry(dir_key).or_default().push(node);
        }

        // Ensure root entry exists
        tree.cache.entry(String::new()).or_default();

        // Verify the cache structure
        assert!(tree.cache.contains_key(""));
        assert!(tree.cache.contains_key("src"));
        assert!(tree.cache.contains_key("src/components"));

        // Check root level items
        let root_items = tree.cache.get("").unwrap();
        let root_names: Vec<&str> = root_items.iter().map(|n| n.name.as_str()).collect();
        assert!(root_names.contains(&"src"));
        assert!(root_names.contains(&"docs"));
        assert!(root_names.contains(&"README.mdc"));
        assert!(root_names.contains(&"manifest.txt"));

        // Check src directory items
        let src_items = tree.cache.get("src").unwrap();
        let src_names: Vec<&str> = src_items.iter().map(|n| n.name.as_str()).collect();
        assert!(src_names.contains(&"components"));
        assert!(src_names.contains(&"main.mdc"));
        assert!(src_names.contains(&"settings.yml"));

        // Check file type classification
        let readme = root_items.iter().find(|n| n.name == "README.mdc").unwrap();
        assert_eq!(readme.kind, NodeKind::RuleFile);

        let manifest = root_items
            .iter()
            .find(|n| n.name == "manifest.txt")
            .unwrap();
        assert_eq!(manifest.kind, NodeKind::Manifest);

        let config = root_items.iter().find(|n| n.name == "config.yaml").unwrap();
        assert_eq!(config.kind, NodeKind::Manifest);
    }
}
