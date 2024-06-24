use crate::pack::Pack;
use crate::packet_line::{pkt_line_iter, pkt_line_str, pkt_line_str_keep_newline, PacketLine};
use eyre::{Context, Result};
use std::io::Write;

#[derive(Debug)]
struct Ref {
    hash: String,
    name: String,
}

pub fn run(repo_url: &str, output_dir: Option<&str>) -> Result<()> {
    let repo_url = repo_url.trim_end_matches('/');

    let (refs, extras) = fetch_refs(repo_url)?;

    let head_ref = refs
        .iter()
        .find(|_ref| _ref.name == "HEAD")
        .expect("HEAD ref must exist");

    let default_branch = find_default_branch(&extras);
    let packfile = fetch_packfile(repo_url, head_ref)?;

    if packfile.is_empty() {
        eyre::bail!("oops! looks like we didn't receive anything in the packfile");
    }

    let mut f = std::fs::File::create("repo.pack")?;
    f.write_all(&packfile)?;
    drop(f);

    let mut pack = Pack::open("repo.pack").context("read packfile")?;

    let output_dir = output_dir.unwrap_or_else(|| {
        let (_, repo_name) = repo_url.rsplit_once('/').expect("repo url contains slash");
        repo_name.trim_end_matches(".git")
    });

    std::fs::create_dir(output_dir).context("create directory to clone into")?;
    std::env::set_current_dir(output_dir).unwrap();

    crate::subcommand::init::with_default_branch(default_branch)
        .context("initialize empty repository")?;

    pack.unpack().context("unpack packfile contents")?;
    drop(pack);

    let git_dir = std::path::Path::new(".git");
    let ref_file = git_dir.join(format!("heads/refs/{default_branch}"));
    std::fs::create_dir_all(ref_file.parent().unwrap()).context("create default ref parent")?;
    std::fs::write(ref_file, format!("{}\n", head_ref.hash).as_bytes())
        .context(format!("create .git/refs/heads/{}", default_branch))?;

    crate::subcommand::checkout::run(default_branch)?;

    std::env::set_current_dir("..").unwrap();
    std::fs::remove_file("repo.pack").context("remove packfile")?;

    Ok(())
}

fn fetch_refs(repo_url: &str) -> Result<(Vec<Ref>, Vec<String>)> {
    let refs_url = format!("{}/info/refs?service=git-upload-pack", repo_url);
    let resp = reqwest::blocking::get(refs_url)?;

    const ADV_CONTENT_TYPE: &str = "application/x-git-upload-pack-advertisement";
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if content_type != ADV_CONTENT_TYPE {
        tracing::warn!(
            "bad remote: unexpected content type (wanted \"{}\", got \"{}\")",
            ADV_CONTENT_TYPE,
            content_type
        );
    }

    let bytes = resp.bytes()?;
    let mut line_iter = pkt_line_iter(&bytes);
    let announce = line_iter.next().unwrap_or(b"");
    if announce != b"# service=git-upload-pack\n" {
        tracing::debug!("bad remote: first line from git-upload-pack should announce service");
        tracing::debug!("{}", String::from_utf8_lossy(announce));
        eyre::bail!("bad remote");
    }

    let mut refs: Vec<Ref> = Vec::new();
    let mut extras: Vec<String> = Vec::new();

    for (index, line) in line_iter.enumerate() {
        let line = pkt_line_str(line);
        let (hash, line) = line
            .split_once(' ')
            .ok_or_else(|| eyre::eyre!("read ref hash"))?;

        let name = if index == 0 {
            match line.split_once('\0') {
                None => line,
                Some((name, kvps)) => {
                    extras.extend(kvps.split(' ').map(String::from));
                    name
                }
            }
        } else if line.ends_with("^{}") {
            // TODO: peeled refs
            //
            // For example:
            //
            //   aaa refs/tags/1
            //   bbb refs/tags/1^{}
            //
            // aaa is an annotated tag that points to bbb
            // aaa is "peeled off" to get bbb
            continue;
        } else {
            line
        };

        refs.push(Ref {
            hash: hash.to_owned(),
            name: name.to_owned(),
        });
    }

    Ok((refs, extras))
}

// In order to determine the default branch after a clone, we need
// to find a commit that matches `HEAD`. For newer versions of git,
// that's reported by the `symref` capability. For older versions,
// `refs/heads/master` is preferred if available; if not, the first
// matching ref (sorted alphabetically) is chosen instead. [1]
//
// [1]: https://stackoverflow.com/questions/18726037/what-determines-default-branch-after-git-clone
fn find_default_branch(extras: &[String]) -> &str {
    let default_ref = extras
        .iter()
        .find(|ex| ex.starts_with("symref="))
        .map(|ex| {
            let (head, _ref) = ex
                .trim_start_matches("symref=")
                .split_once(':')
                .expect("valid symref format");
            assert!(head == "HEAD", "symref should start with HEAD");
            _ref
        })
        .unwrap_or_else(|| todo!("default branch resolution when server doesn't support symref"));

    let (_, default_branch) = default_ref
        .rsplit_once('/')
        .expect("ref to be formatted as refs/heads/$BRANCH");

    default_branch
}

// TODO: fetch more than just HEAD?
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

    if pkt_line_str(line_iter.next().unwrap()) != "NAK" {
        eyre::bail!("expected server to respond");
    }

    let mut packfile: Vec<u8> = Vec::new();

    for line in line_iter {
        let Some((channel, line)) = line.split_first() else {
            eyre::bail!("malformed packet w/out channel");
        };

        match channel {
            1 => packfile.extend_from_slice(line),
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
