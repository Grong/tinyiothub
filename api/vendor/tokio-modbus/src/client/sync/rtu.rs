// SPDX-FileCopyrightText: Copyright (c) 2017-2022 slowtec GmbH <post@slowtec.de>
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::io::{Error, ErrorKind};
use std::time::Duration;

use tokio::time;
use tokio_serial::{SerialPortBuilder, SerialStream};

use crate::client::rtu::connect_slave as async_connect_slave;
use crate::slave::Slave;

use super::{Context, Result};

/// Connect to no particular Modbus slave device for sending
/// broadcast messages.
pub fn connect(builder: &SerialPortBuilder) -> Result<Context> {
    connect_slave(builder, Slave::broadcast())
}

/// Connect to any kind of Modbus slave device.
pub fn connect_slave(builder: &SerialPortBuilder, slave: Slave) -> Result<Context> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;
    // SerialStream::open requires a runtime at least on cfg(unix).
    let serial = rt.block_on(async { SerialStream::open(builder) })?;
    let async_ctx = rt.block_on(async_connect_slave(serial, slave))?;
    let sync_ctx = Context {
        core: rt,
        async_ctx,
        timeout: Duration::from_millis(1000),
    };
    Ok(sync_ctx)
}

pub fn connect_slave_with_timeout(builder: &SerialPortBuilder, slave: Slave, timeout: Duration) -> Result<Context> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .enable_io()
        .build()?;
    // SerialStream::open requires a runtime at least on cfg(unix).
    let serial = rt.block_on(async {
        SerialStream::open(builder)
    })?;
    let async_ctx = rt.block_on(async {
        tokio::select! {
            ctx = async_connect_slave(serial, slave) => {
                ctx
            }
            _ = time::sleep(timeout) => {
                Err(Error::new(ErrorKind::TimedOut,"async_connect_slave timeout"))
            }
        }
    })?;
    let sync_ctx = Context {
        core: rt,
        async_ctx,
        timeout,
    };
    Ok(sync_ctx)
}

