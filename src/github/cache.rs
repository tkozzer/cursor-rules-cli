//! Persistent cache implementation for GitHub repository data.
//!
//! This module provides disk-based caching for GitHub tree and blob data to minimize
//! API calls and enable offline browsing. Uses XDG-compliant cache directories and
//! HTTP caching semantics with ETag support.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use std::fs;
use std::path::PathBuf;

use super::{RepoLocator, RepoNode};

/// Cache expiration time (24 hours)
const CACHE_EXPIRY_HOURS: u64 = 24;

/// Cache metadata stored in meta.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// When the cache was last fetched
    pub fetched_at: DateTime<Utc>,
    /// ETag from GitHub response for conditional requests
    pub etag: Option<String>,
    /// Last-Modified header from GitHub response
    pub last_modified: Option<String>,
    /// Repository information
    pub owner: String,
    pub repo: String,
    pub branch: String,
}

/// Persistent cache trait for abstracting cache operations
pub trait PersistentCache {
    /// Get cached tree data if fresh, otherwise None
    async fn get_tree_cache(
        &self,
        locator: &RepoLocator,
        force_refresh: bool,
    ) -> Result<Option<Vec<RepoNode>>>;

    /// Store tree data in cache with metadata
    async fn store_tree_cache(
        &self,
        locator: &RepoLocator,
        nodes: &[RepoNode],
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Result<()>;

    /// Get cached blob content if exists
    async fn get_blob_cache(&self, content_sha: &str) -> Result<Option<String>>;

    /// Store blob content in cache
    async fn store_blob_cache(&self, content_sha: &str, content: &str) -> Result<()>;

    /// Check if cache is fresh (within expiry time)
    fn is_cache_fresh(&self, locator: &RepoLocator) -> Result<bool>;

    /// Clear cache for a specific repository
    async fn clear_cache(&self, locator: &RepoLocator) -> Result<()>;

    /// List all cached repositories
    fn list_cached_repos(&self) -> Result<Vec<(String, String, DateTime<Utc>)>>;

    /// Get cache metadata for conditional requests
    fn get_metadata(&self, locator: &RepoLocator) -> Result<Option<CacheMetadata>>;
}

/// File system implementation of persistent cache
pub struct FileSystemCache {
    cache_root: PathBuf,
}

impl FileSystemCache {
    /// Create new filesystem cache instance
    pub fn new() -> Result<Self> {
        let cache_root = get_cache_directory()?;
        Ok(Self { cache_root })
    }

    /// Compute SHA-1 hash for cache directory name
    fn compute_cache_key(owner: &str, repo: &str) -> String {
        let mut hasher = Sha1::new();
        hasher.update(format!("{owner}/{repo}").as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Get cache directory for a specific repository
    fn get_repo_cache_dir(&self, locator: &RepoLocator) -> PathBuf {
        let cache_key = Self::compute_cache_key(&locator.owner, &locator.repo);
        self.cache_root.join(cache_key)
    }

    /// Get metadata file path
    fn get_metadata_path(&self, locator: &RepoLocator) -> PathBuf {
        self.get_repo_cache_dir(locator).join("meta.json")
    }

    /// Get tree cache file path
    fn get_tree_cache_path(&self, locator: &RepoLocator) -> PathBuf {
        self.get_repo_cache_dir(locator)
            .join("tree")
            .join("tree.json")
    }

    /// Load cache metadata
    fn load_metadata(&self, locator: &RepoLocator) -> Result<Option<CacheMetadata>> {
        let meta_path = self.get_metadata_path(locator);
        if !meta_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&meta_path)
            .with_context(|| format!("Failed to read metadata from {}", meta_path.display()))?;

        let metadata: CacheMetadata =
            serde_json::from_str(&content).with_context(|| "Failed to parse cache metadata")?;

        Ok(Some(metadata))
    }

    /// Save cache metadata
    fn save_metadata(&self, locator: &RepoLocator, metadata: &CacheMetadata) -> Result<()> {
        let repo_dir = self.get_repo_cache_dir(locator);
        fs::create_dir_all(&repo_dir)
            .with_context(|| format!("Failed to create cache directory {}", repo_dir.display()))?;

        let meta_path = self.get_metadata_path(locator);
        let content = serde_json::to_string_pretty(metadata)
            .with_context(|| "Failed to serialize metadata")?;

        fs::write(&meta_path, content)
            .with_context(|| format!("Failed to write metadata to {}", meta_path.display()))?;

        Ok(())
    }

    /// Acquire exclusive lock on cache directory
    fn acquire_cache_lock(&self, locator: &RepoLocator) -> Result<Option<fs::File>> {
        let repo_dir = self.get_repo_cache_dir(locator);
        fs::create_dir_all(&repo_dir)
            .with_context(|| format!("Failed to create cache directory {}", repo_dir.display()))?;

        let lock_path = repo_dir.join(".lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&lock_path)
            .with_context(|| format!("Failed to open lock file {}", lock_path.display()))?;

        // Try to acquire exclusive lock (non-blocking)
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(file)),
            Err(_) => {
                // Lock is held by another process, return None for graceful fallback
                Ok(None)
            }
        }
    }

