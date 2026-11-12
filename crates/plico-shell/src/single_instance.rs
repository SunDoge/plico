//! Launch activation parsing and single-instance forwarding.
//!
//! Desktop applications are often started with URLs or file paths - a deep
//! link, a double-clicked document, or a second launch while the app is
//! already running. [`AppActivation`] normalizes that command line, and
//! [`SingleInstance`] lets the first process receive activations from later
//! launches on the same user session.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
use std::path::Path;

use interprocess::local_socket::traits::{Listener as _, Stream as _};
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
use interprocess::local_socket::{GenericFilePath, ToFsName};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use interprocess::local_socket::{GenericNamespaced, ToNsName};
use interprocess::local_socket::{
    Listener as LocalListener, ListenerNonblockingMode, ListenerOptions, Stream as LocalStream,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

use plico_renderer::WakeCallback;

const MAX_ACTIVATION_BYTES: usize = 1 << 20;
const ACCEPT_POLL: Duration = Duration::from_millis(25);

/// Errors raised while acquiring or communicating over the single-instance
/// activation channel.
#[derive(Debug, Snafu)]
pub enum SingleInstanceError {
    /// The local socket backend failed.
    #[snafu(display("single-instance socket failed: {source}"))]
    Socket {
        /// Underlying I/O error.
        source: io::Error,
    },
    /// The activation could not be encoded.
    #[snafu(display("activation payload could not be encoded: {source}"))]
    Encode {
        /// Serialization error.
        source: serde_json::Error,
    },
}

/// One launch's command-line activation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppActivation {
    /// Arguments after the executable path.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory of the launching process.
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    /// Wayland/desktop activation token, when the launcher provided one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_token: Option<String>,
}

impl AppActivation {
    /// Capture this process's arguments and environment without interpreting
    /// any of them.
    pub fn current() -> Self {
        Self::from_args(std::env::args().skip(1), std::env::current_dir().ok())
    }

    /// Build an activation from already-divided arguments.
    pub fn from_args(args: impl IntoIterator<Item = String>, working_dir: Option<PathBuf>) -> Self {
        let token = std::env::var("XDG_ACTIVATION_TOKEN")
            .ok()
            .filter(|token| !token.is_empty())
            .or_else(|| {
                std::env::var("DESKTOP_STARTUP_ID")
                    .ok()
                    .filter(|token| !token.is_empty())
            });
        Self {
            args: args.into_iter().collect(),
            working_dir,
            activation_token: token,
        }
    }

    /// Arguments that are URLs of one of `schemes`, preserving order.
    ///
    /// Scheme matching is ASCII case-insensitive. A single-letter prefix is
    /// treated as a Windows drive path, not a URL scheme.
    pub fn urls(&self, schemes: &[&str]) -> Vec<String> {
        self.args
            .iter()
            .filter(|argument| {
                let Some((scheme, _)) = argument.split_once(':') else {
                    return false;
                };
                scheme.len() > 1
                    && is_url_scheme(scheme)
                    && schemes
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(scheme))
            })
            .cloned()
            .collect()
    }

    /// Existing regular files among the arguments whose extension is in
    /// `extensions` (with or without a leading dot).
    ///
    /// Relative paths are resolved against [`Self::working_dir`], and
    /// `file://` URIs are decoded.
    pub fn files(&self, extensions: &[&str]) -> Vec<PathBuf> {
        if extensions.is_empty() {
            return Vec::new();
        }
        let mut files = Vec::new();
        for argument in &self.args {
            let path = file_argument_path(argument);
            let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
                continue;
            };
            if !extensions.iter().any(|candidate| {
                candidate
                    .trim_start_matches('.')
                    .eq_ignore_ascii_case(extension)
            }) {
                continue;
            }
            let path = if path.is_absolute() {
                path
            } else {
                let Some(working_dir) = &self.working_dir else {
                    continue;
                };
                working_dir.join(path)
            };
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if path.is_file() {
                files.push(path);
            }
        }
        files
    }
}

fn file_argument_path(argument: &str) -> PathBuf {
    if let Ok(url) = url::Url::parse(argument)
        && url.scheme() == "file"
        && let Ok(path) = url.to_file_path()
    {
        return path;
    }
    PathBuf::from(argument)
}

