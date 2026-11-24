use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum TailRead {
    /// Complete lines appended since the last read, without `\n` and one trailing `\r`.
    Lines(Vec<Vec<u8>>),
    /// The file was truncated or replaced. The offset is back at 0; the next read starts over.
    Reset,
    Gone,
}

#[derive(Debug)]
pub struct Tailer {
    path: PathBuf,
    offset: u64,
    identity: Option<(u64, u64)>,
}

impl Tailer {
    pub fn new(path: PathBuf) -> Tailer {
        Tailer {
            path,
            offset: 0,
            identity: None,
        }
    }

    pub fn read_new(&mut self) -> io::Result<TailRead> {
        let mut file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(TailRead::Gone),
            Err(e) => return Err(e),
        };
        let meta = file.metadata()?;
        let identity = identity(&meta);
        let replaced = self.identity.is_some() && self.identity != identity;
        self.identity = identity;
        if replaced || meta.len() < self.offset {
            self.offset = 0;
            return Ok(TailRead::Reset);
        }

        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let Some(last_nl) = buf.iter().rposition(|&b| b == b'\n') else {
            return Ok(TailRead::Lines(Vec::new()));
        };
        self.offset += last_nl as u64 + 1;
        let lines = buf[..last_nl]
            .split(|&b| b == b'\n')
            .map(|line| line.strip_suffix(b"\r").unwrap_or(line).to_vec())
            .collect();
        Ok(TailRead::Lines(lines))
    }
}

#[cfg(unix)]
fn identity(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn identity(_meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}
