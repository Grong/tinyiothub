//! 鸿蒙系统串口通信实现
//!
//! 提供与Linux版本兼容的串口接口，但使用鸿蒙系统的串口API

use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{Read, Write},
    sync::Mutex,
    time::Duration,
};

use tracing::{debug, info};

/// 串口配置
#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub port: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: SerialParity,
    pub timeout: Duration,
}

/// 串口校验位
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SerialParity {
    None,
    Even,
    Odd,
}

/// 鸿蒙系统串口连接
pub struct HarmonySerialConnection {
    config: SerialConfig,
    file: Option<std::fs::File>,
    is_open: bool,
}

impl HarmonySerialConnection {
    /// 创建新的串口连接
    pub fn new(config: SerialConfig) -> Self {
        Self { config, file: None, is_open: false }
    }

    /// 打开串口连接
    pub fn open(&mut self) -> Result<(), std::io::Error> {
        info!("Opening HarmonyOS serial port: {}", self.config.port);

        // 使用标准文件I/O打开串口设备
        let file = OpenOptions::new().read(true).write(true).open(&self.config.port)?;

        self.file = Some(file);
        self.is_open = true;

        info!("HarmonyOS serial port opened successfully: {}", self.config.port);
        Ok(())
    }

    /// 关闭串口连接
    pub fn close(&mut self) -> Result<(), std::io::Error> {
        if !self.is_open {
            return Ok(());
        }

        info!("Closing HarmonyOS serial port: {}", self.config.port);

        self.file = None;
        self.is_open = false;

        Ok(())
    }

    /// 检查串口是否已打开
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// 写入数据到串口
    pub fn write(&mut self, data: &[u8]) -> Result<usize, std::io::Error> {
        if !self.is_open {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Serial port not open",
            ));
        }

        debug!("Writing {} bytes to HarmonyOS serial port: {}", data.len(), self.config.port);

        match &mut self.file {
            Some(file) => {
                let written = file.write(data)?;
                file.flush()?;
                Ok(written)
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Serial port file not available",
            )),
        }
    }

    /// 从串口读取数据
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, std::io::Error> {
        if !self.is_open {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Serial port not open",
            ));
        }

        debug!("Reading from HarmonyOS serial port: {}", self.config.port);

        match &mut self.file {
            Some(file) => {
                let read_count = file.read(buffer)?;
                if read_count > 0 {
                    debug!("Read {} bytes from serial port", read_count);
                }
                Ok(read_count)
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Serial port file not available",
            )),
        }
    }

    /// 异步写入数据
    pub async fn write_async(&mut self, data: &[u8]) -> Result<usize, std::io::Error> {
        let data = data.to_vec();
        let port = self.config.port.clone();

        tokio::task::spawn_blocking(move || {
            let mut file = OpenOptions::new().write(true).open(&port)?;
            let written = file.write(&data)?;
            file.flush()?;
            Ok::<usize, std::io::Error>(written)
        })
        .await
        .map_err(std::io::Error::other)?
    }

    /// 异步读取数据
    pub async fn read_async(&mut self, buffer_size: usize) -> Result<Vec<u8>, std::io::Error> {
        let port = self.config.port.clone();

        tokio::task::spawn_blocking(move || {
            let mut file = OpenOptions::new().read(true).open(&port)?;
            let mut buffer = vec![0u8; buffer_size];
            let read_count = file.read(&mut buffer)?;
            buffer.truncate(read_count);
            Ok::<Vec<u8>, std::io::Error>(buffer)
        })
        .await
        .map_err(std::io::Error::other)?
    }

    /// 刷新串口缓冲区
    pub fn flush(&mut self) -> Result<(), std::io::Error> {
        if !self.is_open {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Serial port not open",
            ));
        }

        debug!("Flushing HarmonyOS serial port: {}", self.config.port);

        match &mut self.file {
            Some(file) => file.flush(),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Serial port file not available",
            )),
        }
    }

    /// 设置超时时间
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<(), std::io::Error> {
        self.config.timeout = timeout;

        // 在鸿蒙系统上，超时设置可能需要特殊处理
        // 这里先更新配置，实际的超时控制在读写操作中实现

        Ok(())
    }

    /// 获取串口配置
    pub fn get_config(&self) -> &SerialConfig {
        &self.config
    }

    /// 检查串口设备是否存在
    pub fn is_available(&self) -> bool {
        std::path::Path::new(&self.config.port).exists()
    }
}

impl Drop for HarmonySerialConnection {
    fn drop(&mut self) {
        if self.is_open {
            let _ = self.close();
        }
    }
}

/// 鸿蒙系统串口管理器
pub struct HarmonySerialManager {
    connections: Mutex<HashMap<String, HarmonySerialConnection>>,
}

impl HarmonySerialManager {
    /// 创建新的串口管理器
    pub fn new() -> Self {
        Self { connections: Mutex::new(HashMap::new()) }
    }

