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

pub fn pkt_line_iter(mut input: &[u8]) -> impl Iterator<Item = &[u8]> {
    std::iter::from_fn(move || {
        let mut len: usize;

        // skip all flush pkts
        loop {
            if input.is_empty() {
                return None;
            }

            len = usize::from_str_radix(
                std::str::from_utf8(&input[..4]).expect("pkt len is valid utf-8"),
                16,
            )
            .expect("parse pkt len");

            // `0000` is a "flush pkt" and should be handled differently than an empty line (`0004`);
            // servers aren't ever supposed to send empty lines
            if len == 0 {
                input = &input[4..];
            } else {
                break;
            }
        }

        let line = &input[..len][4..];
        input = &input[len..];
        Some(line)
    })
}