    /// Try to load tree cache with detailed error handling
    fn try_load_tree_cache(&self, tree_path: &std::path::Path) -> Result<Vec<RepoNode>> {
        let content = fs::read_to_string(tree_path)
            .with_context(|| format!("Failed to read tree cache from {}", tree_path.display()))?;

        // Check if file is empty
        if content.trim().is_empty() {
            anyhow::bail!("Cache file is empty");
        }

        // Try to parse JSON
        let nodes: Vec<RepoNode> = serde_json::from_str(&content)
            .with_context(|| "Failed to parse cached tree data - file may be corrupted")?;

        // Basic validation - ensure we have at least one valid node structure
        if !nodes.is_empty() {
            // Validate first node has required fields
            if nodes[0].name.is_empty() || nodes[0].path.is_empty() {
                anyhow::bail!("Cache file contains invalid node data");
            }
        }

        Ok(nodes)
    }
}

impl PersistentCache for FileSystemCache {
    async fn get_tree_cache(
        &self,
        locator: &RepoLocator,
        force_refresh: bool,
    ) -> Result<Option<Vec<RepoNode>>> {
        if force_refresh || !self.is_cache_fresh(locator)? {
            return Ok(None);
        }

        let tree_path = self.get_tree_cache_path(locator);
        if !tree_path.exists() {
            return Ok(None);
        }

        // Try to read and parse cache file with error recovery
        match self.try_load_tree_cache(&tree_path) {
            Ok(nodes) => Ok(Some(nodes)),
            Err(e) => {
                // Cache file is corrupted, remove it and let caller re-download
                tracing::warn!(
                    "Corrupted cache file detected at {}: {}. Removing cache directory.",
                    tree_path.display(),
                    e
                );

                // Remove the entire cache directory to ensure clean state
                if let Err(remove_err) = self.clear_cache(locator).await {
                    tracing::warn!("Failed to clear corrupted cache: {}", remove_err);
                    // Fallback: try to remove just the file
                    let _ = fs::remove_file(&tree_path);
                } else {
                    tracing::info!(
                        "Successfully cleared corrupted cache for {}/{}",
                        locator.owner,
                        locator.repo
                    );
                }

                // Return None to trigger fresh download
                Ok(None)
            }
        }
    }

