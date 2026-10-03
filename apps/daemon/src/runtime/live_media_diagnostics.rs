// 只保留会话累计值和一个最新发送样本；不保存媒体或按帧增长的历史。
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveMediaDiagnostics {
    received_binary_bytes: u64,
    source_sequence_gaps: u64,
    buffer_evictions: u64,
    forwarded_frames: u64,
    forwarded_binary_bytes: u64,
    viewer_skipped_frames: u64,
    failed_writes: u64,
    last_forward: Option<LiveMediaForwardSample>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveMediaForwardSample {
    viewer_device_id: String,
    epoch: u64,
    frame_id: u64,
    binary_bytes: u64,
    skipped_frames: u64,
    queue_age_ms: u64,
    socket_write_ms: u64,
    write_succeeded: bool,
}

impl LiveMediaForwardSample {
    fn selected(
        viewer_device_id: &str,
        frame: &StoredLiveFrame,
        after_epoch: u64,
        after_frame_id: u64,
    ) -> Self {
        Self {
            viewer_device_id: viewer_device_id.to_owned(),
            epoch: frame.epoch,
            frame_id: frame.frame_id,
            binary_bytes: frame.bytes.len() as u64,
            // 新连接或 epoch 切换不能把未曾订阅的帧算作丢帧。
            skipped_frames: if after_epoch == frame.epoch && after_frame_id > 0 {
                frame
                    .frame_id
                    .saturating_sub(after_frame_id.saturating_add(1))
            } else {
                0
            },
            queue_age_ms: bounded_media_millis(frame.received_at.elapsed()),
            socket_write_ms: 0,
            write_succeeded: false,
        }
    }
}

fn bounded_media_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

impl LiveSessionStore {
    fn record_media_forward(&self, session_id: &str, sample: LiveMediaForwardSample) {
        let Ok(mut sessions) = self.state.lock() else {
            return;
        };
        let Some(record) = sessions.get_mut(session_id) else {
            return;
        };
        if record.closed || record.epoch != sample.epoch {
            return;
        }
        let metrics = &mut record.media_diagnostics;
        if sample.write_succeeded {
            metrics.forwarded_frames = metrics.forwarded_frames.saturating_add(1);
            metrics.forwarded_binary_bytes = metrics
                .forwarded_binary_bytes
                .saturating_add(sample.binary_bytes);
            metrics.viewer_skipped_frames = metrics
                .viewer_skipped_frames
                .saturating_add(sample.skipped_frames);
        } else {
            metrics.failed_writes = metrics.failed_writes.saturating_add(1);
        }
        metrics.last_forward = Some(sample);
    }
}
