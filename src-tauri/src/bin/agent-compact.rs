use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: agent-compact <data-dir> <package>");
        std::process::exit(2);
    }));
    let package = args.next().unwrap_or_else(|| {
        eprintln!("missing package");
        std::process::exit(2);
    });

    match ai_apptest_agent_lib::run_headless_compact(data_dir, package) {
        Ok(summary) => println!(
            "{}",
            serde_json::to_string_pretty(&summary).unwrap_or_default()
        ),
        Err(error) => {
            eprintln!("compact failed: {error}");
            std::process::exit(1);
        }
    }
}
