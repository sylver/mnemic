use clap::Parser;

#[derive(Parser)]
#[command(name = "mnemic", version, about = "Mnemic")]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