fn is_url_scheme(scheme: &str) -> bool {
    let mut bytes = scheme.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// Result of trying to become the process that owns an activation channel.
pub enum SingleInstanceOutcome {
    /// This process owns the channel and should keep running.
    Primary(SingleInstance),
    /// Another process owns the channel; its activation was forwarded.
    Secondary,
}

/// The primary process's activation receiver.
///
/// A background thread accepts forwarded launches and wakes the host through
/// the callback supplied to [`SingleInstance::acquire`]. Call [`Self::drain`]
/// from the event loop to consume them. Dropping this value stops the
/// receiver and releases the channel name.
pub struct SingleInstance {
    pending: Arc<Mutex<VecDeque<AppActivation>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl SingleInstance {
    /// Become the primary instance for `app_id`, or forward `activation` to
    /// the existing one.
    ///
    /// `app_id` should be stable across launches and unique to the
    /// application (typically a reverse-DNS identifier). The current user is
    /// included in the channel name, so different users do not collide.
    pub fn acquire(
        app_id: &str,
        activation: &AppActivation,
        wake: WakeCallback,
    ) -> Result<SingleInstanceOutcome, SingleInstanceError> {
        let endpoint = endpoint_for(app_id);
        if let Ok(stream) = connect(&endpoint) {
            send_activation(&stream, activation)?;
            return Ok(SingleInstanceOutcome::Secondary);
        }
        let listener = match create_listener(&endpoint) {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                // Lost a race with another process between connect and create.
                let stream = connect(&endpoint).context(SocketSnafu)?;
                send_activation(&stream, activation)?;
                return Ok(SingleInstanceOutcome::Secondary);
            }
            Err(source) => return Err(SingleInstanceError::Socket { source }),
        };
        Ok(SingleInstanceOutcome::Primary(Self::start(listener, wake)))
    }

    fn start(listener: LocalListener, wake: WakeCallback) -> Self {
        let pending = Arc::new(Mutex::new(VecDeque::new()));
        let queue = Arc::clone(&pending);
        let stop = Arc::new(AtomicBool::new(false));
        let should_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("plico-activation".into())
            .spawn(move || {
                if listener
                    .set_nonblocking(ListenerNonblockingMode::Accept)
                    .is_err()
                {
                    return;
                }
                while !should_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok(stream) => {
                            let _ = stream.set_recv_timeout(Some(Duration::from_secs(2)));
                            if let Ok(Some(activation)) = read_activation(&stream)
                                && let Ok(mut queue) = queue.lock()
                            {
                                queue.push_back(activation);
                                drop(queue);
                                wake();
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::park_timeout(ACCEPT_POLL);
                        }
                        Err(error) => {
                            tracing::warn!(%error, "single-instance accept failed");
                            thread::park_timeout(ACCEPT_POLL);
                        }
                    }
                }
            })
            .expect("failed to spawn activation thread");
        Self {
            pending,
            stop,
            worker: Some(worker),
        }
    }

    /// Take activations forwarded since the previous call.
    pub fn drain(&mut self) -> Vec<AppActivation> {
        self.pending
            .lock()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default()
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

fn send_activation(
    mut stream: &LocalStream,
    activation: &AppActivation,
) -> Result<(), SingleInstanceError> {
    let payload = serde_json::to_vec(activation).context(EncodeSnafu)?;
    let length = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    stream
        .write_all(&length.to_le_bytes())
        .and_then(|()| stream.write_all(&payload))
        .and_then(|()| stream.flush())
        .context(SocketSnafu)
}

fn read_activation(mut stream: &LocalStream) -> io::Result<Option<AppActivation>> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_ACTIVATION_BYTES {
        return Ok(None);
    }
    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload)?;
    Ok(serde_json::from_slice(&payload).ok())
}

enum Endpoint {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    Namespaced(String),
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    Path(PathBuf),
}

impl Endpoint {
    fn name(&self) -> io::Result<interprocess::local_socket::Name<'_>> {
        match self {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            Self::Namespaced(name) => name.as_str().to_ns_name::<GenericNamespaced>(),
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            Self::Path(path) => path
                .to_str()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "non-UTF-8 socket path")
                })?
                .to_fs_name::<GenericFilePath>(),
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn path(&self) -> Option<&Path> {
        match self {
            Self::Path(path) => Some(path),
        }
    }
}

fn connect(endpoint: &Endpoint) -> io::Result<LocalStream> {
    LocalStream::connect(endpoint.name()?)
}

