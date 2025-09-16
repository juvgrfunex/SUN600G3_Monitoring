Reads power information from one or more Deye SUN600G3 using the Modbus interface and writes it to an InfluxDB 1.8 Database.

The inverters and InfluxDB information need to be configured in a `config.toml` file. An example config with two inverters would look like this:
```toml
log_level = "info"

[monitoring]
influx_ip = "<influx_server_ip>"
intervall_secs = 60
database = "<databse_name>"


[inverter.<name1>]
ip = "<inverter_ip>"


[inverter.<name2>]
ip = "<inverter_ip>"
```
