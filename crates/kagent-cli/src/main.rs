fn main() {
    let command = std::env::args().nth(1).unwrap_or_else(|| "dash".to_owned());

    match command.as_str() {
        "dash" => {
            println!("kagent dashboard skeleton");
        }
        "context" => {
            println!("kagent context skeleton");
        }
        _ => {
            eprintln!("unknown command: {command}");
            std::process::exit(2);
        }
    }
}
