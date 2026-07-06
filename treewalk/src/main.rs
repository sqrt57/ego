use std::io::{self, BufRead, Write};

use treewalk::bootstrap::bootstrap;
use treewalk::eval::{eval_source_print, eval_source_run, EgoSignal};

enum CliMode {
    Repl,
    Eval(String),
    Script(String),
    BadArgs,
}

fn parse_args(args: &[String]) -> CliMode {
    match args {
        [] => CliMode::BadArgs,
        [flag] if flag == "--repl" => CliMode::Repl,
        [flag, code] if flag == "-e" => CliMode::Eval(code.clone()),
        [path] if !path.starts_with('-') => CliMode::Script(path.clone()),
        _ => CliMode::BadArgs,
    }
}

fn print_signal(sig: EgoSignal) {
    match sig {
        EgoSignal::Err(e) => eprintln!("{e}"),
        EgoSignal::Exception(_) => eprintln!("Exception raised"),
        EgoSignal::NonLocalReturn(_, _) => eprintln!("Non-local return escaped activation"),
    }
}

/// Returns true when `s` is syntactically complete (depth zero, no open string).
fn input_complete(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next(); // escaped ''
                } else {
                    in_string = false;
                }
            }
        } else {
            match c {
                '\'' => in_string = true,
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                '"' => {
                    // skip ego comment
                    loop {
                        match chars.next() {
                            Some('"') => {
                                if chars.peek() == Some(&'"') {
                                    chars.next(); // "" in comment
                                } else {
                                    break;
                                }
                            }
                            None => break,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
    depth <= 0 && !in_string
}

fn run_repl(interp: &mut treewalk::bootstrap::Interpreter) {
    let stdin = io::stdin();
    let mut input = String::new();

    print!("ego> ");
    io::stdout().flush().ok();

    for line in stdin.lock().lines() {
        let line = line.unwrap_or_default();
        input.push_str(&line);
        input.push('\n');

        if input_complete(&input) {
            let trimmed = input.trim().to_string();
            if !trimmed.is_empty() {
                match eval_source_print(&trimmed, "<repl>", interp) {
                    Ok(Some(s)) => println!("{s}"),
                    Ok(None) => {}
                    Err(sig) => print_signal(sig),
                }
            }
            input.clear();
            print!("\nego> ");
        } else {
            print!("...   ");
        }
        io::stdout().flush().ok();
    }
    println!();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = parse_args(&args);

    if matches!(mode, CliMode::BadArgs) {
        eprintln!("Usage: ego --repl | -e EXPR | FILE");
        std::process::exit(2);
    }

    let mut interp = match bootstrap() {
        Ok(i) => i,
        Err(e) => { eprintln!("{e}"); std::process::exit(1); }
    };

    match mode {
        CliMode::BadArgs => unreachable!(),
        CliMode::Repl => run_repl(&mut interp),
        CliMode::Eval(code) => {
            match eval_source_print(&code, "<eval>", &mut interp) {
                Ok(Some(s)) => println!("{s}"),
                Ok(None) => {}
                Err(sig) => { print_signal(sig); std::process::exit(1); }
            }
        }
        CliMode::Script(path) => {
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => { eprintln!("{path}: {e}"); std::process::exit(1); }
            };
            if let Err(sig) = eval_source_run(&src, &path, &mut interp) {
                print_signal(sig);
                std::process::exit(1);
            }
        }
    }
}
