use bytes::Bytes;
use futures_core::Stream;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug)]
pub struct PacketLine(String);

impl PacketLine {
    pub fn flush() -> Self {
        Self("".into())
    }

    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn repr(&self) -> String {
        // so-called "flush" packets should be treated differently
        // than an empty packet (`0004`), which should never be sent
        // over the wire
        if self.0.is_empty() {
            "0000".into()
        } else {
            // # of bytes in line + 4 bytes for length + 1 byte for newline
            format!("{:04x}{}\n", self.0.len() + 5, self.0)
        }
    }
}

pub fn pkt_line_str(pkt: &[u8]) -> &str {
    pkt_line_str_keep_newline(pkt).trim_end_matches('\n')
}

pub fn pkt_line_str_keep_newline(pkt: &[u8]) -> &str {
    std::str::from_utf8(pkt).expect("valid utf-8")
}

/// Attempt to parse the next available packet line, returning the
/// number of bytes to advance the cursor (how many bytes were consumed
/// to read the full packet) and the parsed packet. If the packet was
/// a flush, the parsed packet will be `None`. If no full packet was
/// available, returns `(0, None)`.
fn pkt_line_next(input: &[u8]) -> (usize, Option<&[u8]>) {
    if input.len() < 4 {
        // we don't have enough input to parse a full packet
        return (0, None);
    }

    let len_str = std::str::from_utf8(&input[..4]).expect("pkt len is valid utf-8");
    let len = usize::from_str_radix(len_str, 16).expect("parse pkt len");

    if len == 0 {
        // we got a flush packet
        return (4, None);
    }

    if input.len() < len {
        // we know the packet's size, but don't have enough input
        // to parse the packet's contents
        return (0, None);
    }

    // we got a full packet!
    (len, Some(&input[..len][4..]))
}

pub fn pkt_line_iter(mut input: &[u8]) -> impl Iterator<Item = &[u8]> {
    std::iter::from_fn(move || {
        // skip all flush pkts
        loop {
            if input.is_empty() {
                return None;
            }

            match pkt_line_next(input) {
                // only partial packet available
                (0, None) => panic!("malformed partial packet!"),

                // flush packet
                (4, None) => input = &input[4..],

                // standard packet
                (n, Some(packet)) => {
                    input = &input[n..];
                    return Some(packet);
                }

                _ => unreachable!(),
            }
        }
    })
}

pin_project! {
    pub struct PacketLineStream<S> where S: Stream<Item = reqwest::Result<Bytes>> {
        #[pin]
        inner: S,
        buf: Vec<u8>,
        cursor: usize,
    }
}

impl<S> Stream for PacketLineStream<S>
where
    S: Stream<Item = reqwest::Result<Bytes>>,
{
    type Item = reqwest::Result<Vec<u8>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            match this.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(new_bytes))) => {
                    this.buf.extend(new_bytes);

                    match pkt_line_next(&this.buf[*this.cursor..]) {
                        // only partial packet available
                        (0, None) => continue,

                        // flush packet
                        (4, None) => continue,

                        // standard packet
                        (n, Some(packet)) => {
                            *this.cursor += n;
                            return std::task::Poll::Ready(Some(Ok(packet.to_vec())));
                        }

                        _ => unreachable!(),
                    };
                }

                Poll::Ready(Some(Err(err))) => {
                    return Poll::Ready(Some(Err::<Vec<u8>, reqwest::Error>(err)))
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            };
        }
    }
}

impl<S> PacketLineStream<S>
where
    S: Stream<Item = reqwest::Result<Bytes>>,
{
    pub fn new(s: S) -> Self {
        Self {
            inner: s,
            buf: Vec::new(),
            cursor: 0,
        }
    }
}
