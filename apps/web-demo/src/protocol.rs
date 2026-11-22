//! Binary contract of the split counter demo, shared by its guest and browser host.
use plico::{FrameTime, PathKey, Theme, ViewNode};
use plico_dev::StateSnapshot;
use rkyv::{Archive, Deserialize, Serialize};

pub const ABI: u32 = 3;
pub const CAPACITY: usize = 1 << 20;

#[derive(Clone, Default, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
pub struct ServiceSnapshot {
    pub instance: u64,
    pub progress: u64,
}

#[derive(Default, Archive, Serialize, Deserialize)]
pub struct Input {
    pub base: Option<u64>,
    pub services: ServiceSnapshot,
    pub state: StateSnapshot,
    pub viewport: Option<[f32; 2]>,
    pub hovered: Option<Vec<PathKey>>,
    pub scroll_viewports: Vec<(Vec<PathKey>, [f32; 2])>,
    pub time: FrameTime,
    pub clicks: Vec<Vec<PathKey>>,
    pub values: Vec<(Vec<PathKey>, f64)>,
    pub key_events: Vec<plico::KeyEvent>,
}

#[derive(Archive, Serialize, Deserialize)]
pub struct Output<N = Vec<ViewNode>> {
    pub revision: u64,
    pub version: u32,
    pub state: StateSnapshot,
    pub theme: Theme,
    pub nodes: N,
    pub reset: Option<String>,
    pub key_handled: bool,
    pub requests: u16,
}

pub fn render(input: Input) -> plico_dev::Result<Output> {
    let frame = plico_dev::services::render::<super::Counter>(plico_dev::Request {
        services: serde_json::json!(input.services),
        state: input.state,
        viewport: input.viewport,
        hovered: input.hovered,
        scroll_viewports: input.scroll_viewports,
        time: input.time,
        clicks: input.clicks,
        values: input.values,
        key_events: input.key_events,
        ..Default::default()
    })?;
    if !frame.commands.is_empty() {
        return Err(plico_dev::Error::Module {
            message: "split counter has no command executor".into(),
        });
    }
    Ok(Output {
        revision: 0,
        version: frame.version,
        state: frame.state,
        theme: frame.theme,
        nodes: frame.nodes,
        reset: frame.reset,
        key_handled: frame.key_handled,
        requests: frame.requests.bits(),
    })
}

impl<N> Output<N> {
    pub fn map_nodes<T>(self, transform: impl FnOnce(N) -> T) -> Output<T> {
        Output {
            revision: self.revision,
            version: self.version,
            state: self.state,
            theme: self.theme,
            nodes: transform(self.nodes),
            reset: self.reset,
            key_handled: self.key_handled,
            requests: self.requests,
        }
    }
}

pub type Reply = Output<plico_dev::tree_update::TreeUpdate>;

impl Reply {
    pub fn resolve(
        self,
        base: Option<u64>,
        previous: &mut Vec<ViewNode>,
    ) -> plico_dev::Result<Output> {
        use plico_dev::tree_update::TreeUpdate;
        fn portable(nodes: &[ViewNode]) -> bool {
            nodes.iter().all(|n| {
                !matches!(n.kind, plico::Kind::Shader | plico::Kind::TextInput)
                    && portable(&n.children)
            })
        }
        if self.version != plico_dev::PROTOCOL_VERSION {
            return Err(plico_dev::Error::Protocol {
                actual: self.version,
            });
        }
        let full = matches!(self.nodes, TreeUpdate::Full(_));
        if !full && base.and_then(|r| r.checked_add(1)) != Some(self.revision) {
            return Err(plico_dev::Error::Tree {
                message: "text update has no matching baseline".into(),
            });
        }
        let Output {
            revision,
            version,
            state,
            theme,
            nodes,
            reset,
            key_handled,
            requests,
        } = self;
        let nodes = nodes.resolve(previous)?;
        if full && !portable(&nodes) {
            return Err(plico_dev::Error::Tree {
                message: "unsupported portable web node".into(),
            });
        }
        Ok(Output {
            revision,
            version,
            state,
            theme,
            nodes,
            reset,
            key_handled,
            requests,
        })
    }
}
