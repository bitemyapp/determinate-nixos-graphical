use anyhow::{Result, ensure};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

pub fn prepare(root: &Path, hostname: &str, templates: &Path) -> Result<String> {
    let hostname = if hostname.is_empty() {
        "nixos"
    } else {
        hostname
    };
    ensure!(
        hostname.len() <= 63
            && hostname
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && hostname.as_bytes()[0].is_ascii_alphanumeric()
            && hostname.as_bytes()[hostname.len() - 1].is_ascii_alphanumeric(),
        "The target hostname must be a DNS label of 1 to 63 characters"
    );
    ensure!(
        root.is_absolute() && root.canonicalize()? != Path::new("/"),
        "Expected an absolute installation root other than /"
    );
    let destination = root.join("etc/nixos");
    for file in ["configuration.nix", "hardware-configuration.nix"] {
        ensure!(
            destination.join(file).is_file(),
            "Calamares must generate {file} first"
        );
    }
    let template = fs::read_to_string(templates.join("flake.nix.in"))?;
    ensure!(
        template.matches("@HOSTNAME@").count() == 1,
        "Expected exactly one hostname placeholder"
    );
    let lock = fs::read(templates.join("flake.lock"))?;
    let data: Value = serde_json::from_slice(&lock)?;
    let root_node = data["root"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing lock root"))?;
    let inputs = data["nodes"][root_node]["inputs"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Missing root inputs"))?;
    ensure!(
        inputs.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == BTreeSet::from(["nixpkgs", "determinate", "fh"]),
        "Target lock has unexpected root inputs"
    );
    fs::write(
        destination.join("flake.nix"),
        template.replace("@HOSTNAME@", &serde_json::to_string(hostname)?),
    )?;
    fs::write(destination.join("flake.lock"), lock)?;
    Ok(format!("{}#{hostname}", destination.display()))
}
pub fn main(args: Vec<String>) -> Result<()> {
    ensure!(
        args.len() == 2 || args.len() == 3,
        "Usage: prepare_target.rs ROOT HOSTNAME [TEMPLATE_DIR]"
    );
    println!(
        "{}",
        prepare(
            Path::new(&args[0]),
            &args[1],
            Path::new(
                args.get(2)
                    .map(String::as_str)
                    .unwrap_or("/etc/determinate-installer")
            )
        )?
    );
    Ok(())
}

pub fn regression_tests(template: &Path, lock: &Path) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("target");
    let config = root.join("etc/nixos");
    let templates = temp.path().join("templates");
    fs::create_dir_all(&config)?;
    fs::create_dir(&templates)?;
    fs::copy(template, templates.join("flake.nix.in"))?;
    fs::copy(lock, templates.join("flake.lock"))?;
    let original = "{ networking.hostName = \"test-host\"; }";
    fs::write(config.join("configuration.nix"), original)?;
    fs::write(config.join("hardware-configuration.nix"), "{}")?;
    ensure!(
        prepare(&root, "test-host", &templates)? == format!("{}#test-host", config.display()),
        "Wrong flake selector"
    );
    ensure!(
        fs::read_to_string(config.join("configuration.nix"))? == original,
        "Configuration changed"
    );
    ensure!(
        fs::read(config.join("flake.lock"))? == fs::read(lock)?,
        "Lock changed"
    );
    let generated = fs::read_to_string(config.join("flake.nix"))?;
    ensure!(
        generated.contains("nixosConfigurations.\"test-host\"")
            && generated.contains("determinate.nixosModules.default")
            && !generated.contains("@HOSTNAME@"),
        "Wrong generated flake"
    );
    ensure!(
        prepare(&root, "", &templates)?.ends_with("#nixos"),
        "Wrong default hostname"
    );
    for name in [
        "../oops",
        "${builtins.abort \"oops\"}",
        "-bad",
        "bad-",
        "x".repeat(64).as_str(),
        "é",
    ] {
        ensure!(
            prepare(&root, name, &templates).is_err(),
            "Accepted invalid hostname: {name}"
        );
    }
    ensure!(
        prepare(Path::new("/"), "nixos", &templates).is_err(),
        "Accepted host root"
    );
    fs::remove_file(config.join("hardware-configuration.nix"))?;
    ensure!(
        prepare(&root, "nixos", &templates).is_err(),
        "Accepted missing hardware configuration"
    );
    println!("PASS: Rust target-handoff regression tests");
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn invalid_hostname_rejected_before_io() {
        for hostname in ["bad-", "-bad", "bad.name", "a/b", "é"] {
            assert!(
                super::prepare(
                    std::path::Path::new("/absent"),
                    hostname,
                    std::path::Path::new("/absent")
                )
                .unwrap_err()
                .to_string()
                .contains("DNS label")
            );
        }
    }
}
