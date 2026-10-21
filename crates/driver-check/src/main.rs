//! `driver-check` — run acceptance profiles against a driver package.
//!
//! Usage: `driver-check --package <dir> [--profile <name>]...`
//! Prints the JSON report plus a one-line summary; exits 1 when acceptance
//! fails.

use std::path::PathBuf;
use std::process::ExitCode;

use tinyiothub_runtime::acceptance::runner::{PROFILE_NAMES, run_profiles};

fn usage() -> ! {
    eprintln!("usage: driver-check --package <dir> [--profile <name>]...");
    eprintln!("profiles: {}", PROFILE_NAMES.join(", "));
    std::process::exit(2);
}

fn main() -> ExitCode {
    let mut package: Option<PathBuf> = None;
    let mut profiles: Vec<String> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--package" | "-p" => package = Some(PathBuf::from(args.next().unwrap_or_else(|| usage()))),
            "--profile" => profiles.push(args.next().unwrap_or_else(|| usage())),
            "-h" | "--help" => {
                println!("usage: driver-check --package <dir> [--profile <name>]...");
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
