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
