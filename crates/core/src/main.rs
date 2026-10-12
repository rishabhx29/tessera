//! The `tessera` binary: engine entry point (see docs/usage once it exists;
//! today: `tessera index <folder>`, `tessera search <terms>`, `tessera stats`).

fn main() -> std::process::ExitCode {
    match tessera_core::cli::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("tessera: {err:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
