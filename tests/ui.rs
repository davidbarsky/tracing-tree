use ui_test::{color_eyre::Result, run_tests, Config, Mode, OutputConflictHandling};

fn main() -> Result<()> {
    let mut config = Config::default();
    config.root_dir = "examples".into();
    config.dependencies_crate_manifest_path = Some("test_dependencies/Cargo.toml".into());
    config.args.push("--cfg".into());
    config.args.push("feature=\"tracing-log\"".into());
    config.out_dir = Some("target/ui_test".into());
    config.mode = Mode::Run { exit_code: 0 };
    config.stdout_filter("[0-9]{3}(ms|s|m)", "  X$1");
    config.stdout_filter("[0-9]{2}(ms|s|m)", " X$1");
    config.stdout_filter("[0-9]{1}(ms|s|m)", "X$1");
    config.stderr_filter("[0-9]{3}(ms|s|m)", "  X$1");
    config.stderr_filter("[0-9]{2}(ms|s|m)", " X$1");
    config.stderr_filter("[0-9]{1}(ms|s|m)", "X$1");
    config.output_conflict_handling = if std::env::args().any(|arg| arg == "--bless") {
        OutputConflictHandling::Bless
    } else {
        OutputConflictHandling::Error
    };
    run_tests(config)
}
