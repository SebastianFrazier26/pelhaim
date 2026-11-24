use std::sync::Arc;
use std::time::Duration;

use pelhaim_core::{
    AlertEvent, AlertId, Answer, AnswerOutcome, Command, Effect, Engine, Event, Input, Origin,
    Snapshot,
};
use pelhaim_remote::{CompactBoard, RemoteApi, RemoteError, compact};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::keep_awake::{Holder, parse_pmset_ps};
use crate::store_worker::{StoreJob, StoreWorker};

pub const INPUT_CAPACITY: usize = 4096;
const REQUEST_CAPACITY: usize = 64;
const ALERT_CAPACITY: usize = 256;
const SLOW_EVERY_TICKS: u64 = 30;
const PMSET_TIMEOUT: Duration = Duration::from_secs(5);

/// F43: created by the caller, so the store worker can get a Sender before
/// the engine task exists.
pub fn input_channel() -> (mpsc::Sender<Input>, mpsc::Receiver<Input>) {
    mpsc::channel(INPUT_CAPACITY)
}

enum Request {
    Answer(Answer, oneshot::Sender<AnswerOutcome>),
    Dismiss(AlertId, oneshot::Sender<Result<(), RemoteError>>),
}

#[derive(Clone)]
pub struct EngineHandle {
    input: mpsc::Sender<Input>,
    requests: mpsc::Sender<Request>,
    alerts: broadcast::Sender<AlertEvent>,
    snapshot: watch::Receiver<Arc<Snapshot>>,
}

impl EngineHandle {
    /// `rt` needs time and IO enabled (`pmset` runs as a tokio child). The task
    /// stops once every handle is dropped; it then drops the store worker
    /// (flushing its queue) and the holder (releasing any hold).
    pub fn spawn(
        rt: &tokio::runtime::Handle,
        engine: Engine,
        store: StoreWorker,
        holder: Holder,
        input: (mpsc::Sender<Input>, mpsc::Receiver<Input>),
    ) -> EngineHandle {
        let (input_tx, input_rx) = input;
        let (req_tx, req_rx) = mpsc::channel(REQUEST_CAPACITY);
        let (alert_tx, _) = broadcast::channel(ALERT_CAPACITY);
        let (snap_tx, snap_rx) = watch::channel(Arc::new(engine.snapshot()));
        let (power_tx, power_rx) = mpsc::channel(1);
        let task = EngineTask {
            published: engine.version(),
            engine,
            store,
            mechanism: holder.mechanism_name().map(str::to_string),
            holder,
            on_battery: None,
            alerts: alert_tx.clone(),
            snapshot: snap_tx,
            power_tx,
        };
        rt.spawn(task.run(input_rx, req_rx, power_rx));
        EngineHandle {
            input: input_tx,
            requests: req_tx,
            alerts: alert_tx,
            snapshot: snap_rx,
        }
    }

    /// For UI input: never blocks, so a full queue drops the input.
    pub fn send(&self, input: Input) {
        match self.input.try_send(input) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("engine input queue full; input dropped");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("engine stopped; input dropped");
            }
        }
    }

    /// F44: for adapter forwarders, which must not drop events during catch-up.
    pub async fn send_event(&self, input: Input) {
        if self.input.send(input).await.is_err() {
            tracing::warn!("engine stopped; event dropped");
        }
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.borrow().clone()
    }

    /// F45: changes once per new `Snapshot.version`; feeds the UI waker.
    pub fn watch(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshot.clone()
    }

    fn request<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<T>) -> Request,
    ) -> Result<T, RemoteError> {
        let (tx, rx) = oneshot::channel();
        // Both calls panic inside a tokio worker; see `RemoteApi`.
        self.requests
            .blocking_send(make(tx))
            .map_err(|_| RemoteError::EngineStopped)?;
        rx.blocking_recv().map_err(|_| RemoteError::EngineStopped)
    }
}

impl RemoteApi for EngineHandle {
    fn subscribe_alerts(&self) -> broadcast::Receiver<AlertEvent> {
        self.alerts.subscribe()
    }

    fn compact_snapshot(&self) -> CompactBoard {
        compact(&self.snapshot())
    }

    fn answer(&self, a: Answer) -> Result<AnswerOutcome, RemoteError> {
        self.request(|tx| Request::Answer(a, tx))
    }

    /// `origin` is not recorded: a dismissal only parks the alert in the tray.
    fn dismiss(&self, alert_id: AlertId, _origin: Origin) -> Result<(), RemoteError> {
        self.request(|tx| Request::Dismiss(alert_id, tx))?
    }
}

