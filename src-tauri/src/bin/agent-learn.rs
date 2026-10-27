use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!(
            "usage: agent-learn <data-dir> <device-serial> <package> [max-cycles] [steps-per-cycle] [stagnant-cycles]"
        );
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
    let max_cycles = args
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(8);
    let steps_per_cycle = args
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(45);
    let stagnant_cycles = args
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(2);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create tokio runtime");
    let result = runtime.block_on(ai_apptest_agent_lib::run_headless_learning(
        data_dir,
        device_serial,
        package,
        max_cycles,
        steps_per_cycle,
        stagnant_cycles,
    ));
    match result {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        ),
        Err(error) => {
            eprintln!("learning loop failed: {error}");
            std::process::exit(1);
        }
    }
}
