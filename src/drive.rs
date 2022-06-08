use super::code;
use super::vm::Vm;

#[derive(Debug)]
pub enum Command {
    RunString(String),
}

#[derive(Debug)]
pub struct Scenario {
    pub commands: Vec<Command>,
}

impl Scenario {
    pub fn execute(&self) {
        let mut vm = Vm::new();
        for command in self.commands.iter() {
            match command {
                Command::RunString(string) => code::run(&mut vm, string),
            }
        }
    }
}
