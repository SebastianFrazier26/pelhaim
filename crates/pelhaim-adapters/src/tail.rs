use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum TailRead {
    /// Complete lines appended since the last read, without `\n` and one trailing `\r`.
    Lines(Vec<Vec<u8>>),
    /// The file was truncated or replaced. The offset is back at 0; the next read starts over.
    Reset,
    Gone,
}

/// Bytes consumed per `read_new` call, so first attach to a large transcript stays bounded;
/// the rest is picked up by later calls. A first line longer than this is still returned whole.
pub const READ_CAP: u64 = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct Tailer {
    path: PathBuf,
    offset: u64,
    identity: Option<(u64, u64)>,
    read_cap: u64,
}

impl Tailer {
    pub fn new(path: PathBuf) -> Tailer {
        Tailer {
            path,
            offset: 0,
            identity: None,
            read_cap: READ_CAP,
        }
    }

    #[doc(hidden)]
    pub fn with_read_cap(path: PathBuf, read_cap: u64) -> Tailer {
        Tailer {
            read_cap,
            ..Tailer::new(path)
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
        let mut reader = BufReader::new(file);
        let mut lines = Vec::new();
        let mut consumed = 0u64;
        loop {
            let mut line = Vec::new();
            let n = reader.read_until(b'\n', &mut line)? as u64;
            // The first line is always taken, even past the cap, so an oversized line cannot stall.
            if line.pop() != Some(b'\n') || (!lines.is_empty() && consumed + n > self.read_cap) {
                break;
            }
            consumed += n;
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(line);
        }
        self.offset += consumed;
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
