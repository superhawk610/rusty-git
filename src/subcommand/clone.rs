use std::io::Read;

use eyre::Result;

#[derive(Debug)]
struct Ref<'a, 'b> {
    hash: &'a str,
    name: &'b str,
}

#[derive(Debug)]
struct PacketLine(String);

impl PacketLine {
    fn flush() -> Self {
        Self("".into())
    }

    fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    fn repr(&self) -> String {
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

pub fn run(repo_url: &str, output_dir: Option<&str>) -> Result<()> {
    let repo_url = repo_url.trim_end_matches('/');
    let url = format!("{}/info/refs?service=git-upload-pack", repo_url);

    // TODO: verify that first line is # service=git-upload-pack
    // TODO: verify that content-type is application/x-git-upload-pack-advertisement
    // let resp = reqwest::blocking::get(url)?.text()?;
    // dbg!(&resp);

    let resp = b"001e# service=git-upload-pack\n0000015375a0543c8e03629a410e92bb45dd2123a5c48fda HEAD\0multi_ack thin-pack side-band side-band-64k ofs-delta shallow deepen-since deepen-not deepen-relative no-progress include-tag multi_ack_detailed allow-tip-sha1-in-want allow-reachable-sha1-in-want no-done symref=HEAD:refs/heads/main filter object-format=sha1 agent=git/github-f133c3a1d7e6\n003d75a0543c8e03629a410e92bb45dd2123a5c48fda refs/heads/main\n0000";

    let mut refs: Vec<Ref> = Vec::new();
    let mut extras: Vec<&str> = Vec::new();

    // skip the first line, since it will just be the service announcement header
    for (index, line) in pkt_line_iter(resp).skip(1).enumerate() {
        let line = pkt_line_str(line);
        let (hash, line) = line
            .split_once(' ')
            .ok_or_else(|| eyre::eyre!("read ref hash"))?;

        let name = if index == 0 {
            match line.split_once('\0') {
                None => line,
                Some((name, kvps)) => {
                    extras.extend(kvps.split(' '));
                    name
                }
            }
        } else {
            line
        };

        refs.push(Ref { hash, name });
    }

    println!("{refs:#?}");
    println!("{extras:#?}");

    let head_ref = refs
        .iter()
        .find(|_ref| _ref.name == "HEAD")
        .expect("HEAD ref must exist");
    dbg!(head_ref);

    // side-band, side-band-64k
    //
    // This capability means that server can send, and client understand multiplexed progress
    // reports and error info interleaved with the packfile itself.
    //
    // These two options are mutually exclusive. A modern client always favors side-band-64k.
    //
    // Either mode indicates that the packfile data will be streamed broken up into packets
    // of up to either 1000 bytes in the case of side_band, or 65520 bytes in the case of
    // side_band_64k. Each packet is made up of a leading 4-byte pkt-line length of how much
    // data is in the packet, followed by a 1-byte stream code, followed by the actual data.
    //
    // The stream code can be one of:
    //
    //   1 - pack data
    //   2 - progress messages
    //   3 - fatal error message just before stream aborts
    //

    let mut body = String::new();
    body.push_str(&PacketLine::new(format!("want {} side-band-64k", head_ref.hash)).repr());
    body.push_str(&PacketLine::flush().repr());
    body.push_str(&PacketLine::new("done").repr());
    dbg!(&body);

    let client = reqwest::blocking::Client::new();
    let url = format!("{}/git-upload-pack", repo_url);
    let resp = client
        .post(url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-git-upload-pack-request",
        )
        .body(body)
        .send()?
        .bytes()?;

    let mut line_iter = pkt_line_iter(&resp);
    // TODO: verify that this is `NAK`
    dbg!(pkt_line_str(line_iter.next().unwrap()));
    for line in line_iter {
        let Some((channel, line)) = line.split_first() else {
            eyre::bail!("malformed packet w/out channel");
        };

        match channel {
            1 => {
                println!("received data packet w/ len {}", line.len());
            }
            2 | 3 => {
                // TODO: switch away from reqwest blocking to display this in real time
                print!("remote: {}", pkt_line_str_keep_newline(line));
            }
            other => {
                panic!("unrecognized channel {other}");
            }
        }
    }

    Ok(())
}

fn pkt_line_str(pkt: &[u8]) -> &str {
    pkt_line_str_keep_newline(pkt).trim_end_matches('\n')
}

fn pkt_line_str_keep_newline(pkt: &[u8]) -> &str {
    std::str::from_utf8(pkt).expect("valid utf-8")
}

fn pkt_line_iter(mut input: &[u8]) -> impl Iterator<Item = &[u8]> {
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
