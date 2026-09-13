mod datetime;
mod parser;

use datetime::DateTimeValue;
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut path: Option<String> = None;
    let mut at: Option<String> = None;
    let mut show_all = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--at" => {
                i += 1;
                match args.get(i) {
                    Some(v) => at = Some(v.clone()),
                    None => {
                        eprintln!("--at requires a value, e.g. --at 20250115T090000Z");
                        return ExitCode::from(2);
                    }
                }
            }
            "--all" => show_all = true,
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => {
                if path.is_none() && !other.starts_with('-') {
                    path = Some(other.to_string());
                } else {
                    eprintln!("unrecognized argument: {}", other);
                    print_usage();
                    return ExitCode::from(2);
                }
            }
        }
        i += 1;
    }

    let path = match path {
        Some(p) => p,
        None => {
            print_usage();
            return ExitCode::from(2);
        }
    };

    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to read {}: {}", path, e);
            return ExitCode::from(1);
        }
    };

    let reference = match &at {
        Some(raw) => match DateTimeValue::parse(raw) {
            Ok(dt) => dt,
            Err(msg) => {
                eprintln!("invalid --at value \"{}\": {}", raw, msg);
                return ExitCode::from(2);
            }
        },
        None => datetime::now_utc(),
    };

    let events = match parser::parse_events(&contents) {
        Ok(events) => events,
        Err(e) => {
            eprintln!("{}: {}", path, e);
            return ExitCode::from(1);
        }
    };

    let mut upcoming: Vec<&parser::VEvent> =
        events.iter().filter(|e| e.start >= reference).collect();
    upcoming.sort_by_key(|e| e.start);

    if upcoming.is_empty() {
        println!("no upcoming events found in {}", path);
        return ExitCode::SUCCESS;
    }

    if show_all {
        for event in &upcoming {
            println!("{}  {}", event.start, event.summary);
        }
    } else {
        let next = upcoming[0];
        println!("{}  {}", next.start, next.summary);
    }

    ExitCode::SUCCESS
}

fn print_usage() {
    eprintln!("usage: ics-next <file.ics> [--at YYYYMMDDTHHMMSSZ] [--all]");
    eprintln!();
    eprintln!("Prints the next upcoming VEVENT in an .ics file, compared against now (UTC)");
    eprintln!("unless --at gives a reference time. --all lists every upcoming event, soonest first.");
}
