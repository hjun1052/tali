mod cli;
mod condition;
mod doctor;
mod input;
mod interpolate;
mod logs;
mod manifest;
mod runner;
mod safety;
mod self_test;
mod store;

fn main() {
    if let Err(error) = cli::run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
