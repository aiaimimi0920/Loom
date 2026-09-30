mod command;
mod frame;
mod model_client;
mod runtime;
mod translation;
mod translation_input;
mod translation_paragraphs;
mod translation_scene;
mod translation_typography;

#[cfg(test)]
mod manifest_tests;
#[cfg(test)]
mod translation_paragraph_tests;
#[cfg(test)]
mod translation_tests;

fn main() {
    if let Err(error) = runtime::run() {
        eprintln!("text-translation-host terminated: {error}");
        std::process::exit(1);
    }
}
