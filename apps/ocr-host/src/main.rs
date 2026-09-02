mod code_exclusion;
mod code_overlay;
mod code_scan;
mod commands;
mod frame;
mod overlay;
mod overlay_color;
mod overlay_typography;
mod runtime;

#[cfg(test)]
mod tests;

fn main() {
    if let Err(error) = runtime::run() {
        eprintln!("ocr-host terminated: {error}");
        std::process::exit(1);
    }
}
