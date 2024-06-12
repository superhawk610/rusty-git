use crate::parser::Parser;
use eyre::Result;
use std::io::Read;

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
    let pack_path = std::path::Path::new("repo.pack");
    if pack_path.exists() {
        println!("found existing repo.pack packfile, using that...");

        let packfile = std::fs::File::open(pack_path)?;
        let packfile_size = packfile.metadata()?.len() as usize;
        let reader = std::io::BufReader::new(packfile);
        let mut parser = Parser::new(reader);

        let header = parser.parse_str_exact::<4>()?;
        dbg!(header);

        let version = parser.parse_usize_exact::<4>()?;
        dbg!(version);

        let obj_count = parser.parse_usize_exact::<4>()?;
        dbg!(obj_count);

        let mut packfile_offset: usize = 12; // 4 + 4 + 4
        loop {
            // the final 20 bytes of a packfile contain a hash of its contents
            if packfile_offset == packfile_size - 20 {
                let mut buf = [0; 20];
                parser.read_exact(&mut buf)?;
                print!("packfile hash: ");
                for byte in buf.iter() {
                    print!("{byte:x} ");
                }
                println!();
                break;
            }

            // 1 0 0 1 1 1 1 0   0 0 0 0 1 1 1 1
            // ^ |-t-| |--A--|   ^ |-----B-----|
            //
            // the MSB of each byte tells whether to continue parsing (variable len encoding)
            //
            // the first 3 bits of the result indicate the type (see below); the remaining
            // bits should be concatenated, in reverse order (A is the low bits, B is high),
            // to form the actual value: 0b1111_1110
            let size_bytes = parser.parse_size_enc_bytes()?;
            packfile_offset += size_bytes.len() as usize;
            for byte in size_bytes.iter() {
                print!("{byte:08b} ");
            }
            println!();

            // Valid object types are:
            //
            //   - OBJ_COMMIT (1)
            //   - OBJ_TREE (2)
            //   - OBJ_BLOB (3)
            //   - OBJ_TAG (4)
            //   - OBJ_OFS_DELTA (6)
            //   - OBJ_REF_DELTA (7)
            //
            // Type 5 is reserved for future expansion. Type 0 is invalid.
            let obj_type = (size_bytes[0] & 0b0111_0000) >> 4;
            dbg!(obj_type);

            let mut size: usize = (size_bytes[0] & 0b0000_1111) as usize;
            size = size_enc_init(&size_bytes[1..], size, 4);
            dbg!(size);
            println!("{size:016b}");

            use crate::object::{ObjectBuf, ObjectType};

            let consumed = match obj_type {
                // OFS delta encodes the offset of the object in the pack
                6 => todo!("OFS delta encoding"),
                // REF delta uses the object's hash
                7 => {
                    let hash = parser.read_bytes::<20>()?;
                    print!("object hash: ");
                    for byte in hash.iter() {
                        print!("{byte:x}");
                    }
                    println!();

                    let (consumed, mut contents) = parser.split_off_decode(size)?;

                    let size_base_bytes = contents.parse_size_enc_bytes()?;
                    dbg!(&size_base_bytes);
                    print!("size_bash_bytes hash: ");
                    for byte in size_base_bytes.iter() {
                        print!("{byte:08b} ");
                    }
                    println!();
                    let size_base = size_enc(&size_base_bytes);
                    dbg!(size_base);

                    let size_new_bytes = contents.parse_size_enc_bytes()?;
                    dbg!(&size_new_bytes);
                    let size_new = size_enc(&size_new_bytes);
                    dbg!(size_new);

                    // TODO: parse instruction + apply until contents is consumed

                    (consumed as usize) + 20
                }
                _ => {
                    let (consumed, contents) = parser.split_off_decode(size)?;

                    // TODO: figure out how to display/store tags
                    if obj_type != 4 {
                        println!("------------------");
                        let object = ObjectBuf {
                            object_type: match obj_type {
                                1 => ObjectType::Commit,
                                2 => ObjectType::Tree,
                                3 => ObjectType::Blob,
                                _ => panic!("unrecognized object type {obj_type}"),
                            },
                            content_len: size,
                            contents,
                        };
                        crate::subcommand::cat_file::print_obj(object)?;
                        println!();
                    }

                    consumed as usize
                }
            };

            packfile_offset += consumed;
            let packfile = std::fs::File::open(pack_path)?;
            let mut reader = std::io::BufReader::new(packfile);
            reader.seek_relative(packfile_offset as _)?;
            parser = Parser::new(reader);
        }

        return Ok(());
    }

    let repo_url = repo_url.trim_end_matches('/');

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
