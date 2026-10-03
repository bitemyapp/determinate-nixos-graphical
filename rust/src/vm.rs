use crate::support::*;
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const SHORT: Duration = Duration::from_secs(120);
static IDS: AtomicU64 = AtomicU64::new(1);
pub struct Rpc {
    reader: BufReader<UnixStream>,
}
impl Rpc {
    pub fn connect(path: &Path, qmp: bool, seconds: u64) -> Result<Self> {
        let stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_secs(seconds)))?;
        stream.set_write_timeout(Some(Duration::from_secs(seconds)))?;
        let mut rpc = Self {
            reader: BufReader::new(stream),
        };
        if qmp {
            rpc.read()?;
            rpc.call("qmp_capabilities", json!({}))?;
        }
        Ok(rpc)
    }
    fn read(&mut self) -> Result<Value> {
        let mut line = String::new();
        ensure!(
            self.reader.read_line(&mut line)? > 0,
            "QEMU RPC connection closed"
        );
        Ok(serde_json::from_str(line.trim())?)
    }
    fn send(&mut self, command: &str, arguments: Value) -> Result<u64> {
        let id = IDS.fetch_add(1, Ordering::Relaxed);
        writeln!(
            self.reader.get_mut(),
            "{}",
            json!({"execute":command,"arguments":arguments,"id":id})
        )?;
        Ok(id)
    }
    fn request_shutdown(&mut self) -> Result<()> {
        // guest-shutdown deliberately has no success response. Vm::poweroff
        // confirms success by waiting for QEMU to exit with status zero.
        // https://www.qemu.org/docs/master/interop/qemu-ga-ref.html#command-guest-shutdown
        self.send("guest-shutdown", json!({"mode":"powerdown"}))?;
        Ok(())
    }
    pub fn call(&mut self, command: &str, arguments: Value) -> Result<Value> {
        let id = self.send(command, arguments)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            check_interrupt()?;
            ensure!(Instant::now() < deadline, "QEMU RPC response timed out");
            let response = self.read()?;
            if response["id"] != id {
                continue;
            }
            ensure!(
                response.get("error").is_none(),
                "QEMU RPC error: {response}"
            );
            return response
                .get("return")
                .cloned()
                .context("Missing QEMU RPC result");
        }
    }
}