    async fn store_tree_cache(
        &self,
        locator: &RepoLocator,
        nodes: &[RepoNode],
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Result<()> {
        // Try to acquire lock for writing
        let _lock = self.acquire_cache_lock(locator)?;

        // Create directory structure
        let tree_path = self.get_tree_cache_path(locator);
        let tree_dir = tree_path.parent().unwrap();
        fs::create_dir_all(tree_dir).with_context(|| {
            format!(
                "Failed to create tree cache directory {}",
                tree_dir.display()
            )
        })?;

        // Save tree data
        let tree_content =
            serde_json::to_string_pretty(nodes).with_context(|| "Failed to serialize tree data")?;

        fs::write(&tree_path, tree_content)
            .with_context(|| format!("Failed to write tree cache to {}", tree_path.display()))?;

        // Save metadata
        let metadata = CacheMetadata {
            fetched_at: Utc::now(),
            etag,
            last_modified,
            owner: locator.owner.clone(),
            repo: locator.repo.clone(),
            branch: locator.branch.clone(),
        };

        self.save_metadata(locator, &metadata)?;

        Ok(())
    }

    async fn get_blob_cache(&self, content_sha: &str) -> Result<Option<String>> {
        // For blob cache, we need to search across all repo caches
        // This is a simplified implementation - in practice, we'd need better indexing
        let cache_dirs = fs::read_dir(&self.cache_root)
            .with_context(|| "Failed to read cache root directory")?;

        for entry in cache_dirs {
            let entry = entry?;
            let blobs_dir = entry.path().join("blobs");
            let blob_path = blobs_dir.join(format!("{content_sha}.mdc"));

            if blob_path.exists() {
                let content = fs::read_to_string(&blob_path).with_context(|| {
                    format!("Failed to read blob cache from {}", blob_path.display())
                })?;
                return Ok(Some(content));
            }
        }

        Ok(None)
    }

    async fn store_blob_cache(&self, content_sha: &str, content: &str) -> Result<()> {
        // For blob storage, we'll store in the first available repo cache
        // This is simplified - a better implementation would track which repo the blob belongs to
        let cache_dirs = fs::read_dir(&self.cache_root)
            .with_context(|| "Failed to read cache root directory")?;

        for entry in cache_dirs {
            let entry = entry?;
            let blobs_dir = entry.path().join("blobs");

            if blobs_dir.exists() || blobs_dir.parent().is_some_and(|p| p.exists()) {
                fs::create_dir_all(&blobs_dir).with_context(|| {
                    format!("Failed to create blobs directory {}", blobs_dir.display())
                })?;

                let blob_path = blobs_dir.join(format!("{content_sha}.mdc"));
                fs::write(&blob_path, content).with_context(|| {
                    format!("Failed to write blob cache to {}", blob_path.display())
                })?;

                return Ok(());
            }
        }

        // If no existing cache directories, skip blob caching
        // This will be handled better when we track repo context for blobs
        Ok(())
    }

    fn is_cache_fresh(&self, locator: &RepoLocator) -> Result<bool> {
        let metadata = match self.load_metadata(locator)? {
            Some(meta) => meta,
            None => return Ok(false),
        };

        let now = Utc::now();
        let expiry_time = metadata.fetched_at + chrono::Duration::hours(CACHE_EXPIRY_HOURS as i64);

        Ok(now < expiry_time)
    }

    async fn clear_cache(&self, locator: &RepoLocator) -> Result<()> {
        let repo_dir = self.get_repo_cache_dir(locator);
        if repo_dir.exists() {
            fs::remove_dir_all(&repo_dir).with_context(|| {
                format!("Failed to remove cache directory {}", repo_dir.display())
            })?;
        }
        Ok(())
    }

    fn list_cached_repos(&self) -> Result<Vec<(String, String, DateTime<Utc>)>> {
        let mut repos = Vec::new();

        if !self.cache_root.exists() {
            return Ok(repos);
        }

        let cache_dirs = fs::read_dir(&self.cache_root)
            .with_context(|| "Failed to read cache root directory")?;

        for entry in cache_dirs {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let meta_path = entry.path().join("meta.json");
                if let Ok(content) = fs::read_to_string(&meta_path) {
                    if let Ok(metadata) = serde_json::from_str::<CacheMetadata>(&content) {
                        repos.push((metadata.owner, metadata.repo, metadata.fetched_at));
                    }
                }
            }
        }

        Ok(repos)
    }

