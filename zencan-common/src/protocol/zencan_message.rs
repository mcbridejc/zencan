//! Message definitions encompassing multiple types of messages

use crate::can::CanMessage;

use super::{
    Heartbeat, InvalidNmtStateError, LssRequest, LssResponse, MessageError, NmtCommand, NmtState,
    SdoRequest, SdoResponse, SyncObject, HEARTBEAT_ID, LSS_REQ_ID, LSS_RESP_ID, NMT_CMD_ID,
    SYNC_ID,
};

/// An enum representing all of the standard messages
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum ZencanMessage {
    NmtCommand(NmtCommand),
    Sync(SyncObject),
    Heartbeat(Heartbeat),
    SdoRequest(SdoRequest),
    SdoResponse(SdoResponse),
    LssRequest(LssRequest),
    LssResponse(LssResponse),
}

impl TryFrom<CanMessage> for ZencanMessage {
    type Error = MessageError;

    fn try_from(msg: CanMessage) -> Result<Self, Self::Error> {
        let cob_id = msg.id();
        if cob_id == NMT_CMD_ID {
            Ok(ZencanMessage::NmtCommand(msg.try_into()?))
        } else if cob_id.raw() & !0x7f == HEARTBEAT_ID as u32 {
            let node = (cob_id.raw() & 0x7f) as u8;
            let toggle = (msg.data[0] & (1 << 7)) != 0;
            let state: NmtState = (msg.data[0] & 0x7f)
                .try_into()
                .map_err(|e: InvalidNmtStateError| MessageError::InvalidNmtState { value: e.0 })?;
            Ok(ZencanMessage::Heartbeat(Heartbeat {
                node,
                toggle,
                state,
            }))
        } else if cob_id.raw() & 0xff80 == 0x580 {
            // SDO response
            let resp: SdoResponse = msg
                .try_into()
                .map_err(|_| MessageError::MalformedMsg { cob_id })?;
            Ok(ZencanMessage::SdoResponse(resp))
        } else if cob_id.raw() >= 0x580 && cob_id.raw() <= 0x580 + 256 {
            // SDO request
            let req: SdoRequest = msg
                .data()
                .try_into()
                .map_err(|_| MessageError::MalformedMsg { cob_id })?;
            Ok(ZencanMessage::SdoRequest(req))
        } else if cob_id == SYNC_ID {
            Ok(ZencanMessage::Sync(msg.into()))
        } else if cob_id == LSS_REQ_ID {
            let req: LssRequest = msg
                .data()
                .try_into()
                .map_err(|_| MessageError::MalformedMsg { cob_id })?;
            Ok(ZencanMessage::LssRequest(req))
        } else if cob_id == LSS_RESP_ID {
            let resp: LssResponse = msg
                .data()
                .try_into()
                .map_err(|_| MessageError::MalformedMsg { cob_id })?;
            Ok(ZencanMessage::LssResponse(resp))
        } else {
            Err(MessageError::UnrecognizedId { cob_id })
        }
    }
}
