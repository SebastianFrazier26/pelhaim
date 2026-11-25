use async_trait::async_trait;
use pelhaim_core::Event;
use tokio::sync::mpsc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AdapterKind {
    ObserveClaude,
    ObserveCodex,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Capabilities {
    pub can_answer: bool,
    pub can_take_over: bool,
    pub can_spawn: bool,
    pub read_only: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AdapterCommand {
    /// Rediscover session files now instead of at the next sweep.
    Refresh,
    Shutdown,
}

#[async_trait]
pub trait SessionAdapter: Send {
    fn kind(&self) -> AdapterKind;
    fn capabilities(&self) -> Capabilities;
    /// Runs until `Shutdown`, until either channel closes, or, for a one-shot adapter, until caught up.
    async fn run(
        self: Box<Self>,
        tx: mpsc::Sender<Event>,
        rx: mpsc::Receiver<AdapterCommand>,
    ) -> anyhow::Result<()>;
}
