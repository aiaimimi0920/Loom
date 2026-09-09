mod command;
mod frame;
mod runtime;
mod translation;

fn main() {
    if let Err(error) = runtime::run() {
        eprintln!("text-translation-host terminated: {error}");
        std::process::exit(1);
    }
}
