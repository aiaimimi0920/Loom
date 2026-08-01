use anyhow::Result;

fn main() -> Result<()> {
    loom_plugin_cli::run(std::env::args(), &mut std::io::stdout())
}
