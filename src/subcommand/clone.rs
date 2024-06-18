use crate::pack::Pack;
use eyre::{Context, Result};

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

    let pack_path = std::path::Path::new("repo.pack");
    if pack_path.exists() {
        println!("found existing repo.pack packfile, using that...");

        let mut pack = Pack::open(pack_path).context("read packfile")?;

        let output_dir = output_dir.unwrap_or_else(|| {
            let (_, repo_name) = repo_url.rsplit_once('/').expect("repo url contains slash");
            repo_name.trim_end_matches(".git")
        });
        std::fs::create_dir(output_dir).context("create directory to clone into")?;
        std::env::set_current_dir(output_dir).expect("directory exists");
        crate::subcommand::init::run().context("initialize empty repository")?;
        pack.unpack().context("unpack packfile contents")?;

        // FIXME: use the `head_ref` from the server response
        let head_ref = Ref {
            hash: "341e1584c9ca9432fa182de6a595348483ed137b",
            name: "main",
        };
        std::fs::create_dir(".git/refs/heads").context("create .git/refs/heads")?;
        std::fs::write(
            ".git/refs/heads/main",
            format!("{}\n", head_ref.hash).as_bytes(),
        )
        .context("create .git/refs/heads/main")?;

        return Ok(());
    }

    let refs = fetch_refs(repo_url)?;

    let head_ref = refs
        .iter()
        .find(|_ref| _ref.name == "HEAD")
        .expect("HEAD ref must exist");
    dbg!(head_ref);

    let packfile = fetch_packfile(repo_url, head_ref)?;

    if packfile.is_empty() {
        println!("oops! looks like we didn't receive anything in the packfile");
    } else {
        use std::io::Write;

        let mut f = std::fs::File::create("repo.pack")?;
        f.write_all(&packfile)?;
        println!("wrote packfile contents to repo.pack");
    }

    Ok(())
}

fn fetch_refs(repo_url: &str) -> Result<Vec<Ref>> {
    // TODO: verify that first line is # service=git-upload-pack
    // TODO: verify that content-type is application/x-git-upload-pack-advertisement
    // let refs_url = format!("{}/info/refs?service=git-upload-pack", repo_url);
    // let resp = reqwest::blocking::get(refs_url)?.bytes()?;
    // dbg!(&resp);

    let resp = b"001e# service=git-upload-pack\n00000153341e1584c9ca9432fa182de6a595348483ed137b HEAD\0multi_ack thin-pack side-band side-band-64k ofs-delta shallow deepen-since deepen-not deepen-relative no-progress include-tag multi_ack_detailed allow-tip-sha1-in-want allow-reachable-sha1-in-want no-done symref=HEAD:refs/heads/main filter object-format=sha1 agent=git/github-f133c3a1d7e6\n003d341e1584c9ca9432fa182de6a595348483ed137b refs/heads/main\n0000";

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

    Ok(refs)
}

fn fetch_packfile(repo_url: &str, head_ref: &Ref) -> Result<Vec<u8>> {
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

    let mut packfile: Vec<u8> = Vec::new();

    for line in line_iter {
        let Some((channel, line)) = line.split_first() else {
            eyre::bail!("malformed packet w/out channel");
        };

        match channel {
            1 => {
                println!("received data packet w/ len {}", line.len());
                packfile.extend_from_slice(line);
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

    Ok(packfile)
}

fn size_enc(size_bytes: &[u8]) -> usize {
    size_enc_init(size_bytes, 0, 0)
}

fn size_enc_init(size_bytes: &[u8], init_n: usize, init_shift: usize) -> usize {
    let mut n = init_n;
    let mut shift = init_shift;

    for byte in size_bytes {
        n += ((byte & 0b0111_1111) as usize) << shift;
        shift += 7;
    }

    n
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
