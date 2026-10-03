//! Rust fixture for the upstream Python Calamares API. No Python fixture source
//! is generated or executed: callbacks, disk policy, and assertions live here.
use crate::{support::*, target};
use anyhow::{Result, ensure};
use pyo3::{
    exceptions::PyRuntimeError,
    prelude::*,
    types::{PyDict, PyList, PyModule},
};
use serde_json::json;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
};

#[pyfunction]
fn progress(value: f64) {
    static LAST: AtomicU32 = AtomicU32::new(0);
    let step = (value * 1000.0) as u32;
    if step > LAST.fetch_max(step, Ordering::Relaxed) {
        println!("PROGRESS {value:.3}");
    }
}
#[pyfunction]
fn log(message: &str) {
    println!("{message}");
}
#[pyfunction]
fn gettext_path() -> &'static str {
    "/usr/share/locale"
}
#[pyfunction]
fn gettext_languages() -> Vec<&'static str> {
    vec!["en"]
}
#[pyfunction]
fn no_proxy() -> Vec<String> {
    vec![]
}

#[pyfunction(signature = (argv, cwd=None, input_data=None))]
fn host_process(
    py: Python<'_>,
    argv: Vec<String>,
    cwd: Option<String>,
    input_data: Option<String>,
) -> PyResult<String> {
    let result = (|| -> Result<_> {
        ensure!(!argv.is_empty(), "Empty command");
        let mut input = input_data;
        if argv == ["cp", "/dev/stdin", "/mnt/etc/nixos/configuration.nix"] {
            let original = input.as_deref().ok_or_else(|| anyhow::anyhow!("Missing generated configuration"))?;
            let end = original.rfind('}').ok_or_else(|| anyhow::anyhow!("Invalid generated configuration"))?;
            input = Some(format!("{}\n  # Test diagnostics only; absent from normal GUI installs.\n  services.qemuGuest.enable = true;\n  boot.kernelParams = [ \"console=ttyS0,115200n8\" \"console=tty0\" ];\n{}", &original[..end], &original[end..]));
        }
        let mut command = Command::new(&argv[0]); command.args(&argv[1..]).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(cwd) = cwd { command.current_dir(cwd); }
        if input.is_some() { command.stdin(Stdio::piped()); } else { command.stdin(Stdio::null()); }
        let mut child = command.spawn()?;
        if let Some(input) = input { child.stdin.take().unwrap().write_all(input.as_bytes())?; }
        Ok(child.wait_with_output()?)
    })().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    if !result.status.success() {
        let kwargs = PyDict::new(py);
        kwargs.set_item("output", String::from_utf8_lossy(&result.stdout).as_ref())?;
        kwargs.set_item("stderr", String::from_utf8_lossy(&result.stderr).as_ref())?;
        let error = py
            .import("subprocess")?
            .getattr("CalledProcessError")?
            .call((result.status.code().unwrap_or(-1), argv), Some(&kwargs))?;
        return Err(PyErr::from_value(error));
    }
    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}

fn calamares_module() -> Result<PathBuf> {
    let normal = PathBuf::from("/run/current-system/sw/lib/calamares/modules/nixos/main.py");
    if normal.is_file() {
        return Ok(normal);
    }
    for entry in fs::read_dir("/nix/store")? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .contains("-calamares-nixos-extensions-")
        {
            let path = entry.path().join("lib/calamares/modules/nixos/main.py");
            if path.is_file() && fs::read_to_string(&path)?.contains("target_flake") {
                return Ok(path);
            }
        }
    }
    anyhow::bail!("Patched Calamares job not found")
}

