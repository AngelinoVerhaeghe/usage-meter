// No console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|a| a == "--print") {
        return usage_meter_lib::print_report();
    }
    usage_meter_lib::run()
}
