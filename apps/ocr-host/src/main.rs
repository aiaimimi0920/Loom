mod code_exclusion;
mod code_overlay;
mod code_scan;
mod command_options;
mod commands;
mod copy_commands;
mod frame;
mod layout_text;
mod overlay;
mod overlay_color;
mod overlay_typography;
mod runtime;

#[cfg(test)]
mod manifest_tests;
#[cfg(test)]
mod normalized_text_tests;
#[cfg(test)]
mod tests;

fn main() {
    if let Err(error) = runtime::run() {
        eprintln!("ocr-host terminated: {error}");
        std::process::exit(1);
    }
}
