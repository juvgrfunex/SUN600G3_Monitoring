#![forbid(unsafe_code)]
#![warn(
    clippy::dbg_macro,
    clippy::decimal_literal_representation,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_in_result,
    clippy::unwrap_used,
    clippy::use_debug
)]


use anyhow::Context;
use inverter::Inverter;
use rinfluxdb::line_protocol::blocking::Client;
use rinfluxdb::line_protocol::LineBuilder;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, str::FromStr};

mod inverter;
mod solarmanv5;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct InverterConfig {
    #[serde(default = "default_inverter_ip")]
    ip: std::net::IpAddr,
    #[serde(default = "default_inverter_port")]
    port: u16,
    #[serde(default = "default_inverter_location")]
    location: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MonitoringConfig {
    influx_ip: std::net::IpAddr,
    #[serde(default = "default_influx_port")]
    influx_port: u16,
    #[serde(default = "default_database_name")]
    database: String,
    #[serde(default = "default_measurement_name")]
    measurement: String,
    #[serde(default = "default_monitoring_intervall")]
    intervall_secs: u32,
    #[serde(default = "default_monitoring_timeout")]
    timeout_secs: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    monitoring: MonitoringConfig,
    inverter: HashMap<String, InverterConfig>,
    #[serde(default = "default_log_level")]
    log_level: String,
}

fn default_inverter_location() -> String {
    "no_location".to_owned()
}

fn default_inverter_ip() -> std::net::IpAddr {
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 10, 100, 254))
}

fn default_inverter_port() -> u16 {
    8899
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_measurement_name() -> String {
    "deye_dual".to_string()
}
fn default_database_name() -> String {
    "solar".to_string()
}
fn default_influx_port() -> u16 {
    8086
}
fn default_monitoring_intervall() -> u32 {
    300
}

fn default_monitoring_timeout() -> u32 {
    10
}

fn run_monitoring(
    inverter_name: String,
    inverter_cfg: InverterConfig,
    monitoring_config: MonitoringConfig,
) -> anyhow::Result<()>{
    let mut inverter = loop {
        match Inverter::new(
            inverter_cfg.ip,
            inverter_cfg.port,
            std::time::Duration::from_secs(monitoring_config.timeout_secs.into()),
        ) {
            Ok(inv) => break inv,
            Err(e) => log::debug!("[{inverter_name}] Failed to connect ({e})"),
        }

    };
    let sleep_dur = std::time::Duration::from_secs(monitoring_config.intervall_secs.into());
    let client = loop {
        if let Ok(client) = Client::new::<String, String>(
            reqwest::Url::parse(&format!(
                "http://{}:{}",
                monitoring_config.influx_ip, monitoring_config.influx_port
            ))
            .context("Influxdb ip or port invalid")?,
            None,
        ) {
            break client;
        }
    };
    loop {
        let data = match inverter.get_data() {
            Ok(data) => {
                log::debug!("[{inverter_name}] Recieved data: {data:#?}");
                data
            }
            Err(e) => {
                log::debug!("[{inverter_name}] Failed to recieve data ({e})");
                std::thread::sleep(sleep_dur);
                continue;
            }
        };

	let power_a = data.voltage_a * data.current_a;
	let power_b = data.voltage_b * data.current_b;
        let lines = vec![
            LineBuilder::new(inverter_cfg.location.clone())
                .insert_field("voltage", data.voltage_a)
                .insert_field("current", data.current_a)
		.insert_field("power", power_a)
                .insert_tag("inverter", inverter_name.clone())
                .insert_tag("input", "A")
                .build(),
            LineBuilder::new(inverter_cfg.location.clone())
                .insert_field("voltage", data.voltage_b)
                .insert_field("current", data.current_b)
		.insert_field("power", power_b)
                .insert_tag("inverter", inverter_name.clone())
                .insert_tag("input", "B")
                .build(),
        ];

        if client.send(&monitoring_config.database, &lines).is_err() {
            log::error!("[{inverter_name}] Failed to store data in database");
        }
        std::thread::sleep(sleep_dur)
    }
}
fn main() -> anyhow::Result<()> {
    let config_str =
        std::fs::read_to_string("config.toml").context("Failed to read config file.")?;

    let config: Config = toml::from_str(&config_str).context("Failed to parse config file.")?;
    simple_logger::init_with_level(log::Level::from_str(&config.log_level)?)
        .context("Failed to init logging")?;
    let mut handles = Vec::new();
    for inverter_cfg in config.inverter {
        let mon_cfg = config.monitoring.clone();
        handles.push(std::thread::spawn(move || {
            run_monitoring(inverter_cfg.0, inverter_cfg.1, mon_cfg)
        }));
    }

    for handle in handles {
        if let Err(e) = handle.join().expect("monitoring threads do not panic"){
            log::error!("Thread exited unexpectedly: {e}");
        }
    }

    Ok(())
}
