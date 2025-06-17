use std::{fs, io, path::PathBuf, process::Command};

use anyhow::Context;
use inquire::Text;
use is_terminal::IsTerminal;
use regex::Regex;
use thiserror::Error;
use tracing::{debug, instrument};

/// Regex that matches a valid GitHub repository name.
/// See: https://docs.github.com/en/repositories/creating-and-managing-repositories/about-repositories#repository-name-limitations
const REPO_NAME_REGEX: &str = r"^[A-Za-z0-9._-]+$";

/// Same rules apply to owner/user logins.
const LOGIN_REGEX: &str = REPO_NAME_REGEX;

/// Resulting locator that uniquely identifies a GitHub repository and branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoLocator {
    pub owner: String,
    pub repo: String,
    pub branch: String,
}

/// All possible errors that can occur while resolving a [`RepoLocator`].
#[derive(Debug, Error)]
pub enum RepoDiscoveryError {
    /// Automatic owner inference failed in non-interactive mode.
    #[error("Unable to determine GitHub owner automatically; set one with `git config --global user.username <login>`")]
    OwnerNotFound,

    /// Interactive prompt was displayed but the user cancelled it (e.g. pressed *Esc* or sent EOF).
    #[error("Owner prompt was cancelled by the user")]
    OwnerPromptCancelled,

    /// The requested repository does not exist or the authenticated user does not have access to it.
    #[error("Repository '{owner}/{repo}' not found or is private")]
    RepoNotFound { owner: String, repo: String },

