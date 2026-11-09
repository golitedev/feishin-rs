//! Safe platform transports for MPV's local IPC endpoint.
use std::{io, time::Duration};

pub(crate) struct Endpoint {
    name: String,
    #[cfg(unix)]
    _directory: tempfile::TempDir,
}

impl Endpoint {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    #[cfg(unix)]
    pub(crate) fn new() -> Result<Self, &'static str> {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::Builder::new()
            .prefix("feishin-mpv-")
            .tempdir()
            .map_err(|_| "Could not create a private MPV IPC directory.")?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not secure the MPV IPC directory.")?;
        let name = directory
            .path()
            .join("ipc")
            .to_str()
            .ok_or("The MPV IPC path is not valid Unicode.")?
            .to_string();
        Ok(Self {
            name,
            _directory: directory,
        })
    }

    #[cfg(windows)]
    pub(crate) fn new() -> Result<Self, &'static str> {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| "Could not generate a private MPV pipe name.")?;
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self {
            name: format!(r"\\.\pipe\feishin-rs-{suffix}"),
        })
    }
}

#[cfg(unix)]
pub(crate) type Stream = std::os::unix::net::UnixStream;

#[cfg(unix)]
pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<Stream> {
    let stream = Stream::connect(endpoint.name())?;
    stream.set_read_timeout(Some(Duration::from_millis(20)))?;
    stream.set_write_timeout(Some(Duration::from_millis(250)))?;
    Ok(stream)
}

#[cfg(windows)]
pub(crate) struct Stream {
    runtime: tokio::runtime::Runtime,
    pipe: tokio::net::windows::named_pipe::NamedPipeClient,
}

#[cfg(windows)]
pub(crate) fn connect(endpoint: &Endpoint) -> io::Result<Stream> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pipe = {
        let _entered = runtime.enter();
        tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint.name())?
    };
    Ok(Stream { runtime, pipe })
}

#[cfg(windows)]
impl io::Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        use tokio::io::AsyncReadExt;
        let runtime = &self.runtime;
        let pipe = &mut self.pipe;
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(20), pipe.read(buffer))
                .await
                .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
        })
    }
}

#[cfg(windows)]
impl io::Write for Stream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        use tokio::io::AsyncWriteExt;
        let runtime = &self.runtime;
        let pipe = &mut self.pipe;
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(250), pipe.write(buffer))
                .await
                .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