fn create_listener(endpoint: &Endpoint) -> io::Result<LocalListener> {
    match ListenerOptions::new().name(endpoint.name()?).create_sync() {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            // A crashed process may have left a filesystem socket behind on
            // Unix platforms that do not use the abstract namespace.
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            if let Some(path) = endpoint.path() {
                let _ = std::fs::remove_file(path);
                return ListenerOptions::new().name(endpoint.name()?).create_sync();
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}

fn endpoint_for(app_id: &str) -> Endpoint {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let hash = stable_hash(format!("{app_id}\n{user}").as_bytes());
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        Endpoint::Namespaced(format!("plico.{hash:016x}"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Endpoint::Path(std::env::temp_dir().join(format!("plico-{hash:016x}.sock")))
    }
}

/// FNV-1a keeps the channel name stable across releases and platforms,
/// unlike `DefaultHasher`, whose seed is not part of its contract.
fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_app_id(label: &str) -> String {
        format!(
            "dev.plico.test.{label}.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    fn validates_url_schemes() {
        assert!(is_url_scheme("plico"));
        assert!(is_url_scheme("a+b-c.d"));
        assert!(!is_url_scheme(""));
        assert!(!is_url_scheme("1plico"));
        assert!(!is_url_scheme("pli co"));
        assert!(!is_url_scheme("plico:"));
    }

    #[test]
    fn extracts_declared_urls_case_insensitively() {
        let activation = AppActivation {
            args: vec![
                "plico://open?id=1".into(),
                "PLICO://other".into(),
                "https://example.com".into(),
                "--flag".into(),
                "C:\\notes.txt".into(),
            ],
            ..Default::default()
        };
        assert_eq!(
            activation.urls(&["plico"]),
            vec!["plico://open?id=1", "PLICO://other"]
        );
        assert!(activation.urls(&[]).is_empty());
    }

    #[test]
    fn resolves_declared_files_relative_to_the_launch_directory() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("note.md"), b"#").unwrap();
        std::fs::write(root.path().join("other.txt"), b"x").unwrap();
        let absolute = root.path().join("absolute.MD");
        std::fs::write(&absolute, b"#").unwrap();
        let activation = AppActivation {
            args: vec![
                "note.md".into(),
                "other.txt".into(),
                absolute.to_string_lossy().into_owned(),
                url::Url::from_file_path(root.path().join("absolute.MD"))
                    .unwrap()
                    .to_string(),
                root.path()
                    .join("missing.md")
                    .to_string_lossy()
                    .into_owned(),
            ],
            working_dir: Some(root.path().to_path_buf()),
            activation_token: None,
        };
        let files = activation.files(&["md"]);
        assert_eq!(files.len(), 3, "unexpected files: {files:?}");
        assert!(
            files
                .iter()
                .all(|path| path.extension().unwrap() == "MD" || path.extension().unwrap() == "md")
        );
        assert!(activation.files(&[".txt"]).len() == 1);
        assert!(activation.files(&[]).is_empty());
    }

    #[test]
    fn forwards_activations_to_the_primary_instance() {
        let app_id = unique_app_id("forward");
        let woken = Arc::new(AtomicBool::new(false));
        let wake_flag = Arc::clone(&woken);
        let primary = match SingleInstance::acquire(
            &app_id,
            &AppActivation::default(),
            Arc::new(move || wake_flag.store(true, Ordering::Release)),
        )
        .unwrap()
        {
            SingleInstanceOutcome::Primary(primary) => primary,
            SingleInstanceOutcome::Secondary => panic!("first acquire was not primary"),
        };
        let activation = AppActivation {
            args: vec!["plico://open?id=42".into()],
            working_dir: Some(PathBuf::from("/tmp")),
            activation_token: Some("token".into()),
        };
        assert!(matches!(
            SingleInstance::acquire(&app_id, &activation, Arc::new(|| {})).unwrap(),
            SingleInstanceOutcome::Secondary
        ));
        let mut primary = primary;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut received = Vec::new();
        while std::time::Instant::now() < deadline && received.is_empty() {
            received = primary.drain();
            if received.is_empty() {
                thread::sleep(Duration::from_millis(10));
            }
        }
        assert_eq!(received, vec![activation]);
        assert!(
            woken.load(Ordering::Acquire),
            "wake callback was not invoked"
        );
    }

    #[test]
    fn dropping_the_primary_releases_the_name() {
        let app_id = unique_app_id("release");
        let primary =
            SingleInstance::acquire(&app_id, &AppActivation::default(), Arc::new(|| {})).unwrap();
        assert!(matches!(primary, SingleInstanceOutcome::Primary(_)));
        drop(primary);
        let next =
            SingleInstance::acquire(&app_id, &AppActivation::default(), Arc::new(|| {})).unwrap();
        assert!(matches!(next, SingleInstanceOutcome::Primary(_)));
    }
}
