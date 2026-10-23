//! `driver-check` — run acceptance profiles against a driver package, or
//! inventory installed packages for migration.
//!
//! Usage:
//!   `driver-check --package <dir> [--profile <name>]...`
//!   `driver-check inventory --data-dir <dir> [--strict]`
//!
//! The profile mode prints the JSON report plus a one-line summary and exits 1
//! when acceptance fails. Inventory is informational and exits 0 even when it
//! finds v1/invalid packages; `--strict` makes those exit 1.

mod inventory;

use std::path::PathBuf;
use std::process::ExitCode;

use tinyiothub_runtime::acceptance::runner::{PROFILE_NAMES, run_profiles};

fn usage() -> ! {
    eprintln!("usage: driver-check --package <dir> [--profile <name>]...");
    eprintln!("       driver-check inventory --data-dir <dir> [--strict]");
    eprintln!("profiles: {}", PROFILE_NAMES.join(", "));
    std::process::exit(2);
}

fn inventory_usage() -> ! {
    eprintln!("usage: driver-check inventory --data-dir <dir> [--strict]");
    std::process::exit(2);
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("inventory") {
        return run_inventory(&args[1..]);
    }
    run_check(args)
}

fn run_check(args: Vec<String>) -> ExitCode {
    let mut package: Option<PathBuf> = None;
    let mut profiles: Vec<String> = Vec::new();

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--package" | "-p" => package = Some(PathBuf::from(args.next().unwrap_or_else(|| usage()))),
            "--profile" => profiles.push(args.next().unwrap_or_else(|| usage())),
            "-h" | "--help" => {
                println!("usage: driver-check --package <dir> [--profile <name>]...");
                println!("       driver-check inventory --data-dir <dir> [--strict]");
                println!("profiles: {}", PROFILE_NAMES.join(", "));
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
    }

    let Some(package) = package else {
        eprintln!("--package is required");
        usage();
    };
    if profiles.is_empty() {
        profiles.push("all".into());
    }
    if let Some(bad) = profiles.iter().find(|p| !PROFILE_NAMES.contains(&p.as_str())) {
        eprintln!("unknown profile: {bad} (expected one of: {})", PROFILE_NAMES.join(", "));
        return ExitCode::from(2);
    }

    let selected: Vec<&str> = profiles.iter().map(String::as_str).collect();
    let report = run_profiles(&package, &selected);

    println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    let verdict = if report.passed() { "PASS" } else { "FAIL" };
    let summary: Vec<String> = report
        .profiles
        .iter()
        .map(|p| format!("{}={}", p.profile, p.level()))
        .collect();
    println!(
        "driver-check: {}@{} -> {} ({} profile(s): {})",
        report.driver,
        report.version,
        verdict,
        report.profiles.len(),
        summary.join(", ")
    );

    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_inventory(args: &[String]) -> ExitCode {
    let mut data_dir: Option<PathBuf> = None;
    let mut strict = false;

    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => data_dir = Some(PathBuf::from(args.next().unwrap_or_else(|| inventory_usage()))),
            "--strict" => strict = true,
            "-h" | "--help" => {
                println!("usage: driver-check inventory --data-dir <dir> [--strict]");
                println!("Lists built-in manifests and installed driver packages (manifest/binary hashes, ABI).");
                println!("Informational: exits 0 when problems are found unless --strict is set.");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                inventory_usage();
            }
        }
    }

    let Some(data_dir) = data_dir else {
        eprintln!("--data-dir is required");
        inventory_usage();
    };

    let report = inventory::collect(&data_dir);
    println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    println!("{}", report.summary());

    if strict && report.has_problems() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
