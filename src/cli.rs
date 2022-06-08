use super::drive;
use super::drive::Command;
use std::env;

pub fn parse(mut args: env::Args) -> drive::Scenario {
    args.next();
    drive::Scenario {
        commands: args.map(|arg| Command::RunString(arg)).collect(),
    }
}
