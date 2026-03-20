use std::env;
use std::process::ExitCode;

use flowjoish_backend::{capabilities_json, serve};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("describe") => {
            println!("{}", capabilities_json());
            Ok(())
        }
        Some("serve") => {
            let bind_addr = args.get(2).map(String::as_str).unwrap_or("127.0.0.1:8787");
            serve(bind_addr)
        }
        _ => Err("usage: flowjoish-backend <describe|serve> [bind_addr]".to_string()),
    }
}
