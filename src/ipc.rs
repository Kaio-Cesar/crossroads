//! Single instance for the settings window: a running instance listens on a
//! local socket (a named pipe on Windows). A second plain start asks it to
//! come to the front and exits. Link routing never uses this.

use std::io::{BufRead, BufReader, Write};

use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream, prelude::*};

/// Requests one settings process can make of another.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Bring the window to the front.
    Show,
    /// Config changed elsewhere (e.g. the picker added a rule): reload.
    Reload,
}

impl Request {
    fn encode(&self) -> &'static str {
        match self {
            Self::Show => "show\n",
            Self::Reload => "reload\n",
        }
    }

    fn decode(line: &str) -> Option<Self> {
        match line.trim() {
            "show" => Some(Self::Show),
            "reload" => Some(Self::Reload),
            _ => None,
        }
    }
}

/// Per-user pipe name, so different Windows users don't collide.
fn socket_name() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    format!("crossroads-{user}.sock")
}

/// Sends `request` to a running settings window. Returns `true` if one received it.
pub fn forward(request: &Request) -> bool {
    let Ok(name) = socket_name().to_ns_name::<GenericNamespaced>() else { return false };
    let Ok(mut stream) = Stream::connect(name) else { return false };
    stream.write_all(request.encode().as_bytes()).is_ok()
}

/// Starts listening for requests from later instances. Returns `None` if the
/// socket couldn't be created (e.g. another instance won a startup race).
pub fn serve() -> Option<async_channel::Receiver<Request>> {
    let name = socket_name().to_ns_name::<GenericNamespaced>().ok()?;
    let listener = match ListenerOptions::new().name(name).create_sync() {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("ipc: not listening: {e}");
            return None;
        }
    };
    let (tx, rx) = async_channel::unbounded();
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            for conn in listener.incoming().filter_map(Result::ok) {
                let mut line = String::new();
                if BufReader::new(conn).read_line(&mut line).is_ok()
                    && let Some(request) = Request::decode(&line)
                    && tx.send_blocking(request).is_err()
                {
                    break;
                }
            }
        })
        .ok()?;
    Some(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_round_trips() {
        for r in [Request::Show, Request::Reload] {
            assert_eq!(Request::decode(r.encode()), Some(r));
        }
        assert_eq!(Request::decode("bogus"), None);
    }
}
