//! The quality gate, run on its own.
//!
//!     antilib-qa            # run every check and print the table
//!     antilib-qa --quiet    # print only the total and the failures
//!
//! Exits non-zero if anything failed, so it can stand in a build script.

fn main() {
    let quiet = std::env::args().any(|a| a == "--quiet" || a == "-q");
    let gate = anti_library::qa::run();
    if quiet {
        println!("{} checks, {} failed", gate.checks, gate.failures.len());
        for f in &gate.failures {
            println!(" - {f}");
        }
    } else {
        anti_library::qa::report(&gate);
    }
    if !gate.passed() {
        std::process::exit(1);
    }
}
