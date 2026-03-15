//! Generate reference files from composition fragments.
//!
//! Usage: generate-refs <config.toml> [ticks] [dt]

use std::fs;

use fpa_testkit::reference::ReferenceFile;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: generate-refs <config.toml> [ticks] [dt]");
        std::process::exit(1);
    }

    let config_path = &args[1];
    let ticks: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    let dt: f64 = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0 / 60.0);

    let config_str = fs::read_to_string(config_path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", config_path, e);
        std::process::exit(1);
    });

    let fragment = fpa_config::load_from_str(&config_str).unwrap_or_else(|e| {
        eprintln!("Failed to parse {}: {}", config_path, e);
        std::process::exit(1);
    });

    let registry = fpa_testkit::registry::with_all_test_partitions();
    let reference = ReferenceFile::generate(&fragment, &registry, ticks, dt).unwrap_or_else(|e| {
        eprintln!("Failed to generate reference: {}", e);
        std::process::exit(1);
    });

    let output = reference.to_toml_string().unwrap_or_else(|e| {
        eprintln!("Failed to serialize reference: {}", e);
        std::process::exit(1);
    });

    println!("{}", output);
}
