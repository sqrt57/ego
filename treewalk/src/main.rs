use std::io::{self, BufRead, Write};
use std::rc::Rc;

use treewalk::bootstrap::{bootstrap, Interpreter};
use treewalk::error::SourceSpan;
use treewalk::eval::{eval_send, eval_source_print, eval_source_run, EgoSignal};
use treewalk::object::ObjectKind;

enum Fragment {
    Eval(String),
    File(String),
}

enum CliMode {
    Repl,
    Fragments(Vec<Fragment>),
    Version,
    Help,
    BadArgs(String),
}

fn parse_args(args: &[String]) -> CliMode {
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut repl = false;
    let mut version = false;
    let mut help = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--repl" => { repl = true; i += 1; }
            "--version" => { version = true; i += 1; }
            "--help" => { help = true; i += 1; }
            "-e" | "--eval" => {
                if i + 1 >= args.len() {
                    return CliMode::BadArgs(format!("{}: requires an argument", args[i]));
                }
                fragments.push(Fragment::Eval(args[i + 1].clone()));
                i += 2;
            }
            s if s.starts_with("--eval=") => {
                fragments.push(Fragment::Eval(s["--eval=".len()..].to_string()));
                i += 1;
            }
            s if s.starts_with('-') => {
                return CliMode::BadArgs(format!("unknown option: {s}"));
            }
            s => {
                fragments.push(Fragment::File(s.to_string()));
                i += 1;
            }
        }
    }

    match (repl, version, help, fragments.is_empty()) {
        (true, false, false, true) => CliMode::Repl,
        (false, true, false, true) => CliMode::Version,
        (false, false, true, true) => CliMode::Help,
        (false, false, false, false) => CliMode::Fragments(fragments),
        (false, false, false, true) => CliMode::BadArgs("no arguments given".to_string()),
        _ => CliMode::BadArgs("conflicting options".to_string()),
    }
}

/// Formats an uncaught exception (no `on:Do:` matched it — lang-spec.md
/// §10) by reading its `messageText`, the same slot `signal:` sets, and
/// reusing `EgoError`'s `file:line:column: error: ...` rendering for `span`.
fn exception_error(exc_obj: treewalk::arena::ObjectId, span: SourceSpan, interp: &mut Interpreter) -> treewalk::error::EgoError {
    let lookup_span = SourceSpan::new(Rc::new("<error>".to_string()), 0, 0);
    let text = match eval_send(exc_obj, "messageText", &[], &lookup_span, interp) {
        Ok(id) => match &interp.arena.get(id).kind {
            ObjectKind::StringVal(s) => s.to_string(),
            _ => "an exception".to_string(),
        },
        Err(_) => "an exception".to_string(),
    };
    treewalk::error::EgoError::new(span, text)
}

fn print_signal(sig: EgoSignal, interp: &mut Interpreter) {
    match sig {
        EgoSignal::Err(e) => eprintln!("{e}"),
        EgoSignal::Exception(exc_obj, span) => eprintln!("{}", exception_error(exc_obj, span, interp)),
        EgoSignal::NonLocalReturn(_, _) => eprintln!("Non-local return escaped activation"),
        EgoSignal::HandlerUnwind(_, _) => eprintln!("exception handler escape leaked to top level"),
        EgoSignal::Resume(_) => eprintln!("exception resume leaked to top level"),
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
                    Err(sig) => print_signal(sig, interp),
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

    match mode {
        CliMode::BadArgs(msg) => {
            eprintln!("ego: {msg}");
            eprintln!("Usage: ego --repl | -e EXPR [-e EXPR ...] | FILE [FILE ...]");
            std::process::exit(2);
        }
        CliMode::Version => {
            println!("ego {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        CliMode::Help => {
            println!("Usage: ego --repl | -e EXPR [-e EXPR ...] | FILE [FILE ...]");
            println!();
            println!("Options:");
            println!("  --repl            Start interactive REPL");
            println!("  -e, --eval EXPR   Evaluate expression (may be repeated)");
            println!("  --version         Print version and exit");
            println!("  --help            Print this help and exit");
            return;
        }
        _ => {}
    }

    let mut interp = match bootstrap() {
        Ok(i) => i,
        Err(e) => { eprintln!("{e}"); std::process::exit(1); }
    };

    match mode {
        CliMode::BadArgs(_) | CliMode::Version | CliMode::Help => unreachable!(),
        CliMode::Repl => run_repl(&mut interp),
        CliMode::Fragments(fragments) => {
            // Mixing in a file switches the whole invocation to script rules:
            // no fragment auto-prints, not even `-e` ones (cli.md "Mixed eval
            // and files"). With no files, each `-e` fragment prints its own
            // final expression, matching the plain inline-eval mode.
            let has_files = fragments.iter().any(|f| matches!(f, Fragment::File(_)));

            for fragment in fragments {
                match fragment {
                    Fragment::Eval(code) => {
                        if has_files {
                            if let Err(sig) = eval_source_run(&code, "<eval>", &mut interp) {
                                print_signal(sig, &mut interp);
                                std::process::exit(1);
                            }
                        } else {
                            match eval_source_print(&code, "<eval>", &mut interp) {
                                Ok(Some(s)) => println!("{s}"),
                                Ok(None) => {}
                                Err(sig) => { print_signal(sig, &mut interp); std::process::exit(1); }
                            }
                        }
                    }
                    Fragment::File(path) => {
                        let src = match std::fs::read_to_string(&path) {
                            Ok(s) => s,
                            Err(e) => { eprintln!("{path}: {e}"); std::process::exit(1); }
                        };
                        if let Err(sig) = eval_source_run(&src, &path, &mut interp) {
                            print_signal(sig, &mut interp);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    }
}
