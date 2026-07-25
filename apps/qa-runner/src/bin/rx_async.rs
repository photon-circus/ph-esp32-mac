//! Dedicated raw-async RX-exhaustion release validation firmware.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::main;

esp_app_desc!();

#[main]
fn main() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    ph_esp32_mac_qa_runner::run_blocked_suite("rx-async", "rx-async.exhausted")
}
