mod cleanup;
mod cli;
mod condition;
mod doctor;
mod gitignore;
mod input;
mod interpolate;
mod logs;
mod manifest;
mod runner;
mod safety;
mod self_test;
mod skill;
mod store;
mod update_check;

fn main() {
    if let Err(error) = cli::run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
