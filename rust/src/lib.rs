pub mod builder;
pub mod config;
pub mod gui_input;
pub mod support;
pub mod vm;

pub fn dispatch(command: &str, args: Vec<String>) -> anyhow::Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        let usage = match command {
            "build-rootless" => "BOOTSTRAP_ISO [--sha256 HEX] [--rebuild-iso]",
            "qemu-test" => "ISO [--firmware bios|uefi] [--install] [--gui]",
            "qmp-input" => "RUN-NAME screenshot|key|type|click [VALUES]",
            "boot-installed" => "RUN-NAME",
            "check-config" => "template|settings FILE FILE",
            _ => "(run only inside the disposable guest)",
        };
        println!("Usage: {command} {usage}");
        return Ok(());
    }
    support::init_signals()?;
    match command {
        "build-rootless" => builder::main(args),
        "prepare-builder" => builder::prepare(),
        "build-iso" => {
            anyhow::ensure!(
                args.is_empty() || args == ["--rebuild"],
                "Usage: build-iso [--rebuild]"
            );
            builder::build_iso(!args.is_empty())
        }
        "qemu-test" => vm::main(args),
        "qmp-input" => gui_input::main(args),
        "boot-installed" => vm::boot_installed(args),
        "check-config" => config::main(args),
        _ => anyhow::bail!("Unknown command: {command}"),
    }
}