struct EngineTask {
    engine: Engine,
    store: StoreWorker,
    holder: Holder,
    mechanism: Option<String>,
    on_battery: Option<bool>,
    alerts: broadcast::Sender<AlertEvent>,
    snapshot: watch::Sender<Arc<Snapshot>>,
    published: u64,
    power_tx: mpsc::Sender<Option<bool>>,
}

impl EngineTask {
    async fn run(
        mut self,
        mut inputs: mpsc::Receiver<Input>,
        mut requests: mpsc::Receiver<Request>,
        mut power: mpsc::Receiver<Option<bool>>,
    ) {
        // F48: the lamp shows the mechanism from the start, not from the first hold.
        self.step(self.keep_awake_env());
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut ticks: u64 = 0;
        loop {
            // Biased so inputs already queued are folded before a request is
            // answered: an answer sent right after its alert's events sees them.
            tokio::select! {
                biased;
                Some(input) = inputs.recv() => self.step(input),
                req = requests.recv() => match req {
                    Some(r) => self.serve(r),
                    None => break,
                },
                _ = tick.tick() => {
                    if ticks.is_multiple_of(SLOW_EVERY_TICKS) {
                        self.store.send(StoreJob::RefreshOpenCounts);
                        self.probe_power();
                    }
                    ticks += 1;
                    self.step(Input::Tick(chrono::Utc::now()));
                }
                Some(on_battery) = power.recv() => {
                    self.on_battery = on_battery;
                    self.step(self.keep_awake_env());
                }
            }
            self.publish();
        }
    }

    fn serve(&mut self, r: Request) {
        match r {
            Request::Answer(a, reply) => {
                let (outcome, effects) = self.engine.answer(a, chrono::Utc::now());
                self.execute(effects);
                self.publish();
                let _ = reply.send(outcome);
            }
            Request::Dismiss(alert, reply) => {
                // F46: `Command::Dismiss` is silent about unknown ids, so check first.
                // The published snapshot is current: it is refreshed after every step.
                let known = self.snapshot.borrow().alerts.iter().any(|a| a.id == alert);
                let result = if known {
                    self.step(Input::Command(Command::Dismiss {
                        alert,
                        at: chrono::Utc::now(),
                    }));
                    self.publish();
                    Ok(())
                } else {
                    Err(RemoteError::UnknownAlert)
                };
                let _ = reply.send(result);
            }
        }
    }

    fn step(&mut self, input: Input) {
        let effects = self.engine.apply(input);
        self.execute(effects);
    }

    fn execute(&mut self, effects: Vec<Effect>) {
        for fx in effects {
            match fx {
                Effect::Persist(ev, at) => self.store.send(StoreJob::Persist(ev, at)),
                Effect::ExportDecision(d) => self.store.send(StoreJob::Export(d)),
                // No subscribers is normal: the phone is an M5 feature.
                Effect::Alert(ev) => {
                    let _ = self.alerts.send(ev);
                }
                Effect::KeepAwake(action) => {
                    self.mechanism = match self.holder.apply(action) {
                        Ok(()) => self.holder.mechanism_name().map(str::to_string),
                        Err(e) => {
                            tracing::warn!(error = %e, "keep-awake mechanism failed");
                            None
                        }
                    };
                    let env = self.keep_awake_env();
                    self.step(env);
                }
            }
        }
    }

    fn keep_awake_env(&self) -> Input {
        Input::Event(Event::KeepAwakeEnv {
            mechanism: self.mechanism.clone(),
            on_battery: self.on_battery,
        })
    }

    fn publish(&mut self) {
        let v = self.engine.version();
        if v != self.published {
            self.published = v;
            self.snapshot.send_replace(Arc::new(self.engine.snapshot()));
        }
    }

    /// Runs beside the loop so a hung `pmset` never stalls the engine.
    fn probe_power(&self) {
        if !cfg!(target_os = "macos") {
            return;
        }
        let tx = self.power_tx.clone();
        tokio::spawn(async move {
            let run = tokio::process::Command::new("/usr/bin/pmset")
                .args(["-g", "ps"])
                .kill_on_drop(true)
                .output();
            let on_battery = match tokio::time::timeout(PMSET_TIMEOUT, run).await {
                Ok(Ok(out)) => parse_pmset_ps(&String::from_utf8_lossy(&out.stdout)),
                _ => None,
            };
            let _ = tx.try_send(on_battery);
        });
    }
}
