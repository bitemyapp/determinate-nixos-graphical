//! Pins the installer: `nix/calamares-source.json` and the `calamares` node of
//! flake.lock. The node keeps tracking the `stable` branch, as flake.nix and
//! the installed systems' template do, while locking a chosen revision, so an
//! image can be built and verified before `stable` moves to it.
use crate::support::{output, repo};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{fs, process::Command};

const OWNER: &str = "bitemyapp";
const REPO: &str = "calamares";
const BRANCH: &str = "stable";

/// The lock node for a prefetched revision. Its own nixpkgs follows ours.
fn node(rev: &str, nar_hash: &str, last_modified: u64) -> Value {
    json!({
        "inputs": { "nixpkgs": ["nixpkgs"] },
        "locked": {
            "lastModified": last_modified,
            "narHash": nar_hash,
            "owner": OWNER,
            "repo": REPO,
            "rev": rev,
            "type": "github"
        },
        "original": { "owner": OWNER, "ref": BRANCH, "repo": REPO, "type": "github" }
    })
}

fn relock(lock: &mut Value, node: Value) -> Result<()> {
    let root = lock["root"].as_str().context("Missing lock root")?.to_owned();
    ensure!(lock["nodes"][&root].is_object(), "Missing root node");
    lock["nodes"][&root]["inputs"]["calamares"] = json!("calamares");
    lock["nodes"]["calamares"] = node;
    Ok(())
}

pub fn main(args: Vec<String>) -> Result<()> {
    ensure!(args.len() == 1, "Usage: pin-calamares REVISION");
    let rev = args[0].as_str();
    ensure!(
        rev.len() == 40 && rev.bytes().all(|c| c.is_ascii_hexdigit()),
        "Supply a full commit hash"
    );
    let repo = repo()?;
    let fetched: Value = serde_json::from_str(&output(Command::new("nix").args([
        "flake",
        "prefetch",
        "--json",
        &format!("github:{OWNER}/{REPO}/{rev}"),
    ]))?)?;
    ensure!(
        fetched["locked"]["rev"] == rev,
        "Prefetched a different revision"
    );
    let nar_hash = fetched["hash"].as_str().context("No narHash")?;
    let last_modified = fetched["locked"]["lastModified"]
        .as_u64()
        .context("No lastModified")?;
    let pin = json!({ "owner": OWNER, "repo": REPO, "rev": rev, "hash": nar_hash });
    fs::write(
        repo.join("nix/calamares-source.json"),
        serde_json::to_string_pretty(&pin)? + "\n",
    )?;
    let path = repo.join("flake.lock");
    let mut lock: Value = serde_json::from_slice(&fs::read(&path)?)?;
    relock(&mut lock, node(rev, nar_hash, last_modified))?;
    fs::write(&path, serde_json::to_string_pretty(&lock)? + "\n")?;
    // Nix must accept the edited lock as consistent with flake.nix.
    output(Command::new("nix").args([
        "flake",
        "metadata",
        "--no-update-lock-file",
        &format!("git+file://{}", repo.display()),
    ]))?;
    println!("Pinned calamares {rev} ({nar_hash}), tracking {BRANCH}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relock_tracks_stable_and_follows_our_nixpkgs() {
        let mut lock = json!({
            "nodes": { "root": { "inputs": { "nixpkgs": "nixpkgs" } }, "nixpkgs": {} },
            "root": "root",
            "version": 7
        });
        relock(&mut lock, node(&"a".repeat(40), "sha256-x", 7)).unwrap();
        assert_eq!(lock["nodes"]["root"]["inputs"]["calamares"], "calamares");
        let calamares = &lock["nodes"]["calamares"];
        assert_eq!(calamares["original"]["ref"], "stable");
        assert_eq!(calamares["locked"]["rev"], "a".repeat(40));
        assert_eq!(calamares["inputs"]["nixpkgs"], json!(["nixpkgs"]));
        // Nix writes lock keys sorted; serde_json's default map keeps them so.
        let text = serde_json::to_string_pretty(&lock).unwrap();
        assert!(text.find("\"nodes\"").unwrap() < text.find("\"root\": \"root\"").unwrap());
        assert!(relock(&mut json!({}), Value::Null).is_err());
    }
}
