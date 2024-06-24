use std::io::prelude::*;
use std::net::{SocketAddr, TcpStream};
use anyhow::Context;

pub(crate) struct SolarmanDevice {
    addr: std::net::IpAddr,
    port: u16,
    timeout: std::time::Duration,
    logger_serial: [u8; 4],
}

impl SolarmanDevice {
    pub(crate) fn new(
        addr: std::net::IpAddr,
        port: u16,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Self> {
        let mut device = SolarmanDevice {
            addr,
            port,
            timeout,
            logger_serial: [0; 4],
        };
        device.detect_serial()?;
        Ok(device)
    }

    fn create_connection(&self) -> anyhow::Result<std::net::TcpStream> {
        let stream =
            TcpStream::connect_timeout(&SocketAddr::new(self.addr, self.port), self.timeout)?;
        stream.set_read_timeout(Some(self.timeout)).context("Failed to set read timeout")?;
        stream.set_write_timeout(Some(self.timeout)).context("failed to set write timeout")?;
        Ok(stream)
    }

    fn detect_serial(&mut self) -> anyhow::Result<()> {
        let mut connection = self.create_connection()?;
        connection.write_all(
            &Request {
                header: RequestHeader {
                    msg_id: 0,
                    logger_serial: self.logger_serial,
                },
                payload: RequestPayload {
                    frame_type: RequestFrameType::SolarInverter,
                    sensor_type: 0,
                    total_working_second: 0,
                    uptime_second: 0,
                    offset_seconds: 0,
                    modbus_rtu_frame: &[],
                },
            }
            .to_bytes(),
        )?;

        let mut response_buffer = [0; 29];
	connection.read_exact(&mut response_buffer).context("Failed reading serial detection response")?;
	let response = Response::from_bytes(&response_buffer);
        self.logger_serial = response.header.logger_serial;
        Ok(())
    }

    pub(crate) fn send_modbus_frame(&mut self, frame: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut connection = self.create_connection()?;
        let request = Request {
            header: RequestHeader {
                msg_id: 0,
                logger_serial: self.logger_serial,
            },
            payload: RequestPayload {
                frame_type: RequestFrameType::SolarInverter,
                sensor_type: 0,
                total_working_second: 0,
                uptime_second: 0,
                offset_seconds: 0,
                modbus_rtu_frame: frame,
            },
        };
        log::debug!("Sending Request: {request:?}");
        connection.write_all(&request.to_bytes())?;

        let mut response_buffer = [0; 140];
        connection.read_exact(&mut response_buffer)?;

        let response = Response::from_bytes(&response_buffer);
        log::debug!("Recieved Response: {response:?}");
        Ok(response.payload.rtu_frame)
    }
}

#[derive(Debug)]
struct Request<'a> {
    header: RequestHeader,
    payload: RequestPayload<'a>,
}

impl Request<'_> {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let payload_length = self.payload.length();
        bytes.extend(self.header.to_bytes(payload_length));
        bytes.extend(self.payload.to_bytes());
        let checksum = bytes[1..].iter().map(|b| *b as u32).sum::<u32>() as u8;
        bytes.push(checksum);
        bytes.push(0x15);
        bytes
    }
}

#[derive(Debug)]
struct RequestHeader {
    msg_id: u16,
    logger_serial: [u8; 4],
}

impl RequestHeader {
    fn to_bytes(&self, payload_length: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(0xA5);
        bytes.extend(payload_length.to_le_bytes());
        bytes.extend([0x10, 0x45]);
        bytes.extend(self.msg_id.to_le_bytes());
        bytes.extend(self.logger_serial);
        bytes
    }
}

#[derive(Debug, Clone)]
#[repr(u8)]
enum RequestFrameType {
    SolarInverter = 0x02,
    DataLoggingStick = 0x01,
    SolarmanCloud = 0x00,
}

#[derive(Debug)]
struct RequestPayload<'a> {
    frame_type: RequestFrameType,
    sensor_type: u16,
    total_working_second: u32,
    uptime_second: u32,
    offset_seconds: u32,
    modbus_rtu_frame: &'a [u8],
}

impl RequestPayload<'_> {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(self.frame_type.clone() as u8);
        bytes.extend(self.sensor_type.to_be_bytes());
        bytes.extend(self.total_working_second.to_le_bytes());
        bytes.extend(self.uptime_second.to_le_bytes());
        bytes.extend(self.offset_seconds.to_le_bytes());
        bytes.extend(self.modbus_rtu_frame);
        bytes
    }

    fn length(&self) -> u16 {
        (15 + self.modbus_rtu_frame.len()).try_into().expect("RTU frame length does not exceed 65521")
    }
}

#[derive(Debug)]
struct ResponseHeader {
    length: u16,
    msg_id: [u8; 2],
    logger_serial: [u8; 4],
}

impl ResponseHeader {
    fn from_bytes(data: &[u8]) -> Self {
        let length = u16::from_le_bytes(
            data[1..3]
                .try_into()
                .expect("constant slice length will never fail"),
        );
        let mut logger_serial = [0; 4];
        logger_serial.copy_from_slice(&data[7..11]);
        let mut msg_id = [0; 2];
        msg_id.copy_from_slice(&data[5..7]);
        ResponseHeader {
            length,
            logger_serial,
            msg_id,
        }
    }
}

#[derive(Debug)]
struct ResponsePayload {
    status: u8,
    total_working_time: [u8; 4],
    power_on_time: [u8; 4],
    offset_time: [u8; 4],
    rtu_frame: Vec<u8>,
    checksum: u8,
}

impl ResponsePayload {
    fn from_bytes(data: &[u8]) -> Self {
        let status = data[1];
        let mut total_working_time = [0; 4];
        total_working_time.copy_from_slice(&data[2..6]);
        let mut power_on_time = [0; 4];
        power_on_time.copy_from_slice(&data[6..10]);
        let mut offset_time = [0; 4];
        offset_time.copy_from_slice(&data[10..14]);
        let mut rtu_frame = Vec::with_capacity(data.len() - 16);
        rtu_frame.extend_from_slice(&data[14..data.len() - 2]);
        ResponsePayload {
            status,
            total_working_time,
            power_on_time,
            offset_time,
            rtu_frame,
            checksum: data[data.len() - 2],
        }
    }
}

#[derive(Debug)]
struct Response {
    header: ResponseHeader,
    payload: ResponsePayload,
}

impl Response {
    fn from_bytes(data: &[u8]) -> Self {
        Response {
            header: ResponseHeader::from_bytes(&data[0..11]),
            payload: ResponsePayload::from_bytes(&data[11..]),
        }
    }
}
