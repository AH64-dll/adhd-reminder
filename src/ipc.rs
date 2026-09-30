use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
}

pub struct Server {
    pub listener: TcpListener,
    token: String,
    _lock: File,
}

impl Server {
    pub fn claim(dir: &Path) -> io::Result<Option<Self>> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("instance.lock"))?;
        match lock.try_lock_exclusive() {
            Ok(()) => {}
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                return Ok(None)
            }
            Err(e) => return Err(e),
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let token = format!(
            "{:032x}{:032x}",
            rand::random::<u128>(),
            rand::random::<u128>()
        );
        let endpoint = Endpoint {
            port: listener.local_addr()?.port(),
            token: token.clone(),
        };
        crate::storage::atomic_write(&dir.join("endpoint.json"), &serde_json::to_vec(&endpoint)?)?;
        Ok(Some(Self {
            listener,
            token,
            _lock: lock,
        }))
    }

    pub fn read_command(&self, stream: &mut TcpStream) -> io::Result<String> {
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut line = String::new();
        use std::io::Read;
        BufReader::new(stream).take(4096).read_line(&mut line)?;
        let (token, command) = line
            .trim_end()
            .split_once(' ')
            .ok_or_else(|| io::Error::other("Bad IPC request"))?;
        if token != self.token {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Invalid IPC token",
            ));
        }
        Ok(command.to_owned())
    }
}

pub fn send(dir: &Path, command: &str) -> io::Result<String> {
    let endpoint: Endpoint = serde_json::from_slice(&fs::read(dir.join("endpoint.json"))?)?;
    let address = SocketAddr::from(([127, 0, 0, 1], endpoint.port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(4)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    writeln!(stream, "{} {command}", endpoint.token)?;
    let mut response = String::new();
    if BufReader::new(stream).read_line(&mut response)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "The app did not answer",
        ));
    }
    Ok(response.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_instance_and_reclaim_after_exit() {
        let dir = tempfile::tempdir().unwrap();
        let first = Server::claim(dir.path()).unwrap().unwrap();
        assert!(Server::claim(dir.path()).unwrap().is_none());
        drop(first);
        assert!(Server::claim(dir.path()).unwrap().is_some());
    }
    #[test]
    fn authenticated_local_command() {
        let dir = tempfile::tempdir().unwrap();
        let server = Server::claim(dir.path()).unwrap().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = server.listener.accept().unwrap();
            assert_eq!(server.read_command(&mut stream).unwrap(), "show");
            writeln!(stream, "ok").unwrap();
        });
        assert_eq!(send(dir.path(), "show").unwrap(), "ok");
        worker.join().unwrap();
    }
}
