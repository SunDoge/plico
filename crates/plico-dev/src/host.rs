//! Host-owned libraries, serialized state and last-good activation.
use crate::{Frame, PROTOCOL_VERSION, RenderFn, Request};
use libloading::Library;
use snafu::{ResultExt, Snafu};
use stabby::libloading::StabbyLibrary;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("module IO failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("cannot load application module: {source}"))]
    Load { source: libloading::Error },
    #[snafu(display("application ABI layout check failed: {source}"))]
    Symbol {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[snafu(display("application data failed: {source}"))]
    Data { source: crate::Error },
    #[snafu(display("invalid module JSON: {source}"))]
    Json { source: serde_json::Error },
    #[snafu(display("invalid application tree: {message}"))]
    Tree { message: String },
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Descriptor {
    pub version: u32,
    pub library: PathBuf,
}
impl Descriptor {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).context(IoSnafu)?;
        serde_json::from_slice(&bytes).context(JsonSnafu)
    }
}
pub struct Module {
    // Deliberately keep native code mapped until process exit: TLS/destructor pointers
    // from older revisions must not jump into an unloaded image.
    library: std::mem::ManuallyDrop<Library>,
    render: RenderFn,
    _image: tempfile::TempPath,
}
impl Module {
    /// Only load trusted Plico applications: a native library can execute arbitrary code.
    pub fn load(path: &Path) -> Result<Self> {
        let mut image = tempfile::Builder::new()
            .prefix("plico-")
            .suffix(&format!(".{}", std::env::consts::DLL_EXTENSION))
            .tempfile()
            .context(IoSnafu)?;
        image
            .write_all(&std::fs::read(path).context(IoSnafu)?)
            .context(IoSnafu)?;
        let image = image.into_temp_path();
        // SAFETY: the path is a private complete copy; entry points are checked against
        // the documented Plico C ABI. The library is retained for every pointer call.
        let library = unsafe { Library::new(&*image) }.context(LoadSnafu)?;
        let render = *unsafe { library.get_stabbied::<RenderFn>(b"plico_app_render") }
            .context(SymbolSnafu)?;
        Ok(Self {
            library: std::mem::ManuallyDrop::new(library),
            render,
            _image: image,
        })
    }
    pub fn set_wake(&self, token: u64, wake: crate::WakeFn) -> Result<()> {
        // SAFETY: checked function ABI; the library remains mapped until process exit.
        let register = *unsafe {
            self.library
                .get_stabbied::<crate::SetWakeFn>(b"plico_app_set_wake")
        }
        .context(SymbolSnafu)?;
        if register(token, wake) != 0 {
            return Err(crate::Error::Module {
                message: "module rejected wake registration".into(),
            })
            .context(DataSnafu);
        }
        Ok(())
    }
    /// Versioned byte transport shared by the legacy and reactive protocols.
    /// The caller validates its own protocol and payload after decoding.
    pub fn call_json<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        request: &T,
    ) -> Result<R> {
        let input = serde_json::to_vec(request).context(JsonSnafu)?;
        let reply = (self.render)(input.into_iter().collect());
        if reply.status != 0 {
            return Err(crate::Error::Module {
                message: String::from_utf8_lossy(&reply.bytes).into_owned(),
            })
            .context(DataSnafu);
        }
        serde_json::from_slice(&reply.bytes).context(JsonSnafu)
    }
    pub fn call(&self, request: &Request) -> Result<Frame> {
        let input = serde_json::to_vec(request).context(JsonSnafu)?;
        let reply = (self.render)(input.into_iter().collect());
        let status = reply.status;
        let output = reply.bytes;
        if status != 0 {
            return Err(crate::Error::Module {
                message: String::from_utf8_lossy(&output).into_owned(),
            })
            .context(DataSnafu);
        }
        let frame: Frame = serde_json::from_slice(&output).context(JsonSnafu)?;
        if frame.version != PROTOCOL_VERSION {
            return Err(crate::Error::Protocol {
                actual: frame.version,
            })
            .context(DataSnafu);
        }
        validate(&frame.nodes)?;
        Ok(frame)
    }
}
fn validate(nodes: &[plico_core::ViewNode]) -> Result<()> {
    crate::validate_nodes(nodes).map_err(|error| Error::Tree {
        message: error.to_string(),
    })
}
