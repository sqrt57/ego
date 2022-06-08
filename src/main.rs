use ego::cli;
use std::env;

fn main() {
    let args = env::args();
    let scenario = cli::parse(args);
    scenario.execute();
}
