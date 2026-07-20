use anyhow::Result;
use loom_cli::run_cli_with_writer;

fn main() -> Result<()> {
    run_cli_with_writer(std::env::args(), &mut std::io::stdout())
}