    fn get_metadata(&self, locator: &RepoLocator) -> Result<Option<CacheMetadata>> {
        self.load_metadata(locator)
    }
}

/// Get XDG-compliant cache directory
pub fn get_cache_directory() -> Result<PathBuf> {
    let cache_dir =
        dirs::cache_dir().ok_or_else(|| anyhow::anyhow!("Could not determine cache directory"))?;

    let app_cache_dir = cache_dir.join("cursor-rules-cli");

    if !app_cache_dir.exists() {
        fs::create_dir_all(&app_cache_dir).with_context(|| {
            format!(
                "Failed to create cache directory {}",
                app_cache_dir.display()
            )
        })?;
    }

    Ok(app_cache_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_cache() -> (FileSystemCache, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let cache = FileSystemCache {
            cache_root: temp_dir.path().to_path_buf(),
        };
        (cache, temp_dir)
    }

    fn create_test_locator() -> RepoLocator {
        RepoLocator {
            owner: "test".to_string(),
            repo: "repo".to_string(),
            branch: "main".to_string(),
        }
    }

    #[test]
    fn compute_cache_key_sha1() {
        let key = FileSystemCache::compute_cache_key("owner", "repo");
        assert_eq!(key.len(), 40); // SHA-1 produces 40 character hex string
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));

        // Same input should produce same key
        let key2 = FileSystemCache::compute_cache_key("owner", "repo");
        assert_eq!(key, key2);

        // Different input should produce different key
        let key3 = FileSystemCache::compute_cache_key("owner", "other");
        assert_ne!(key, key3);
    }