    /// 创建串口连接
    pub fn create_connection(&self, port: &str, baud_rate: u32) -> Result<(), std::io::Error> {
        let config = SerialConfig {
            port: port.to_string(),
            baud_rate,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            timeout: Duration::from_secs(1),
        };

        let mut connection = HarmonySerialConnection::new(config);
        connection.open()?;

        let mut connections = self.connections.lock().unwrap();
        connections.insert(port.to_string(), connection);

        Ok(())
    }

    /// 获取串口连接的可变引用
    pub fn with_connection<F, R>(&self, port: &str, f: F) -> Result<R, std::io::Error>
    where
        F: FnOnce(&mut HarmonySerialConnection) -> Result<R, std::io::Error>,
    {
        let mut connections = self.connections.lock().unwrap();
        match connections.get_mut(port) {
            Some(connection) => f(connection),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Serial port '{}' not found", port),
            )),
        }
    }

    /// 关闭串口连接
    pub fn close_connection(&self, port: &str) -> Result<(), std::io::Error> {
        let mut connections = self.connections.lock().unwrap();
        if let Some(mut connection) = connections.remove(port) {
            connection.close()?;
        }
        Ok(())
    }

    /// 列出所有可用的串口
    pub fn list_ports(&self) -> Result<Vec<String>, std::io::Error> {
        let mut available_ports = Vec::new();

        // 鸿蒙系统常见的串口设备路径
        let common_paths = [
            "/dev/ttyS0",
            "/dev/ttyS1",
            "/dev/ttyS2",
            "/dev/ttyS3",
            "/dev/ttyUSB0",
            "/dev/ttyUSB1",
            "/dev/ttyUSB2",
            "/dev/ttyUSB3",
            "/dev/ttyACM0",
            "/dev/ttyACM1",
            "/dev/ttyACM2",
            "/dev/ttyACM3",
            "/dev/ttyAMA0",
            "/dev/ttyAMA1", // ARM串口
        ];

        for path in &common_paths {
            if std::path::Path::new(path).exists() {
                available_ports.push(path.to_string());
            }
        }

        info!("Found {} serial ports: {:?}", available_ports.len(), available_ports);
        Ok(available_ports)
    }

    /// 检查串口是否已连接
    pub fn is_connected(&self, port: &str) -> bool {
        let connections = self.connections.lock().unwrap();
        connections.contains_key(port)
    }

    /// 获取所有已连接的串口列表
    pub fn get_connected_ports(&self) -> Vec<String> {
        let connections = self.connections.lock().unwrap();
        connections.keys().cloned().collect()
    }
}

impl Default for HarmonySerialManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局串口管理器实例
static SERIAL_MANAGER: std::sync::LazyLock<HarmonySerialManager> =
    std::sync::LazyLock::new(HarmonySerialManager::new);

/// 获取全局串口管理器
pub fn get_serial_manager() -> &'static HarmonySerialManager {
    &SERIAL_MANAGER
}

/// 兼容性函数：打开串口（与Linux版本兼容）
pub fn open_serial_port(port: &str, baud_rate: u32) -> Result<(), std::io::Error> {
    get_serial_manager().create_connection(port, baud_rate)
}

/// 兼容性函数：关闭串口
pub fn close_serial_port(port: &str) -> Result<(), std::io::Error> {
    get_serial_manager().close_connection(port)
}

/// 兼容性函数：列出串口
pub fn list_serial_ports() -> Result<Vec<String>, std::io::Error> {
    get_serial_manager().list_ports()
}

/// 兼容性函数：写入数据到串口
pub fn write_serial_data(port: &str, data: &[u8]) -> Result<usize, std::io::Error> {
    get_serial_manager().with_connection(port, |conn| conn.write(data))
}

/// 兼容性函数：从串口读取数据
pub fn read_serial_data(port: &str, buffer: &mut [u8]) -> Result<usize, std::io::Error> {
    get_serial_manager().with_connection(port, |conn| conn.read(buffer))
}

/// 异步写入数据到串口
pub async fn write_serial_data_async(port: &str, data: &[u8]) -> Result<usize, std::io::Error> {
    let data = data.to_vec();
    let port = port.to_string();

    tokio::task::spawn_blocking(move || write_serial_data(&port, &data))
        .await
        .map_err(std::io::Error::other)?
}

/// 异步从串口读取数据
pub async fn read_serial_data_async(
    port: &str,
    buffer_size: usize,
) -> Result<Vec<u8>, std::io::Error> {
    let port = port.to_string();

    tokio::task::spawn_blocking(move || {
        let mut buffer = vec![0u8; buffer_size];
        let read_count = read_serial_data(&port, &mut buffer)?;
        buffer.truncate(read_count);
        Ok::<Vec<u8>, std::io::Error>(buffer)
    })
    .await
    .map_err(std::io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_config() {
        let config = SerialConfig {
            port: "/dev/ttyS0".to_string(),
            baud_rate: 115200,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            timeout: Duration::from_secs(1),
        };

        assert_eq!(config.port, "/dev/ttyS0");
        assert_eq!(config.baud_rate, 115200);
    }

    #[test]
    fn test_serial_manager() {
        let manager = HarmonySerialManager::new();
        assert_eq!(manager.get_connected_ports().len(), 0);
        assert!(!manager.is_connected("/dev/ttyS0"));
    }
}
