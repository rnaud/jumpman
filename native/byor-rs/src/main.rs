fn main() {
    let code = byor_rs::cli::run(std::env::args().skip(1).collect(), &mut std::io::stdout());
    std::process::exit(code);
}