    #[test]
    fn cache_directory_creation() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        let repo_dir = cache.get_repo_cache_dir(&locator);
        assert!(repo_dir
            .to_string_lossy()
            .contains(&FileSystemCache::compute_cache_key("test", "repo")));
    }

    #[tokio::test]
    async fn meta_json_serialization() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        let metadata = CacheMetadata {
            fetched_at: Utc::now(),
            etag: Some("test-etag".to_string()),
            last_modified: Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string()),
            owner: "test".to_string(),
            repo: "repo".to_string(),
            branch: "main".to_string(),
        };

        // Save metadata
        cache.save_metadata(&locator, &metadata).unwrap();

        // Load metadata
        let loaded = cache.load_metadata(&locator).unwrap().unwrap();
        assert_eq!(loaded.etag, metadata.etag);
        assert_eq!(loaded.last_modified, metadata.last_modified);
        assert_eq!(loaded.owner, metadata.owner);
        assert_eq!(loaded.repo, metadata.repo);
        assert_eq!(loaded.branch, metadata.branch);
    }

    #[tokio::test]
    async fn cache_expiration_logic() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // Fresh cache
        let metadata = CacheMetadata {
            fetched_at: Utc::now(),
            etag: None,
            last_modified: None,
            owner: "test".to_string(),
            repo: "repo".to_string(),
            branch: "main".to_string(),
        };
        cache.save_metadata(&locator, &metadata).unwrap();
        assert!(cache.is_cache_fresh(&locator).unwrap());

        // Stale cache
        let old_metadata = CacheMetadata {
            fetched_at: Utc::now() - chrono::Duration::hours(25), // 25 hours ago
            etag: None,
            last_modified: None,
            owner: "test".to_string(),
            repo: "repo".to_string(),
            branch: "main".to_string(),
        };
        cache.save_metadata(&locator, &old_metadata).unwrap();
        assert!(!cache.is_cache_fresh(&locator).unwrap());
    }

    #[tokio::test]
    async fn file_locking_concurrent_access() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // First lock should succeed
        let lock1 = cache.acquire_cache_lock(&locator).unwrap();
        assert!(lock1.is_some());

        // Second lock should fail (return None)
        let lock2 = cache.acquire_cache_lock(&locator).unwrap();
        assert!(lock2.is_none());

        // After dropping first lock, should be able to acquire again
        drop(lock1);
        let lock3 = cache.acquire_cache_lock(&locator).unwrap();
        assert!(lock3.is_some());
    }

    #[tokio::test]
    async fn cache_miss_and_storage() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // Cache miss
        let result = cache.get_tree_cache(&locator, false).await.unwrap();
        assert!(result.is_none());

        // Store in cache
        let nodes = vec![RepoNode {
            name: "test.mdc".to_string(),
            path: "test.mdc".to_string(),
            kind: super::super::NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        }];

        cache
            .store_tree_cache(&locator, &nodes, Some("test-etag".to_string()), None)
            .await
            .unwrap();

        // Cache hit
        let result = cache.get_tree_cache(&locator, false).await.unwrap();
        assert!(result.is_some());
        let cached_nodes = result.unwrap();
        assert_eq!(cached_nodes.len(), 1);
        assert_eq!(cached_nodes[0].name, "test.mdc");
    }

    #[tokio::test]
    async fn force_refresh_bypasses_cache() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // Store in cache
        let nodes = vec![RepoNode {
            name: "test.mdc".to_string(),
            path: "test.mdc".to_string(),
            kind: super::super::NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        }];

        cache
            .store_tree_cache(&locator, &nodes, None, None)
            .await
            .unwrap();

        // Normal access should hit cache
        let result = cache.get_tree_cache(&locator, false).await.unwrap();
        assert!(result.is_some());

        // Force refresh should bypass cache
        let result = cache.get_tree_cache(&locator, true).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn clear_cache_removes_directory() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // Store something in cache
        let nodes = vec![RepoNode {
            name: "test.mdc".to_string(),
            path: "test.mdc".to_string(),
            kind: super::super::NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        }];

        cache
            .store_tree_cache(&locator, &nodes, None, None)
            .await
            .unwrap();

        // Verify cache exists
        let repo_dir = cache.get_repo_cache_dir(&locator);
        assert!(repo_dir.exists());

        // Clear cache
        cache.clear_cache(&locator).await.unwrap();

        // Verify cache is removed
        assert!(!repo_dir.exists());
    }

    #[tokio::test]
    async fn list_cached_repos_works() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // Create a cache entry
        let metadata = CacheMetadata {
            fetched_at: Utc::now(),
            etag: Some("test-etag".to_string()),
            last_modified: None,
            owner: locator.owner.clone(),
            repo: locator.repo.clone(),
            branch: locator.branch.clone(),
        };

        cache.save_metadata(&locator, &metadata).unwrap();

        let repos = cache.list_cached_repos().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].0, locator.owner);
        assert_eq!(repos[0].1, locator.repo);
    }

    #[tokio::test]
    async fn corrupted_cache_auto_recovery() {
        let _ = tracing_subscriber::fmt::try_init();
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // Create a corrupted cache file
        let tree_path = cache.get_tree_cache_path(&locator);
        fs::create_dir_all(tree_path.parent().unwrap()).unwrap();
        fs::write(&tree_path, "invalid json").unwrap();

        // Verify the corrupted file exists
        assert!(tree_path.exists());

        // Try to load cache - should detect corruption and return None (triggering fresh download)
        let result = cache.get_tree_cache(&locator, false).await.unwrap();
        assert!(
            result.is_none(),
            "Corrupted cache should return None to trigger fresh download"
        );

        // Test detection works by trying to load the file directly
        let direct_result = cache.try_load_tree_cache(&tree_path);
        assert!(
            direct_result.is_err(),
            "Direct load of corrupted file should fail"
        );
        assert!(
            direct_result.unwrap_err().to_string().contains("corrupted"),
            "Error should mention corruption"
        );
    }

    #[tokio::test]
    async fn empty_cache_file_recovery() {
        let _ = tracing_subscriber::fmt::try_init();
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // Create an empty cache file
        let tree_path = cache.get_tree_cache_path(&locator);
        fs::create_dir_all(tree_path.parent().unwrap()).unwrap();
        fs::write(&tree_path, "").unwrap();

        // Verify the empty file exists
        assert!(tree_path.exists());

        // Try to load cache - should detect empty file and return None (triggering fresh download)
        let result = cache.get_tree_cache(&locator, false).await.unwrap();
        assert!(
            result.is_none(),
            "Empty cache should return None to trigger fresh download"
        );

        // Test detection works by trying to load the file directly
        let direct_result = cache.try_load_tree_cache(&tree_path);
        assert!(
            direct_result.is_err(),
            "Direct load of empty file should fail"
        );
        assert!(
            direct_result.unwrap_err().to_string().contains("empty"),
            "Error should mention empty file"
        );
    }

    #[tokio::test]
    async fn blob_cache_operations_enhanced() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();
        let content_sha = "abc123";
        let content = "test blob content";

        // First create a repo cache directory by storing some tree data
        let nodes = vec![RepoNode {
            name: "test.mdc".to_string(),
            path: "test.mdc".to_string(),
            kind: crate::github::NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        }];
        cache
            .store_tree_cache(&locator, &nodes, None, None)
            .await
            .unwrap();

        // Cache should be empty initially
        let result = cache.get_blob_cache(content_sha).await.unwrap();
        assert!(result.is_none());

        // Store content in cache
        cache.store_blob_cache(content_sha, content).await.unwrap();

        // Should be able to retrieve it
        let result = cache.get_blob_cache(content_sha).await.unwrap();
        assert_eq!(result.unwrap(), content);
    }

    #[tokio::test]
    async fn metadata_persistence_with_etag() {
        let (cache, _temp_dir) = create_test_cache();
        let locator = create_test_locator();

        // Store tree with ETag
        let nodes = vec![RepoNode {
            name: "test.mdc".to_string(),
            path: "test.mdc".to_string(),
            kind: crate::github::NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        }];

        let etag = Some("test-etag-123".to_string());
        let last_modified = Some("Wed, 18 Jun 2025 21:00:00 GMT".to_string());

        cache
            .store_tree_cache(&locator, &nodes, etag.clone(), last_modified.clone())
            .await
            .unwrap();

        // Retrieve metadata
        let metadata = cache.get_metadata(&locator).unwrap().unwrap();
        assert_eq!(metadata.etag, etag);
        assert_eq!(metadata.last_modified, last_modified);
        assert_eq!(metadata.owner, locator.owner);
        assert_eq!(metadata.repo, locator.repo);
    }

    #[test]
    fn try_load_tree_cache_validation() {
        let (cache, temp_dir) = create_test_cache();

        // Create a valid cache file
        let valid_nodes = vec![RepoNode {
            name: "test.mdc".to_string(),
            path: "test.mdc".to_string(),
            kind: crate::github::NodeKind::RuleFile,
            children: None,
            manifest_count: None,
        }];

        let valid_path = temp_dir.path().join("valid.json");
        let valid_content = serde_json::to_string_pretty(&valid_nodes).unwrap();
        fs::write(&valid_path, valid_content).unwrap();

        // Should load successfully
        let result = cache.try_load_tree_cache(&valid_path);
        assert!(result.is_ok());
        let loaded_nodes = result.unwrap();
        assert_eq!(loaded_nodes.len(), 1);
        assert_eq!(loaded_nodes[0].name, "test.mdc");

        // Test invalid JSON
        let invalid_path = temp_dir.path().join("invalid.json");
        fs::write(&invalid_path, "invalid json").unwrap();
        let result = cache.try_load_tree_cache(&invalid_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("corrupted"));

        // Test empty file
        let empty_path = temp_dir.path().join("empty.json");
        fs::write(&empty_path, "").unwrap();
        let result = cache.try_load_tree_cache(&empty_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }
}
