fn main() {
    if let Err(err) = app::run() {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
