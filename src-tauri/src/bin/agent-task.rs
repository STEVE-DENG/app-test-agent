use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: agent-task <data-dir> <device-serial> <instruction>");
        std::process::exit(2);
    }));
    let device_serial = args.next().unwrap_or_else(|| {
        eprintln!("missing device serial");
        std::process::exit(2);
    });
    let instruction = args.collect::<Vec<_>>().join(" ");
    if instruction.trim().is_empty() {
        eprintln!("missing instruction");
        std::process::exit(2);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create tokio runtime");
    let result = runtime.block_on(ai_apptest_agent_lib::run_headless_task(
        data_dir,
        device_serial,
        instruction,
        false,
    ));
    match result {
        Ok(result) => println!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        ),
        Err(error) => {
            eprintln!("task failed: {error}");
            std::process::exit(1);
        }
    }
}