pub struct Vm {
    work: PathBuf,
    process: Option<Process>,
    agent: Option<Rpc>,
}
impl Vm {
    pub fn start(
        work: &Path,
        firmware: &str,
        iso: Option<&Path>,
        disk: Option<&Path>,
        share: Option<&Path>,
    ) -> Result<Self> {
        ensure!(["bios", "uefi"].contains(&firmware), "Unknown firmware");
        fs::create_dir_all(work)?;
        for socket in ["qga.sock", "qmp.sock"] {
            ensure!(
                !work.join(socket).exists(),
                "Socket already exists; check for a running VM: {socket}"
            );
        }
        let mut command = Command::new("qemu-system-x86_64");
        command
            .args([
                "-name",
                "determinate-respin-test",
                "-machine",
                "q35,accel=kvm",
                "-cpu",
                "host",
                "-smp",
                "4",
                "-m",
                "8192",
                "-display",
                "none",
                "-vga",
                "std",
                "-no-reboot",
                "-serial",
            ])
            .arg(format!("file:{}", work.join("serial.log").display()))
            .arg("-qmp")
            .arg(format!(
                "unix:{},server=on,wait=off",
                qpath(&work.join("qmp.sock"))?
            ))
            .args(["-device", "virtio-serial-pci", "-chardev"])
            .arg(format!(
                "socket,path={},server=on,wait=off,id=qga",
                qpath(&work.join("qga.sock"))?
            ))
            .args([
                "-device",
                "virtserialport,chardev=qga,name=org.qemu.guest_agent.0",
                "-netdev",
                "user,id=net0",
                "-device",
                "virtio-net-pci,netdev=net0",
            ]);
        if firmware == "uefi" {
            let code = PathBuf::from(
                std::env::var_os("OVMF_CODE")
                    .unwrap_or("/usr/share/edk2/x64/OVMF_CODE.4m.fd".into()),
            );
            let source = PathBuf::from(
                std::env::var_os("OVMF_VARS")
                    .unwrap_or("/usr/share/edk2/x64/OVMF_VARS.4m.fd".into()),
            );
            let vars = work.join("OVMF_VARS.fd");
            if !vars.exists() {
                fs::copy(source, &vars)?;
            }
            command
                .arg("-drive")
                .arg(format!(
                    "if=pflash,format=raw,readonly=on,file={}",
                    qpath(&code)?
                ))
                .arg("-drive")
                .arg(format!("if=pflash,format=raw,file={}", qpath(&vars)?));
        }
        if let Some(iso) = iso {
            regular(iso)?;
            command
                .args(["-device", "qemu-xhci,id=xhci", "-drive"])
                .arg(format!(
                    "if=none,id=iso,format=raw,readonly=on,file={}",
                    qpath(iso)?
                ))
                .args(["-device", "usb-storage,drive=iso,bootindex=1"]);
        }
        if let Some(disk) = disk {
            regular(disk)?;
            command
                .arg("-drive")
                .arg(format!(
                    "if=none,id=target,format=raw,file={}",
                    qpath(disk)?
                ))
                .args([
                    "-device",
                    "virtio-blk-pci,drive=target,serial=RESPIN_TEST_ONLY,bootindex=2",
                ]);
        }
        if let Some(repo) = share {
            command.arg("-virtfs").arg(format!(
                "local,path={},mount_tag=project,security_model=none,id=project",
                qpath(repo)?
            ));
        }
        let log = File::create(work.join("qemu.log"))?;
        let process = Process(
            command
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .spawn()?,
        );
        Ok(Self {
            work: work.to_path_buf(),
            process: Some(process),
            agent: None,
        })
    }
    pub fn wait_agent(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(240);
        let mut last = String::new();
        while Instant::now() < deadline {
            ensure!(
                self.process
                    .as_mut()
                    .context("VM stopped")?
                    .0
                    .try_wait()?
                    .is_none(),
                "QEMU exited: {}",
                fs::read_to_string(self.work.join("qemu.log"))?
            );
            match Rpc::connect(&self.work.join("qga.sock"), false, 3).and_then(|mut rpc| {
                rpc.call("guest-ping", json!({}))?;
                Ok(rpc)
            }) {
                Ok(rpc) => {
                    rpc.reader
                        .get_ref()
                        .set_read_timeout(Some(Duration::from_secs(20)))?;
                    self.agent = Some(rpc);
                    return Ok(());
                }
                Err(error) => last = error.to_string(),
            }
            pause(Duration::from_secs(2))?;
        }
        bail!(
            "Guest agent did not start: {last}; see {}",
            self.work.join("serial.log").display()
        )
    }
    pub fn execute(&mut self, command: &str, timeout: Duration) -> Result<String> {
        let agent = self.agent.as_mut().context("Guest agent not connected")?;
        let pid = agent.call("guest-exec", json!({"path":"/run/current-system/sw/bin/bash","arg":["-lc", command],"capture-output":true}))?["pid"].clone();
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            let status = agent.call("guest-exec-status", json!({"pid":pid}))?;
            if status["exited"] == true {
                let mut text = String::new();
                for key in ["out-data", "err-data"] {
                    text.push_str(&String::from_utf8_lossy(
                        &STANDARD.decode(status[key].as_str().unwrap_or(""))?,
                    ));
                }
                ensure!(
                    status["exitcode"] == 0,
                    "Guest command failed: {command}\n{text}"
                );
                return Ok(text);
            }
            pause(Duration::from_secs(1))?;
        }
        bail!("Guest command timed out: {command}")
    }
    pub fn screenshot(&self, dest: &Path) -> Result<()> {
        Rpc::connect(&self.work.join("qmp.sock"), true, 20)?
            .call("screendump", json!({"filename":dest,"format":"png"}))?;
        Ok(())
    }
    fn poweroff(&mut self) -> Result<()> {
        self.agent
            .as_mut()
            .context("No guest agent")?
            .request_shutdown()?;
        let status = self
            .process
            .as_mut()
            .context("No VM")?
            .wait(Duration::from_secs(60))?;
        ensure!(status.success(), "QEMU poweroff failed");
        self.stop();
        Ok(())
    }
    pub fn stop(&mut self) {
        self.agent = None;
        self.process.take();
        for path in ["qga.sock", "qmp.sock"] {
            let _ = fs::remove_file(self.work.join(path));
        }
    }
    fn diagnostics(&mut self, artifacts: &Path) {
        let _ = self.screenshot(&artifacts.join("failure.png"));
        if self.agent.is_none() {
            return;
        }
        if let Ok(text) = self.execute("systemctl --failed; ps -eo uid,pid,comm,args; journalctl -b -p warning --no-pager | tail -150", SHORT) {
            let _ = fs::write(artifacts.join("diagnostics.log"), text);
        }
    }
}
impl Drop for Vm {
    fn drop(&mut self) {
        self.stop();
    }
}

