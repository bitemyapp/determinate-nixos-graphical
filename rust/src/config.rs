use anyhow::{Result, bail, ensure};
use std::{collections::BTreeMap, fs};

fn defaults(text: &str) -> Result<BTreeMap<String, String>> {
    let mut section = false;
    let mut values = BTreeMap::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(['#', ';']))
    {
        if line.starts_with('[') {
            ensure!(
                line == "[Defaults]" && !section,
                "Unexpected or duplicate INI section"
            );
            section = true;
        } else {
            ensure!(section, "INI setting outside Defaults");
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid INI setting"))?;
            ensure!(
                values
                    .insert(k.trim().to_lowercase(), v.trim().to_string())
                    .is_none(),
                "Duplicate INI key"
            );
        }
    }
    ensure!(section, "Missing Defaults");
    Ok(values)
}
pub fn main(args: Vec<String>) -> Result<()> {
    ensure!(
        args.len() == 3,
        "Usage: check_config.rs generated|defaults FILE FILE"
    );
    match args[0].as_str() {
        "generated" => ensure!(
            fs::read_to_string(&args[1])?.trim_end()
                == fs::read_to_string(&args[2])?
                    .replace("@HOSTNAME@", "\"nixos\"")
                    .trim_end(),
            "Generated flake differs from template"
        ),
        "defaults" => {
            for (file, kernel) in args[1..].iter().zip(["lts", "latest"]) {
                let config = defaults(&fs::read_to_string(file)?)?;
                ensure!(
                    config.get("flake").map(String::as_str) == Some("1")
                        && config.get("kernel").map(String::as_str) == Some(kernel),
                    "Wrong defaults in {file}"
                );
            }
        }
        _ => bail!("Unknown check"),
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn strict_ini() {
        assert!(super::defaults("[Defaults]\nFlake=1\nKernel=lts").is_ok());
        for text in [
            "[Defaults]\n[Defaults]",
            "[Defaults]\nFlake=1\nflake=1",
            "Flake=1",
            "[Other]",
        ] {
            assert!(super::defaults(text).is_err());
        }
    }
}
