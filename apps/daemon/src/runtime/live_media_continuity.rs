// JPEG/raw remains latest-wins. H264 only advances through consecutive access units or a new IDR.
fn select_live_media_frame(record: &LiveSessionRecord, epoch: u64, frame_id: u64) -> Option<&StoredLiveFrame> {
    let newest = record.frames.back()?;
    let newer = |frame: &&StoredLiveFrame| frame.epoch > epoch || (frame.epoch == epoch && frame.frame_id > frame_id);
    if newest.bytes.get(57) != Some(&2) {
        return Some(newest).filter(|frame| newer(frame));
    }
    let next = record.frames.iter().filter(newer).find(|frame| {
        frame.epoch == epoch && frame_id != 0 && frame_id.checked_add(1) == Some(frame.frame_id)
    });
    next.or_else(|| record.frames.iter().rev().filter(newer).find(|frame| frame.bytes.get(5).is_some_and(|flags| flags & 1 != 0)))
}

fn validate_live_media_continuity(record: &LiveSessionRecord, metadata: &loom_protocol::LiveFrameMetadata) -> Result<(), LiveRuntimeError> {
    if metadata.codec != loom_protocol::LiveCodec::H264 || metadata.keyframe { return Ok(()); }
    let continuous = record.frames.back().is_some_and(|previous| {
        previous.bytes.get(57) == Some(&2)
            && previous.frame_id.checked_add(1) == Some(metadata.frame_id)
            && previous.bytes.get(40..44) == Some(metadata.width.to_be_bytes().as_slice())
            && previous.bytes.get(44..48) == Some(metadata.height.to_be_bytes().as_slice())
    });
    if !continuous {
        return Err(LiveRuntimeError::new(409, "live_h264_keyframe_required", "H264 reference chain requires a new SPS/PPS/IDR"));
    }
    Ok(())
}