pub fn install() -> Result<()> {
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "Run only inside the disposable test VM as root"
    );
    ensure!(
        fs::read_to_string("/sys/class/block/vda/serial")?.trim() == "RESPIN_TEST_ONLY",
        "Refusing a disk without the test-only serial"
    );
    ensure!(
        output(Command::new("blockdev").args(["--getsize64", "/dev/vda"]))?.trim()
            == (40u64 * 1024u64.pow(3)).to_string(),
        "Disk is not exactly 40 GiB"
    );
    ensure!(
        output(Command::new("findmnt").args(["-n", "-o", "FSTYPE", "/"]))?.trim() == "tmpfs",
        "Expected the live temporary root filesystem"
    );
    ensure!(
        !Command::new("blkid").arg("/dev/vda").status()?.success(),
        "Refusing an already initialized disk"
    );
    let efi = Path::new("/sys/firmware/efi").exists();
    run(Command::new("parted").args(["-s", "/dev/vda", "mklabel", "gpt"]))?;
    if efi {
        run(Command::new("parted").args([
            "-s", "/dev/vda", "mkpart", "ESP", "fat32", "1MiB", "1025MiB",
        ]))?;
        run(Command::new("parted").args(["-s", "/dev/vda", "set", "1", "esp", "on"]))?;
    } else {
        run(Command::new("parted").args(["-s", "/dev/vda", "mkpart", "BIOS", "1MiB", "3MiB"]))?;
        run(Command::new("parted").args(["-s", "/dev/vda", "set", "1", "bios_grub", "on"]))?;
    }
    run(Command::new("parted").args([
        "-s", "/dev/vda", "mkpart", "root", "ext4", "1025MiB", "100%",
    ]))?;
    run(Command::new("udevadm").arg("settle"))?;
    run(Command::new("mkfs.ext4").args(["-L", "RESPIN_TEST", "/dev/vda2"]))?;
    fs::create_dir_all("/mnt")?;
    run(Command::new("mount").args(["/dev/vda2", "/mnt"]))?;
    if efi {
        run(Command::new("mkfs.fat").args(["-F", "32", "/dev/vda1"]))?;
        fs::create_dir_all("/mnt/boot")?;
        run(Command::new("mount").args(["/dev/vda1", "/mnt/boot"]))?;
    }
    let mut data = json!({"rootMountPoint":"/mnt", "firmwareType":if efi {"efi"} else {"bios"},
        "bootLoader":{"installPath":"/dev/vda"},
        "partitions":[{"mountPoint":"/","fs":"ext4","fsName":"ext4","claimed":true,"device":"/dev/vda2"}],
        "hostname":"respin-test","username":"respintest","fullname":"Respin Test", "locationRegion":"Etc","locationZone":"UTC",
        "packagechooser_packagechooser":"plasma6","keyboardLayout":"us","keyboardVariant":"","keyboardVConsoleKeymap":"us","nixos_allow_unfree":false});
    if efi {
        data["partitions"].as_array_mut().unwrap().push(json!({"mountPoint":"/boot","fs":"fat32","fsName":"fat32","claimed":true,"device":"/dev/vda1"}));
    }
    let path = calamares_module()?;
    ensure!(
        fs::read_to_string(&path)?.contains("target_flake"),
        "Unpatched Calamares job"
    );
    Python::attach(|py| -> PyResult<()> {
        let fixture = PyModule::new(py, "libcalamares")?;
        let namespace = py.import("types")?.getattr("SimpleNamespace")?;
        let storage = namespace.call0()?;
        let values = py
            .import("json")?
            .getattr("loads")?
            .call1((data.to_string(),))?;
        storage.setattr("value", values.getattr("get")?)?;
        fixture.add("globalstorage", storage)?;
        let job = namespace.call0()?;
        job.setattr("setprogress", wrap_pyfunction!(progress, py)?)?;
        fixture.add("job", job)?;
        let utils = namespace.call0()?;
        utils.setattr("gettext_path", wrap_pyfunction!(gettext_path, py)?)?;
        utils.setattr(
            "gettext_languages",
            wrap_pyfunction!(gettext_languages, py)?,
        )?;
        for name in ["debug", "warning", "error"] {
            utils.setattr(name, wrap_pyfunction!(log, py)?)?;
        }
        utils.setattr(
            "host_env_process_output",
            wrap_pyfunction!(host_process, py)?,
        )?;
        fixture.add("utils", utils)?;
        py.import("sys")?
            .getattr("modules")?
            .set_item("libcalamares", fixture)?;
        let util = py.import("importlib.util")?;
        let spec = util
            .getattr("spec_from_file_location")?
            .call1(("nixos_job", path.to_string_lossy().as_ref()))?;
        let module = util.getattr("module_from_spec")?.call1((&spec,))?;
        spec.getattr("loader")?
            .call_method1("exec_module", (&module,))?;
        let failure = module.call_method0("run")?;
        if !failure.is_none() {
            return Err(PyRuntimeError::new_err(failure.str()?.to_string()));
        }
        Ok(())
    })?;
    ensure!(
        fs::read("/mnt/etc/nixos/flake.lock")?
            == fs::read("/etc/determinate-installer/flake.lock")?,
        "Installed lock changed"
    );
    ensure!(
        fs::read_to_string("/mnt/etc/nixos/flake.nix")?
            .contains("nixosConfigurations.\"respin-test\""),
        "Wrong target hostname"
    );
    run(&mut Command::new("sync"))?;
    println!("CALAMARES_INSTALL_PASS");
    Ok(())
}

