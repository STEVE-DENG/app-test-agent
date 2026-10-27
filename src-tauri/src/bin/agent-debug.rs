use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: agent-debug <data-dir> <device-serial> [package]");
        std::process::exit(2);
    }));
    let device_serial = args.next().unwrap_or_else(|| {
        eprintln!("missing device serial");
        std::process::exit(2);
    });
    let package = args.next();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create tokio runtime");
    let result = runtime.block_on(ai_apptest_agent_lib::run_headless_debug(
        data_dir,
        device_serial,
        package,
    ));
    match result {
        Ok(snapshot) => println!(
            "{}",
            serde_json::to_string_pretty(&snapshot).unwrap_or_default()
        ),
        Err(error) => {
            eprintln!("debug snapshot failed: {error}");
            std::process::exit(1);
        }
    }
}
