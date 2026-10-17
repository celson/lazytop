use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "lazytop",
    version,
    about = "A lazy-style terminal system monitor"
)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