    /// Any other network-related error surfaced by the GitHub API.
    #[error("Network error: {0}")]
    NetworkError(#[from] anyhow::Error),
}

/// Construct an `Octocrab` instance, injecting `OCTO_BASE` when running in tests.
fn build_octocrab(token: Option<&str>) -> Result<octocrab::Octocrab, RepoDiscoveryError> {
    use octocrab::Octocrab;
    let mut builder = Octocrab::builder();
    if let Ok(base) = std::env::var("OCTO_BASE") {
        builder = builder
            .base_uri(&base)
            .map_err(|e| RepoDiscoveryError::NetworkError(e.into()))?;
    }
    if let Some(tok) = token {
        builder
            .personal_token(tok.to_string())
            .build()
            .map_err(|e| RepoDiscoveryError::NetworkError(e.into()))
    } else {
        builder
            .build()
            .map_err(|e| RepoDiscoveryError::NetworkError(e.into()))
    }
}

/// Resolve the GitHub repository coordinates (owner/repo@branch) by applying CLI overrides,
/// local Git configuration, interactive prompt (TTY only) and finally remote existence check.
///
/// * `owner_flag` – value from `--owner` CLI flag.
/// * `repo_flag` – value from `--repo` CLI flag (default = `cursor-rules`).
/// * `branch_flag` – value from `--branch` CLI flag (default = `main`).
/// * `token` – optional GitHub Personal Access Token.
#[instrument(level = "debug", skip(token))]
pub async fn resolve_repo(
    owner_flag: Option<String>,
    repo_flag: Option<String>,
    branch_flag: Option<String>,
    token: Option<String>,
) -> Result<RepoLocator, RepoDiscoveryError> {
    // 1. Owner resolution (multi-step)
    let owner = if let Some(owner) = owner_flag {
        debug!(%owner, "Using --owner override");
        owner
    } else if let Some(o) = git_config_username() {
        debug!(owner=%o, "Found user.username in git config");
        if is_valid_login(&o) {
            o
        } else {
            // Treat as full name; attempt search
            match search_owner_by_fullname(&o, token.as_deref()).await? {
                Some(login) => login,
                None => resolve_owner_interactively()?,
            }
        }
    } else if let Some(o) = gh_hosts_user() {
        debug!(owner=%o, "Found user in gh hosts.yml");
        o
    } else if let Some(fullname) = git_config_fullname() {
        debug!(%fullname, "Trying GitHub search by full name");
        match search_owner_by_fullname(&fullname, token.as_deref()).await {
            Ok(Some(login)) => {
                debug!(owner=%login, "Found login via search API");
                login
            }
            Ok(None) => {
                debug!("Search API returned no hits");
                resolve_owner_interactively()? // maybe prompt or err
            }
            Err(e) => {
                debug!(error=%e, "Search API error");
                resolve_owner_interactively()? // fallback to prompt
            }
        }
    } else {
        resolve_owner_interactively()?
    };

    // 2. Repo & branch defaults / overrides
    let repo = repo_flag.unwrap_or_else(|| "cursor-rules".to_string());
    validate_repo_name(&repo).context("Invalid repository name")?; // convert to anyhow then into NetworkError later maybe

    let branch = branch_flag.unwrap_or_else(|| "main".to_string());

    // 3. Check visibility/existence via GitHub API
    verify_repo_exists(&owner, &repo, token.as_deref()).await?;

    Ok(RepoLocator {
        owner,
        repo,
        branch,
    })
}

fn resolve_owner_interactively() -> Result<String, RepoDiscoveryError> {
    if io::stdin().is_terminal() {
        let ans = Text::new("GitHub owner to fetch rules from")
            .with_placeholder("GitHub username or org")
            .prompt();
        match ans {
            Ok(val) if !val.trim().is_empty() => {
                // Persist for future runs
                let _ = Command::new("git")
                    .args(["config", "--global", "user.username", &val])
                    .status();
                Ok(val)
            }
            _ => Err(RepoDiscoveryError::OwnerPromptCancelled),
        }
    } else {
        Err(RepoDiscoveryError::OwnerNotFound)
    }
}

fn git_config_username() -> Option<String> {
    get_git_config_value("user.username")
}

fn git_config_fullname() -> Option<String> {
    get_git_config_value("user.name")
}

fn get_git_config_value(key: &str) -> Option<String> {
    if let Ok(output) = Command::new("git").args(["config", "--get", key]).output() {
        if output.status.success() {
            if let Ok(mut s) = String::from_utf8(output.stdout) {
                s = s.trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Attempt to read GitHub username from gh CLI hosts.yml
fn gh_hosts_user() -> Option<String> {
    use std::env;
    let path_candidates: Vec<PathBuf> = {
        let mut v = Vec::new();
        if let Some(custom) = env::var_os("XDG_CONFIG_HOME") {
            v.push(PathBuf::from(custom).join("gh").join("hosts.yml"));
        }
        if let Some(dir) = dirs::config_dir() {
            v.push(dir.join("gh").join("hosts.yml"));
        }
        if let Some(home) = dirs::home_dir() {
            v.push(home.join(".config").join("gh").join("hosts.yml"));
        }
        v
    };

    let path = path_candidates.into_iter().find(|p| p.exists())?;

    let content = fs::read_to_string(path).ok()?;

    // Simpler: parse manually
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    // Look for github.com top-level
    if let Some(gh_node) = yaml.get("github.com") {
        if let Some(user) = gh_node.get("user").and_then(|v| v.as_str()) {
            if !user.is_empty() {
                return Some(user.to_string());
            }
        }
        if let Some(users_map) = gh_node.get("users") {
            if let Some(obj) = users_map.as_mapping() {
                if let Some((first_key, _)) = obj.iter().next() {
                    if let Some(login) = first_key.as_str() {
                        return Some(login.to_string());
                    }
                }
            }
        }
    }
    None
}

async fn search_owner_by_fullname(
    fullname: &str,
    token: Option<&str>,
) -> Result<Option<String>, RepoDiscoveryError> {
    let raw = fullname.trim().replace(' ', "+");
    let query = format!("fullname:{}", raw);

    let octocrab = build_octocrab(token)?;

    // REST endpoint: /search/users?q=...
    let result: serde_json::Value = octocrab
        .get("/search/users", Some(&[("q", &query)]))
        .await
        .map_err(|e| RepoDiscoveryError::NetworkError(e.into()))?;

    if let Some(items) = result.get("items").and_then(|v| v.as_array()) {
        if let Some(first) = items.first() {
            if let Some(login) = first.get("login").and_then(|v| v.as_str()) {
                return Ok(Some(login.to_string()));
            }
        }
    }
    Ok(None)
}

fn validate_repo_name(name: &str) -> anyhow::Result<()> {
    let re = Regex::new(REPO_NAME_REGEX).expect("valid regex");
    if re.is_match(name) {
        Ok(())
    } else {
        anyhow::bail!("Repository name '{name}' violates GitHub naming constraints");
    }
}

async fn verify_repo_exists(
    owner: &str,
    repo: &str,
    token: Option<&str>,
) -> Result<(), RepoDiscoveryError> {
    let octocrab = build_octocrab(token)?;

    let path = format!("/repos/{}/{}", owner, repo);
    let res: Result<serde_json::Value, octocrab::Error> = octocrab.get(&path, None::<&()>).await;

    match res {
        Ok(_) => {
            debug!("Repository accessible");
            Ok(())
        }
        Err(e) => {
            if let octocrab::Error::GitHub { source, .. } = &e {
                if source.status_code == http::StatusCode::NOT_FOUND {
                    return Err(RepoDiscoveryError::RepoNotFound {
                        owner: owner.to_string(),
                        repo: repo.to_string(),
                    });
                }
            }
            Err(RepoDiscoveryError::NetworkError(e.into()))
        }
    }
}

fn is_valid_login(name: &str) -> bool {
    Regex::new(LOGIN_REGEX).unwrap().is_match(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use libc;

    #[test]
    #[serial_test::serial]
    fn parse_gh_hosts_user() {
        let sample = r#"github.com:
  git_protocol: https
  users:
    alice:
      oauth_token: x
  user: alice
"#;

        let tmp_dir = tempfile::tempdir().unwrap();
        // Use XDG_CONFIG_HOME so function finds hosts.yml cross-platform.
        let orig_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", tmp_dir.path());

        let gh_dir = tmp_dir.path().join("gh");
        std::fs::create_dir_all(&gh_dir).unwrap();
        let file_path = gh_dir.join("hosts.yml");
        std::fs::write(&file_path, sample).unwrap();

        let owner = gh_hosts_user();

        // Restore
        if let Some(val) = orig_xdg {
            std::env::set_var("XDG_CONFIG_HOME", val);
        } else {
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        assert_eq!(owner, Some("alice".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn validate_repo_name_good() {
        assert!(validate_repo_name("cursor-rules").is_ok());
        assert!(validate_repo_name("a-valid_repo-name.123").is_ok());
    }

    #[test]
    #[serial_test::serial]
    fn validate_repo_name_bad() {
        assert!(validate_repo_name("invalid repo").is_err());
        assert!(validate_repo_name("contains?question").is_err());
    }

    #[test]
    #[serial_test::serial]
    fn git_config_username_detects_value() {
        // Create a tempdir and fake `git` executable that prints a username.
        let tmp_dir = tempfile::tempdir().unwrap();
        let bin_dir = tmp_dir.path();

        // Name of the fake executable
        let git_path = bin_dir.join(if cfg!(windows) { "git.cmd" } else { "git" });

        let script_content = if cfg!(windows) {
            // Simple batch script that echoes argument similar to `git config --get user.username`
            "@echo off\r\necho johndoe\r\n"
        } else {
            "#!/usr/bin/env sh\necho johndoe"
        };

        std::fs::write(&git_path, script_content).unwrap();
        // Make it executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&git_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&git_path, perms).unwrap();
        }

        // Prepend temp bin dir to PATH
        let orig_path = std::env::var("PATH").unwrap_or_default();
        let path_separator = if cfg!(windows) { ";" } else { ":" };
        let new_path = format!("{}{}{}", bin_dir.display(), path_separator, orig_path);
        std::env::set_var("PATH", &new_path);

        let val = super::git_config_username();

        // Restore PATH
        std::env::set_var("PATH", orig_path);

        assert_eq!(val, Some("johndoe".to_string()));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn search_owner_fullname_hit() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/search/users")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{\n  \"items\": [{ \"login\": \"johndoe\" }]\n}")
            .create_async()
            .await;

        std::env::set_var("OCTO_BASE", format!("{}/", server.url()));
        let res = super::search_owner_by_fullname("John Doe", None)
            .await
            .unwrap();
        mock.assert_async().await;
        assert_eq!(res, Some("johndoe".to_string()));
        std::env::remove_var("OCTO_BASE");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn invalid_username_falls_back_to_search() {
        // Fake git returning invalid username with space
        let tmp_dir = tempfile::tempdir().unwrap();
        let bin_dir = tmp_dir.path();
        let git_path = bin_dir.join(if cfg!(windows) { "git.cmd" } else { "git" });
        let script = if cfg!(windows) {
            "@echo off\r\necho John Doe\r\n"
        } else {
            "#!/usr/bin/env sh\necho John Doe"
        };
        std::fs::write(&git_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&git_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let orig_path = std::env::var("PATH").unwrap_or_default();
        let path_separator = if cfg!(windows) { ";" } else { ":" };
        std::env::set_var(
            "PATH",
            format!("{}{}{}", bin_dir.display(), path_separator, orig_path),
        );

        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/search/users")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{\"items\":[{\"login\":\"jdoe\"}]}\n")
            .create_async()
            .await;

        // Also mock repo exists 200
        server
            .mock("GET", "/repos/jdoe/cursor-rules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{\"id\":1,\"node_id\":\"R_kgD...\",\"name\":\"cursor-rules\"}")
            .create_async()
            .await;

        std::env::set_var("OCTO_BASE", format!("{}/", server.url()));
        let locator = super::resolve_repo(None, None, None, None).await.unwrap();
        assert_eq!(locator.owner, "jdoe");

        // cleanup
        std::env::set_var("PATH", orig_path);
        std::env::remove_var("OCTO_BASE");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn verify_repo_exists_maps_404() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/repos/foo/bar")
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body("{\"message\":\"Not Found\"}")
            .create_async()
            .await;
        std::env::set_var("OCTO_BASE", format!("{}/", server.url()));
        let err = super::verify_repo_exists("foo", "bar", None)
            .await
            .unwrap_err();
        std::env::remove_var("OCTO_BASE");
        match err {
            super::RepoDiscoveryError::RepoNotFound { owner, repo } => {
                assert_eq!(owner, "foo");
                assert_eq!(repo, "bar");
            }
            _ => panic!("unexpected error variant"),
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn owner_not_found_non_interactive() {
        // Ensure no git config values by shadowing `git` with a stub that exits with error
        let tmp_dir = tempfile::tempdir().unwrap();
        let bin_dir = tmp_dir.path();
        let git_path = bin_dir.join(if cfg!(windows) { "git.cmd" } else { "git" });

        let script = if cfg!(windows) {
            "@echo off\nexit /B 1"
        } else {
            "#!/usr/bin/env sh\nexit 1"
        };
        std::fs::write(&git_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&git_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // Override PATH so the stub is used
        let orig_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", bin_dir);

        // Redirect XDG_CONFIG_HOME and HOME to the temp dir so gh hosts.yml cannot be found
        let orig_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", tmp_dir.path());
        #[cfg(unix)]
        let orig_home = std::env::var("HOME").ok();
        #[cfg(windows)]
        let orig_home = std::env::var("USERPROFILE").ok();
        #[cfg(unix)]
        std::env::set_var("HOME", tmp_dir.path());
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", tmp_dir.path());

        // Make stdin non-tty so is_terminal() returns false (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let devnull = std::fs::File::open("/dev/null").unwrap();
            unsafe {
                libc::dup2(devnull.as_raw_fd(), 0);
            }
        }

        // Call resolve_repo without overrides – should error with OwnerNotFound
        let err = super::resolve_repo(None, None, None, None)
            .await
            .unwrap_err();

        match err {
            super::RepoDiscoveryError::OwnerNotFound => {}
            _ => panic!("expected OwnerNotFound error, got {err:?}"),
        }

        // Restore env
        std::env::set_var("PATH", orig_path);
        if let Some(val) = orig_xdg {
            std::env::set_var("XDG_CONFIG_HOME", val);
        } else {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        #[cfg(unix)]
        if let Some(val) = orig_home {
            std::env::set_var("HOME", val);
        } else {
            std::env::remove_var("HOME");
        }
        #[cfg(windows)]
        if let Some(val) = orig_home {
            std::env::set_var("USERPROFILE", val);
        } else {
            std::env::remove_var("USERPROFILE");
        }
    }
}
