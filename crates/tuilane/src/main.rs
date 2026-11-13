use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "tuilane",
    version,
    about = "A local-first, modular, plugin-driven terminal workspace"
)]
struct Cli;

fn main() {
    let _ = Cli::parse();
    println!("Hello, tuilane!");
}
