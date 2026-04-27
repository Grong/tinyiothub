// SPDX-FileCopyrightText: Copyright (c) 2017-2022 slowtec GmbH <post@slowtec.de>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::slave::SlaveId;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) slave_id: SlaveId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestAdu {
    pub(crate) hdr: Header,
    pub(crate) pdu: RequestPdu,
    pub(crate) disconnect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResponseAdu {
    pub(crate) hdr: Header,
    pub(crate) pdu: ResponsePdu,
}
