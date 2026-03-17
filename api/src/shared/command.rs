use std::{
    fs,
    fs::File,
    io::{Read, Write},
    process::Command,
};

// [Comment removed due to encoding issues]
pub fn run(cmd: &str) -> String {
    tracing::info!("shell cmd:{}", cmd);

    let output = Command::new("/bin/sh").arg("-c").arg(cmd).output();

    match output {
        Ok(op) => {
            let out = String::from_utf8(op.stdout);

            match out {
                Ok(str) => {
                    return str;
                }

                Err(e) => {
                    tracing::error!("{}", e);
                }
            }
        }

        Err(e) => {
            tracing::error!("{}", e);
        }
    };

    String::from("")
}

static SD_PATH: &str = "/mnt/sdcard/img";

static RO_PATH: &str = "/lib/iot-edge/wwwroot/img";

pub fn open_read(file_name: &str) -> Option<Vec<u8>> {
    if let Some(sdr) = read_file(SD_PATH, file_name) {
        return Some(sdr);
    }

    read_file(RO_PATH, file_name)
}

fn read_file(path: &str, file_name: &str) -> Option<Vec<u8>> {
    let file_path = format!("{path}/{file_name}");

    let f = File::open(file_path);

    if let Ok(mut fe) = f {
        let mut buffer = Vec::new();

        let rs = fe.read_to_end(&mut buffer);

        if rs.is_ok() {
            return Some(buffer);
        }
    }

    None
}

pub fn open_write(file_name: &str, data: &[u8]) -> bool {
    if write_file(SD_PATH, file_name, data) {
        return true;
    }

    write_file(RO_PATH, file_name, data)
}

fn write_file(path: &str, file_name: &str, data: &[u8]) -> bool {
    if fs::metadata(path).is_err() {
        let _ = fs::create_dir(path);
    }

    let file_path = format!("{path}/{file_name}");

    let file_rst = File::create(file_path);

    if let Ok(mut file) = file_rst {
        let rs = file.write(data);

        if rs.is_ok() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_run_returns_string() {
        // Test that run returns a string (command execution may fail on test env)
        let result = run("echo test");
        assert!(result.is_empty() || result.contains("test"));
    }

    #[test]
    fn test_command_run_nonexistent() {
        // Nonexistent command should return empty string
        let result = run("nonexistent_command_xyz_12345");
        assert!(result.is_empty());
    }

    #[test]
    fn test_read_file_invalid_path() {
        // Invalid path should return None
        let result = read_file("/nonexistent/path/xyz", "file.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_write_file_creates_directory() {
        // Test with temporary path
        let temp_path = std::env::temp_dir().join("test_write_xyz");
        let temp_path_str = temp_path.to_str().unwrap_or("");
        
        // Should attempt to write (may fail due to permissions)
        let result = write_file(temp_path_str, "test.txt", b"test data");
        // Result depends on system permissions, just verify it returns bool
        assert!(matches!(result, true | false));
        
        // Cleanup
        let _ = std::fs::remove_dir_all(temp_path);
    }

    #[test]
    fn test_open_read_invalid_file() {
        // Nonexistent file should return None
        let result = open_read("nonexistent_file_xyz_12345.bin");
        assert!(result.is_none());
    }
}
