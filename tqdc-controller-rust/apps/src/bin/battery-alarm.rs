use chrono::Local;

fn main() -> Result<(), battery::Error> {
    let manager = battery::Manager::new()?;
    loop {
        let battery = manager.batteries()?.next().unwrap()?;
        if battery.state() == battery::State::Discharging {
            let failure_timestamp = Local::now().naive_local();
            println!("Alert: power failure detected {failure_timestamp}!!!");
            loop {
                std::process::Command::new("speaker-test")
                    .args(["-t", "sine", "-f", "1000", "-l", "1"])
                    .output()
                    .unwrap();
            }
        }
    }
}