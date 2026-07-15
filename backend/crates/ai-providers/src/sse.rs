use crate::contract::ProviderError;

#[derive(Debug, Clone)]
pub(crate) struct SseFrame {
    pub(crate) event: Option<String>,
    pub(crate) data: String,
}

pub(crate) fn sse_frames(
    body: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + Unpin + 'static,
) -> impl futures::Stream<Item = Result<SseFrame, ProviderError>> + Send {
    use futures::StreamExt;
    let buffer = String::new();

    futures::stream::unfold((body, buffer), |(mut body, mut buf)| async move {
        loop {
            if let Some(frame_start) = find_frame_boundary(&buf) {
                let raw = buf[..frame_start].to_string();
                buf = buf[frame_start + 2..].to_string();
                let frame = parse_sse_frame(&raw);
                if is_empty_frame(&frame) {
                    continue;
                }
                return Some((frame, (body, buf)));
            }

            match body.next().await {
                Some(Ok(chunk)) => {
                    let text = String::from_utf8_lossy(&chunk);
                    buf.push_str(&text);
                }
                Some(Err(e)) => {
                    return Some((
                        Err(ProviderError {
                            category: crate::contract::ErrorCategory::Unavailable,
                            retriable: true,
                            detail: format!("sse read error: {e}"),
                        }),
                        (body, buf),
                    ));
                }
                None => {
                    return None;
                }
            }
        }
    })
}

fn find_frame_boundary(buf: &str) -> Option<usize> {
    buf.find("\n\n").or_else(|| {
        if buf.contains("\r\n\r\n") {
            buf.find("\r\n\r\n")
        } else {
            None
        }
    })
}

fn parse_sse_frame(raw: &str) -> Result<SseFrame, ProviderError> {
    let raw = raw.strip_suffix("\r\n").unwrap_or(raw);
    let raw = raw.strip_suffix('\n').unwrap_or(raw);

    let mut event = None;
    let mut data_lines: Vec<&str> = Vec::new();
    let mut has_comment = false;

    for line in raw.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(':') {
            has_comment = true;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("event:") {
            event = Some(stripped.trim().to_string());
        } else if let Some(stripped) = line.strip_prefix("data:") {
            data_lines.push(stripped.trim_start().trim_end());
        }
    }

    if data_lines.is_empty() {
        if has_comment {
            return Ok(SseFrame {
                event: None,
                data: String::new(),
            });
        }
        return Err(ProviderError {
            category: crate::contract::ErrorCategory::InvalidRequest,
            retriable: false,
            detail: "sse frame with no data lines".into(),
        });
    }

    Ok(SseFrame {
        event,
        data: data_lines.join("\n"),
    })
}

fn is_empty_frame(frame: &Result<SseFrame, ProviderError>) -> bool {
    matches!(frame, Ok(f) if f.data.is_empty() && f.event.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn chunk_stream(
        chunks: Vec<&str>,
    ) -> impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> {
        let items: Vec<Result<bytes::Bytes, reqwest::Error>> = chunks
            .into_iter()
            .map(|s| Ok(bytes::Bytes::copy_from_slice(s.as_bytes())))
            .collect();
        futures::stream::iter(items)
    }

    #[tokio::test]
    async fn one_event_in_one_chunk() {
        let chunks = vec!["data: hello\n\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 1);
        let frame = frames[0].as_ref().unwrap();
        assert!(frame.event.is_none());
        assert_eq!(frame.data, "hello");
    }

    #[tokio::test]
    async fn one_event_split_across_three_chunks() {
        let chunks = vec!["da", "ta: hel", "lo\n\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 1);
        let frame = frames[0].as_ref().unwrap();
        assert_eq!(frame.data, "hello");
    }

    #[tokio::test]
    async fn two_events_in_one_chunk() {
        let chunks = vec!["data: first\n\ndata: second\n\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].as_ref().unwrap().data, "first");
        assert_eq!(frames[1].as_ref().unwrap().data, "second");
    }

    #[tokio::test]
    async fn event_with_type() {
        let chunks = vec!["event: custom\ndata: payload\n\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 1);
        let frame = frames[0].as_ref().unwrap();
        assert_eq!(frame.event.as_deref(), Some("custom"));
        assert_eq!(frame.data, "payload");
    }

    #[tokio::test]
    async fn crlf_variant() {
        let chunks = vec!["data: hello\r\n\r\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_ref().unwrap().data, "hello");
    }

    #[tokio::test]
    async fn multiple_data_lines() {
        let chunks = vec!["data: line1\ndata: line2\n\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_ref().unwrap().data, "line1\nline2");
    }

    #[tokio::test]
    async fn trailing_partial_frame_dropped() {
        let chunks = vec!["data: complete\n\ndata: incomplete"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_ref().unwrap().data, "complete");
    }

    #[tokio::test]
    async fn ignores_comments() {
        let chunks = vec![": comment\ndata: value\n\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_ref().unwrap().data, "value");
    }

    #[tokio::test]
    async fn ignores_comment_only_frame() {
        let chunks = vec![": heartbeat\n\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 0);
    }

    #[tokio::test]
    async fn ignores_comment_frames_between_data() {
        let chunks = vec!["data: first\n\n: heartbeat\n\ndata: second\n\n"];
        let frames: Vec<_> = sse_frames(chunk_stream(chunks)).collect::<Vec<_>>().await;
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].as_ref().unwrap().data, "first");
        assert_eq!(frames[1].as_ref().unwrap().data, "second");
    }
}
