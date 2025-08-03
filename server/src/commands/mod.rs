use std::fs::File;
use std::io;
use std::io::{Read, Stdin, Stdout, Write};
use std::path::Path;

pub mod discovery;
pub mod offer;
pub mod services;
pub mod token;

enum CliWrite {
    Stdout(Stdout),
    File(File),
}

impl CliWrite {
    pub fn stdout() -> Self {
        Self::Stdout(io::stdout())
    }

    pub fn file(file: File) -> Self {
        Self::File(file)
    }

    pub fn create<P: AsRef<Path>>(path: Option<P>) -> io::Result<Self> {
        match path {
            None => Ok(Self::stdout()),
            Some(path) => Self::file_create(path),
        }
    }

    pub fn file_create<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        if path.as_ref().to_string_lossy() == "-" {
            Ok(Self::stdout())
        } else {
            Ok(Self::file(File::create(path)?))
        }
    }
}

impl Write for CliWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            CliWrite::Stdout(w) => w.write(buf),
            CliWrite::File(w) => w.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            CliWrite::Stdout(w) => w.flush(),
            CliWrite::File(w) => w.flush(),
        }
    }
}

enum CliRead {
    Stdin(Stdin),
    File(File),
}

impl CliRead {
    pub fn stdin() -> Self {
        Self::Stdin(io::stdin())
    }

    pub fn file(file: File) -> Self {
        Self::File(file)
    }

    pub fn open<P: AsRef<Path>>(path: Option<P>) -> io::Result<Self> {
        match path {
            None => Ok(Self::stdin()),
            Some(path) => Self::file_open(path),
        }
    }

    pub fn file_open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        if path.as_ref().to_string_lossy() == "-" {
            Ok(Self::stdin())
        } else {
            Ok(Self::file(File::open(path)?))
        }
    }
}

impl Read for CliRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            CliRead::Stdin(r) => r.read(buf),
            CliRead::File(r) => r.read(buf),
        }
    }
}

pub fn cli_write_all<P: AsRef<Path>>(path: Option<P>, buf: &[u8]) -> io::Result<()> {
    let mut w = CliWrite::create(path)?;
    w.write_all(buf)?;
    w.flush()?;
    Ok(())
}

pub fn cli_read_to_string<P: AsRef<Path>>(path: Option<P>, buf: &mut String) -> io::Result<usize> {
    let mut r = CliRead::open(path)?;
    r.read_to_string(buf)
}
