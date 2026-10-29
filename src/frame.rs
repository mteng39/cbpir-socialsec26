use crate::{FRAME_LEN, LogicalRecord, MAX_PAYLOAD, P0dError, ReconstructedRecord, RecordId};

pub fn encode_frame(record: &LogicalRecord) -> Result<[u8; FRAME_LEN], P0dError> {
    if record.payload.len() > MAX_PAYLOAD {
        return Err(P0dError::Frame("payload exceeds 114 bytes"));
    }
    let mut frame = [0; FRAME_LEN];
    frame[..4].copy_from_slice(b"P0D1");
    frame[4..12].copy_from_slice(&record.id.get().to_le_bytes());
    let len =
        u16::try_from(record.payload.len()).map_err(|_| P0dError::Frame("length overflow"))?;
    frame[12..14].copy_from_slice(&len.to_le_bytes());
    frame[14..14 + record.payload.len()].copy_from_slice(&record.payload);
    Ok(frame)
}
pub fn decode_frame(frame: &[u8], expected: RecordId) -> Result<ReconstructedRecord, P0dError> {
    if frame.len() != FRAME_LEN {
        return Err(P0dError::Frame("wrong shape"));
    }
    if &frame[..4] != b"P0D1" {
        return Err(P0dError::Frame("wrong magic"));
    }
    let mut id_bytes = [0; 8];
    id_bytes.copy_from_slice(&frame[4..12]);
    let id = u64::from_le_bytes(id_bytes);
    if id == 0 {
        return Err(P0dError::Frame("zero ID"));
    }
    if id != expected.get() {
        return Err(P0dError::Frame("unexpected ID"));
    }
    let len = usize::from(u16::from_le_bytes([frame[12], frame[13]]));
    if len > MAX_PAYLOAD {
        return Err(P0dError::Frame("declared length exceeds 114 bytes"));
    }
    if frame[14 + len..].iter().any(|x| *x != 0) {
        return Err(P0dError::Frame("nonzero padding"));
    }
    Ok(ReconstructedRecord {
        id: expected,
        payload: frame[14..14 + len].to_vec(),
    })
}
