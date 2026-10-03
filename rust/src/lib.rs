pub mod builder;
pub mod config;
#[cfg(feature = "calamares")]
pub mod guest;
pub mod support;
pub mod target;
pub mod vm;

pub fn dispatch(command: &str, args: Vec<String>) -> anyhow::Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        let usage = match command {
            "build-rootless" => "BOOTSTRAP_ISO [--sha256 HEX]",
            "qemu-test" => "ISO [--firmware bios|uefi] [--install]",
            "boot-installed" => "RUN-NAME",
            "prepare-target" => "ROOT HOSTNAME [TEMPLATE_DIR]",
            "test-handoff" => "TEMPLATE LOCK PATCHED_MAIN",
            "check-config" => "generated|defaults FILE FILE",
            _ => "(run only inside the disposable guest)",
        };
        println!("Usage: {command} {usage}");
        return Ok(());
    }
    support::init_signals()?;
    match command {
        "build-rootless" => builder::main(args),
        "prepare-builder" => builder::prepare(),
        "build-iso" => builder::build_iso(),
        "qemu-test" => vm::main(args),
        "boot-installed" => vm::boot_installed(args),
        "prepare-target" => target::main(args),
        "check-config" => config::main(args),
        #[cfg(feature = "calamares")]
        "install-in-guest" => guest::install(),
        #[cfg(feature = "calamares")]
        "test-handoff" => guest::test_handoff(args),
        _ => anyhow::bail!("Unknown command or missing 'calamares' feature: {command}"),
    }
}