fn verify_installed(vm: &mut Vm, artifacts: &Path, repo: &Path) -> Result<String> {
    vm.wait_agent()?;
    vm.execute("for i in $(seq 1 90); do systemctl is-active --quiet display-manager && exit 0; sleep 1; done; exit 1", SHORT)?;
    let output = vm.execute("set -e; test \"$(hostname)\" = respin-test; nix --version; fh --version; systemctl is-active determinate-nixd.socket; systemctl is-active display-manager; test ! -e /etc/determinate-installer; test ! -e /run/current-system/sw/bin/calamares; id respintest; ! id nixos; sha256sum /etc/nixos/flake.lock", SHORT)?;
    ensure!(
        output.contains("nix (Determinate Nix "),
        "Installed Nix is not Determinate: {output}"
    );
    ensure!(
        output.contains(&sha256(&repo.join("flake.lock"))?),
        "Installed lock differs from repository"
    );
    pause(Duration::from_secs(10))?;
    vm.screenshot(&artifacts.join("installed-desktop.png"))?;
    Ok(output)
}

fn record(
    mut vm: Vm,
    work: &Path,
    artifacts: &Path,
    result: &mut Value,
    outcome: Result<()>,
) -> Result<()> {
    result["passed"] = json!(outcome.is_ok());
    if let Err(error) = &outcome {
        result["error"] = json!(format!("{error:#}"));
        vm.diagnostics(artifacts);
    }
    vm.stop();
    for file in ["serial.log", "qemu.log"] {
        optional_copy(&work.join(file), &artifacts.join(file))?;
    }
    write_json(&artifacts.join("result.json"), result)?;
    outcome?;
    println!("PASS: {}", artifacts.display());
    Ok(())
}

pub fn main(args: Vec<String>) -> Result<()> {
    let mut iso = None;
    let mut firmware = "uefi".to_string();
    let mut install = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--firmware" => firmware = args.next().context("Missing firmware")?,
            "--install" => install = true,
            value if !value.starts_with('-') && iso.is_none() => iso = Some(PathBuf::from(value)),
            _ => bail!("Unknown argument: {arg}"),
        }
    }
    ensure!(
        ["bios", "uefi"].contains(&firmware.as_str()),
        "Firmware must be bios or uefi"
    );
    let iso = regular(&iso.context("Usage: qemu_test.rs ISO [--firmware bios|uefi] [--install]")?)?;
    let digest = sha256(&iso)?;
    let repo = repo()?;
    let name = format!("{firmware}-{}", stamp());
    let work = repo.join(".work").join(&name);
    let artifacts = repo.join("artifacts").join(&name);
    fs::create_dir_all(repo.join(".work"))?;
    fs::create_dir_all(repo.join("artifacts"))?;
    fs::create_dir(&work)?;
    fs::create_dir(&artifacts)?;
    let disk = work.join("target.raw");
    if install {
        sparse(&disk, 40 * 1024u64.pow(3))?;
    }
    let mut result = json!({"firmware":firmware,"media":"usb-storage","iso":iso.file_name().unwrap().to_string_lossy(),"iso_sha256":digest,"install":install,"passed":false,"implementation":"rust-script"});
    let mut vm = Vm::start(
        &work,
        &firmware,
        Some(&iso),
        install.then_some(disk.as_path()),
        install.then_some(repo.as_path()),
    )?;
    let outcome = (|| -> Result<()> {
        println!("Booting {name} from USB mass storage");
        vm.wait_agent()?;
        let end = Instant::now() + Duration::from_secs(180);
        let live = loop {
            match vm.execute("set -e; systemctl is-active display-manager; pgrep -f '^/[^ ]+/bin/[.]?plasmashell'; pgrep -f '^/[^ ]+/bin/[.]?calamares'; nix --version; fh --version; systemctl is-active determinate-nixd.socket; test -s /etc/determinate-installer/flake.lock; lsblk -o NAME,TRAN,FSTYPE,LABEL", SHORT) {
                Ok(text) => break text, Err(e) if Instant::now() >= end => return Err(e), Err(_) => pause(Duration::from_secs(3))?,
            }
        };
        ensure!(
            live.contains("nix (Determinate Nix "),
            "Live Nix is not Determinate"
        );
        println!("{live}");
        result["live"] = json!(live);
        pause(Duration::from_secs(5))?;
        vm.screenshot(&artifacts.join("live-desktop.png"))?;
        if install {
            vm.execute("mkdir -p /workspace; mount -t 9p -o trans=virtio,version=9p2000.L project /workspace", SHORT)?;
            println!("Running the Rust fixture against the real Calamares job");
            // The Nix-built binary has exactly the same Rust implementation as
            // tests/install_in_guest.rs, without runtime compilation/downloads.
            vm.execute(
                &format!(
                    "/run/current-system/sw/bin/respin-tools install-in-guest > {} 2>&1",
                    shell_quote(&format!("/workspace/artifacts/{name}/install.log"))
                ),
                Duration::from_secs(3600),
            )?;
            result["target_before_reboot"] = json!(vm.execute("set -e; cat /mnt/etc/nixos/flake.nix; sha256sum /mnt/etc/nixos/flake.lock; test -L /mnt/nix/var/nix/profiles/system; readlink /mnt/nix/var/nix/profiles/system; sync", SHORT)?);
            vm.execute("umount -R /mnt; sync", SHORT)?;
            vm.poweroff()?;
            fs::copy(work.join("serial.log"), artifacts.join("live-serial.log"))?;
            vm = Vm::start(&work, &firmware, None, Some(&disk), None)?;
            println!("Booting the installed disk with no ISO attached");
            let installed = verify_installed(&mut vm, &artifacts, &repo)?;
            println!("{installed}");
            result["installed"] = json!(installed);
        }
        Ok(())
    })();
    record(vm, &work, &artifacts, &mut result, outcome)
}

