//! Command-line entry point for the host QA controller.

fn main() {
    if let Err(error) = ph_esp32_mac_qa_host::run_cli() {
        eprintln!("qa-host: {error:#}");
        std::process::exit(1);
    }
}
