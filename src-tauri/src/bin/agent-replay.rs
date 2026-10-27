use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: agent-replay <data-dir> <device-serial> <package> <test-id>");
        std::process::exit(2);
    }));
    let device_serial = args.next().unwrap_or_else(|| {
        eprintln!("missing device serial");
        std::process::exit(2);
    });
    let package = args.next().unwrap_or_else(|| {
        eprintln!("missing package");
        std::process::exit(2);
    });
    let test_id = args.next().unwrap_or_else(|| {
        eprintln!("missing test id");
        std::process::exit(2);
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create tokio runtime");
    let result = runtime.block_on(ai_apptest_agent_lib::run_headless_test(
        data_dir,
        device_serial,
        package,
        test_id,
    ));
    match result {
        Ok(result) => println!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        ),
        Err(error) => {
            eprintln!("replay failed: {error}");
            std::process::exit(1);
        }
    }
}
