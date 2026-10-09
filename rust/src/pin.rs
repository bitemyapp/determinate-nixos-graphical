//! Pins the inputs this image is built and verified with: the installer
//! (`calamares`, also recorded in `nix/calamares-source.json`), the Tatami
//! desktop (`tatami`) and the Yukimi app (`yukimi`). Each lock node keeps tracking its `stable` branch, as
//! flake.nix and the installed systems' template do, while locking a chosen
//! revision, so an image can be built and verified before `stable` moves to it.
use crate::support::{output, repo};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::{fs, process::Command};

const OWNER: &str = "bitemyapp";
const BRANCH: &str = "stable";

/// A pinned input: a GitHub repository of the same name.
pub struct Input {
    name: &'static str,
    /// Its own inputs that follow ours, by name.
    follows: &'static [&'static str],
    /// Also recorded here, for building from that source.
    source_json: Option<&'static str>,
}

pub const CALAMARES: Input = Input {
    name: "calamares",
    follows: &["nixpkgs", "tatami", "yukimi"],
    source_json: Some("nix/calamares-source.json"),
};

pub const TATAMI: Input = Input {
    name: "tatami",
    follows: &["nixpkgs"],
    source_json: None,
};

pub const YUKIMI: Input = Input {
    name: "yukimi",
    follows: &["nixpkgs"],
    source_json: None,
};

/// The lock node for a prefetched revision.
fn node(input: &Input, rev: &str, nar_hash: &str, last_modified: u64) -> Value {
    let inputs: Map<String, Value> = input
        .follows
        .iter()
        .map(|name| ((*name).to_owned(), json!([name])))
        .collect();
    json!({
        "inputs": inputs,
        "locked": {
            "lastModified": last_modified,
            "narHash": nar_hash,
            "owner": OWNER,
            "repo": input.name,
            "rev": rev,
            "type": "github"
        },
        "original": { "owner": OWNER, "ref": BRANCH, "repo": input.name, "type": "github" }
    })
}

fn relock(lock: &mut Value, name: &str, node: Value) -> Result<()> {
    let root = lock["root"]
        .as_str()
        .context("Missing lock root")?
        .to_owned();
    ensure!(lock["nodes"][&root].is_object(), "Missing root node");
    lock["nodes"][&root]["inputs"][name] = json!(name);
    lock["nodes"][name] = node;
    Ok(())
}

pub fn main(input: &Input, args: Vec<String>) -> Result<()> {
    ensure!(args.len() == 1, "Usage: pin-{} REVISION", input.name);
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
        &format!("github:{OWNER}/{}/{rev}", input.name),
    ]))?)?;
    ensure!(
        fetched["locked"]["rev"] == rev,
        "Prefetched a different revision"
    );
    let nar_hash = fetched["hash"].as_str().context("No narHash")?;
    let last_modified = fetched["locked"]["lastModified"]
        .as_u64()
        .context("No lastModified")?;
    if let Some(source) = input.source_json {
        let pin = json!({ "owner": OWNER, "repo": input.name, "rev": rev, "hash": nar_hash });
        fs::write(
            repo.join(source),
            serde_json::to_string_pretty(&pin)? + "\n",
        )?;
    }
    let path = repo.join("flake.lock");
    let mut lock: Value = serde_json::from_slice(&fs::read(&path)?)?;
    relock(
        &mut lock,
        input.name,
        node(input, rev, nar_hash, last_modified),
    )?;
    fs::write(&path, serde_json::to_string_pretty(&lock)? + "\n")?;
    // Nix must accept the edited lock as consistent with flake.nix.
    output(Command::new("nix").args([
        "flake",
        "metadata",
        "--no-update-lock-file",
        &format!("git+file://{}", repo.display()),
    ]))?;
    println!(
        "Pinned {} {rev} ({nar_hash}), tracking {BRANCH}",
        input.name
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relock_tracks_stable_and_follows_our_inputs() {
        let mut lock = json!({
            "nodes": { "root": { "inputs": { "nixpkgs": "nixpkgs" } }, "nixpkgs": {} },
            "root": "root",
            "version": 7
        });
        relock(
            &mut lock,
            "tatami",
            node(&TATAMI, &"b".repeat(40), "sha256-y", 8),
        )
        .unwrap();
        relock(
            &mut lock,
            "calamares",
            node(&CALAMARES, &"a".repeat(40), "sha256-x", 7),
        )
        .unwrap();
        assert_eq!(lock["nodes"]["root"]["inputs"]["calamares"], "calamares");
        assert_eq!(lock["nodes"]["root"]["inputs"]["tatami"], "tatami");
        let calamares = &lock["nodes"]["calamares"];
        assert_eq!(calamares["original"]["ref"], "stable");
        assert_eq!(calamares["locked"]["rev"], "a".repeat(40));
        assert_eq!(calamares["inputs"]["nixpkgs"], json!(["nixpkgs"]));
        assert_eq!(calamares["inputs"]["tatami"], json!(["tatami"]));
        assert_eq!(calamares["inputs"]["yukimi"], json!(["yukimi"]));
        let tatami = &lock["nodes"]["tatami"];
        assert_eq!(tatami["original"]["repo"], "tatami");
        assert_eq!(tatami["inputs"], json!({ "nixpkgs": ["nixpkgs"] }));
        // Nix writes lock keys sorted; serde_json's default map keeps them so.
        let text = serde_json::to_string_pretty(&lock).unwrap();
        assert!(text.find("\"nodes\"").unwrap() < text.find("\"root\": \"root\"").unwrap());
        assert!(relock(&mut json!({}), "calamares", Value::Null).is_err());
    }
}
