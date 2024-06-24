use crate::solarmanv5::SolarmanDevice;

pub struct Inverter {
    device: SolarmanDevice,
}

#[derive(Debug)]
pub struct MonitoringData {
    pub voltage_a: f64,
    pub current_a: f64,
    pub voltage_b: f64,
    pub current_b: f64,
}

impl Inverter {
    pub fn new(
        addr: std::net::IpAddr,
        port: u16,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Self> {
        Ok(Inverter {
            device: SolarmanDevice::new(addr, port, timeout)?,
        })
    }

    pub fn get_data(&mut self) -> anyhow::Result<MonitoringData> {
        let resp_frame = self
            .device
            .send_modbus_frame(&[0x1, 0x3, 0x0, 0x3b, 0x0, 0x36, 0xb4, 0x11])?;

        let voltage_a = ((((resp_frame[103] as u32) << 8) + resp_frame[104] as u32) as f64) / 10.0;
        let current_a = ((((resp_frame[105] as u32) << 8) + resp_frame[106] as u32) as f64) / 10.0;
        let voltage_b = (((resp_frame[107] as u32) << 8) + resp_frame[108] as u32) as f64 / 10.0;
        let current_b = (((resp_frame[109] as u32) << 8) + resp_frame[110] as u32) as f64 / 10.0;

        Ok(MonitoringData {
            voltage_a,
            current_a,
            voltage_b,
            current_b,
        })
    }
}