pub fn boot_installed(args: Vec<String>) -> Result<()> {
    ensure!(args.len() == 1, "Usage: boot_installed.rs RUN-NAME");
    let name = &args[0];
    ensure!(
        Path::new(name).file_name().and_then(|n| n.to_str()) == Some(name)
            && ![".", ".."].contains(&name.as_str()),
        "Supply a run name, not a path"
    );
    let repo = repo()?;
    let previous = repo.join("artifacts").join(name);
    let previous_work = repo.join(".work").join(name);
    let mut result: Value = serde_json::from_slice(&fs::read(previous.join("result.json"))?)?;
    ensure!(
        result["install"] == true
            && fs::read_to_string(previous.join("install.log"))?
                .trim_end()
                .ends_with("CALAMARES_INSTALL_PASS"),
        "No successful Calamares installation"
    );
    let firmware = result["firmware"]
        .as_str()
        .context("Missing firmware")?
        .to_string();
    ensure!(
        ["bios", "uefi"].contains(&firmware.as_str())
            && result["live"].is_string()
            && result["iso_sha256"].is_string(),
        "Incomplete original evidence"
    );
    let disk = regular(&previous_work.join("target.raw"))?;
    ensure!(
        fs::metadata(&disk)?.len() == 40 * 1024u64.pow(3),
        "Wrong test disk size"
    );
    let name = format!("{name}-boot-{}", stamp());
    let work = repo.join(".work").join(&name);
    let artifacts = repo.join("artifacts").join(&name);
    fs::create_dir(&work)?;
    fs::create_dir(&artifacts)?;
    if firmware == "uefi" {
        fs::copy(
            previous_work.join("OVMF_VARS.fd"),
            work.join("OVMF_VARS.fd"),
        )?;
    }
    for file in ["live-desktop.png", "install.log"] {
        fs::copy(previous.join(file), artifacts.join(file))?;
    }
    result["passed"] = json!(false);
    if let Some(error) = result
        .as_object_mut()
        .context("Invalid original result")?
        .remove("error")
    {
        result["prior_error"] = error;
    }
    result["resumed_from"] = json!(args[0]);
    result["disk_only_boot"] = json!(true);
    result["implementation"] = json!("rust-script");
    let mut vm = Vm::start(&work, &firmware, None, Some(&disk), None)?;
    println!("Booting installed {firmware} disk; no ISO attached");
    let outcome = verify_installed(&mut vm, &artifacts, &repo).map(|text| {
        println!("{text}");
        result["installed"] = json!(text);
    });
    record(vm, &work, &artifacts, &mut result, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_uses_process_status_without_rpc_reply() {
        for success in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("rpc");
            let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
            let server = std::thread::spawn(move || {
                let (socket, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(socket).read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["execute"], "guest-shutdown");
                assert_eq!(request["arguments"]["mode"], "powerdown");
                // An agent shuts down without replying; ordinary RPC EOF
                // remains an error, as checked separately below.
            });
            let mut vm = Vm {
                work: temp.path().to_path_buf(),
                process: Some(Process(
                    Command::new(if success { "true" } else { "false" })
                        .spawn()
                        .unwrap(),
                )),
                agent: Some(Rpc::connect(&path, false, 1).unwrap()),
            };
            assert_eq!(vm.poweroff().is_ok(), success);
            server.join().unwrap();
        }
    }
    #[test]
    fn rpc_skips_events_and_old_ids() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rpc");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(socket.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            writeln!(socket, "{}", json!({"event":"READY"})).unwrap();
            writeln!(socket, "{}", json!({"id":0,"return":"stale"})).unwrap();
            writeln!(socket, "{}", json!({"id":request["id"],"return":"ok"})).unwrap();
        });
        assert_eq!(
            Rpc::connect(&path, false, 1)
                .unwrap()
                .call("ping", json!({}))
                .unwrap(),
            "ok"
        );
        server.join().unwrap();
    }
    #[test]
    fn rpc_eof_is_failure() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rpc");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (_socket, _) = listener.accept().unwrap();
        });
        assert!(
            Rpc::connect(&path, false, 1)
                .unwrap()
                .call("ping", json!({}))
                .is_err()
        );
        server.join().unwrap();
    }
}