pub fn test_handoff(args: Vec<String>) -> Result<()> {
    ensure!(
        args.len() == 3,
        "Usage: test_handoff.rs TEMPLATE LOCK PATCHED_MAIN"
    );
    target::regression_tests(Path::new(&args[0]), Path::new(&args[1]))?;
    // Exercise the actual upstream Python AST through its API, not a duplicate
    // Rust implementation of the install command or an embedded Python script.
    let source = fs::read_to_string(&args[2])?;
    Python::attach(|py| -> PyResult<()> {
        let ast = py.import("ast")?;
        let tree = ast.call_method1("parse", (source,))?;
        let mut body = None;
        for node in tree.getattr("body")?.try_iter()? {
            let node = node?;
            if node
                .getattr("name")
                .and_then(|n| n.extract::<String>())
                .ok()
                .as_deref()
                == Some("run")
            {
                body = Some(node.getattr("body")?);
                break;
            }
        }
        let body = body.ok_or_else(|| PyRuntimeError::new_err("Missing upstream run function"))?;
        let mut start = None;
        for (index, node) in body.try_iter()?.enumerate() {
            if let Ok(targets) = node?.getattr("targets") {
                for target in targets.try_iter()? {
                    if target?
                        .getattr("id")
                        .and_then(|id| id.extract::<String>())
                        .ok()
                        .as_deref()
                        == Some("nixosInstallCmd")
                    {
                        start = Some(index);
                    }
                }
            }
        }
        let start =
            start.ok_or_else(|| PyRuntimeError::new_err("Missing upstream install argv"))?;
        let statements = PyList::empty(py);
        for i in start..start + 3 {
            statements.append(body.get_item(i)?)?;
        }
        let kwargs = PyDict::new(py);
        kwargs.set_item("body", statements)?;
        kwargs.set_item("type_ignores", PyList::empty(py))?;
        let ast_module = ast.getattr("Module")?.call((), Some(&kwargs))?;
        let builtins = py.import("builtins")?;
        let code = builtins.call_method1("compile", (ast_module, &args[2], "exec"))?;
        let context = PyDict::new(py);
        context.set_item("root_mount_point", "/test-root")?;
        context.set_item("target_flake", "/test-root/etc/nixos#test-host")?;
        context.set_item("generateProxyStrings", wrap_pyfunction!(no_proxy, py)?)?;
        builtins.call_method1("exec", (code, &context))?;
        let argv: Vec<String> = context.get_item("nixosInstallCmd")?.unwrap().extract()?;
        for (flag, expected) in [
            ("--root", "/test-root"),
            ("--flake", "/test-root/etc/nixos#test-host"),
        ] {
            let found = argv.windows(2).any(|p| p[0] == flag && p[1] == expected);
            if !found {
                return Err(PyRuntimeError::new_err(format!(
                    "Missing {flag} {expected}: {argv:?}"
                )));
            }
        }
        if !argv.iter().any(|a| a == "--no-root-passwd") {
            return Err(PyRuntimeError::new_err("Missing --no-root-passwd"));
        }
        Ok(())
    })?;
    println!("PASS: actual Calamares install argv selects the flake");
    Ok(())
}
