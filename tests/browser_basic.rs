use assert_cmd::cargo::cargo_bin;
use expectrl::{spawn, Eof};
use mockito::{Matcher, Server};
use serde_json::json;

#[test]
fn tui_quits_on_q() -> anyhow::Result<()> {
    // Prepare mock GitHub API
    let tree_resp = json!({
        "tree": [
            {"path": "frontend", "type": "tree"},
            {"path": "frontend/react.mdc", "type": "blob"}
        ]
    });

    let mut server = Server::new();
    let base = server.url();

    let _m = server
        .mock("GET", "/repos/test/cursor-rules/git/trees/main")
        .match_query(Matcher::UrlEncoded("recursive".into(), "1".into()))
        .with_status(200)
        .with_body(tree_resp.to_string())
        .create();

    // Build command string to run in a shell so we can pass args
    let bin = cargo_bin("cursor-rules");

    // Set env var for this process so child inherits
    std::env::set_var("OCTO_BASE", &base);

    let cmd_str = format!("{} browse --owner test --all", bin.display());

    // Spawn via expectrl
    let mut p = spawn(cmd_str.as_str())?;

    // Give program moment then send 'q'
    p.send("q")?;
    p.expect(Eof)?;

    Ok(())
}
