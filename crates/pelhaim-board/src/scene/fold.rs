//! Owner, 2026-11-24: one train per line. The engine keeps a train per session (alerts,
//! ETAs and remote stay per session); the board folds a line's sessions into one train
//! each frame, so nothing about the fold is stored.

use pelhaim_core::{
    LEG_BLOCKS, LineKey, Snapshot, StationId, StationStatus, Train, TrainId, TrainState,
};

#[derive(Clone, PartialEq, Debug)]
pub struct LineTrain {
    pub line: LineKey,
    /// The earliest-started session that is not done (else the earliest): its number is
    /// the tag's, and its id is the tag's hit.
    pub lead: TrainId,
    pub number: u16,
    /// Lead first, then by start time.
    pub members: Vec<TrainId>,
    /// The member whose state the train shows, by `state_rank`.
    pub shown: TrainId,
    /// Every member's stations in start order, the oldest folded past `max_stations`.
    pub stations: Vec<StationId>,
    /// Stations folded here plus those the engine already folded per track.
    pub folded: u32,
    /// The station the train runs to: the oldest still active one.
    pub next: Option<usize>,
    /// The head in legs: station `i` sits at `i + 1`, the line ends at `stations.len() + 1`.
    pub at: f32,
    pub berthed: bool,
}

impl LineTrain {
    pub fn has(&self, id: TrainId) -> bool {
        self.members.contains(&id)
    }

    pub fn state<'a>(&self, snap: &'a Snapshot) -> &'a TrainState {
        &snap.trains[&self.shown].state
    }

    pub fn lead<'a>(&self, snap: &'a Snapshot) -> &'a Train {
        &snap.trains[&self.lead]
    }
}

/// Owner order, 2026-11-24: waiting on you > failed > working > stalled > idle > done.
pub fn state_rank(s: &TrainState) -> u8 {
    match s {
        TrainState::Waiting(_) | TrainState::ProbablyWaiting => 0,
        TrainState::Failed(_) => 1,
        TrainState::Working => 2,
        TrainState::Stalled => 3,
        TrainState::Idle => 4,
        TrainState::Done => 5,
    }
}

/// One train per line with any session on it, in line order. DISPATCH draws none (§5.3).
pub fn line_trains(snap: &Snapshot) -> Vec<LineTrain> {
    snap.lines
        .iter()
        .filter(|l| l.key != LineKey::Dispatch)
        .filter_map(|l| fold_line(snap, &l.key, &l.tracks))
        .collect()
}

pub fn line_train_of(snap: &Snapshot, id: TrainId) -> Option<LineTrain> {
    line_trains(snap).into_iter().find(|lt| lt.has(id))
}

fn fold_line(
    snap: &Snapshot,
    key: &LineKey,
    tracks: &[pelhaim_core::TrackId],
) -> Option<LineTrain> {
    let mut members: Vec<&Train> = snap
        .trains
        .values()
        .filter(|t| tracks.contains(&t.track))
        .collect();
    if members.is_empty() {
        return None;
    }
    members.sort_by_key(|t| (t.counters.started, t.id));
    let lead = members
        .iter()
        .find(|t| t.state != TrainState::Done)
        .unwrap_or(&members[0])
        .id;
    let shown = members
        .iter()
        .min_by_key(|t| (state_rank(&t.state), t.counters.started, t.id))
        .expect("non-empty")
        .id;

    let mut stations: Vec<StationId> = members
        .iter()
        .filter_map(|t| snap.tracks.get(&t.track))
        .flat_map(|tr| tr.stations.iter().copied())
        .filter(|s| snap.stations.contains_key(s))
        .collect();
    stations.sort_by_key(|s| (snap.stations[s].started, *s));
    let cap = snap.max_stations.max(1) as usize;
    let over = stations.len().saturating_sub(cap);
    stations.drain(..over);
    let folded = over as u32
        + members
            .iter()
            .filter_map(|t| snap.tracks.get(&t.track))
            .map(|tr| tr.folded)
            .sum::<u32>();

    let next = stations
        .iter()
        .position(|s| snap.stations[s].status == StationStatus::Active);
    let last_block = f32::from(LEG_BLOCKS - 1);
    let (at, berthed) = match next {
        Some(j) => {
            let st = &snap.stations[&stations[j]];
            // The leg's progress belongs to the session that owns the station.
            let owner = members.iter().find(|t| t.track == st.track);
            let own_leg = snap
                .tracks
                .get(&st.track)
                .and_then(|tr| tr.stations.iter().position(|s| *s == st.id));
            match owner {
                Some(t) if Some(usize::from(t.position.leg)) == own_leg => {
                    let f = (f32::from(t.position.block) / last_block).min(1.0);
                    (j as f32 + f, t.position.berthed && f >= 1.0)
                }
                _ => (j as f32, false),
            }
        }
        None if !stations.is_empty() => (stations.len() as f32, true),
        None => {
            let t = members.iter().find(|t| t.id == lead).expect("lead");
            ((f32::from(t.position.block) / last_block).min(1.0), false)
        }
    };

    let number = snap.trains[&lead].number;
    let mut ordered: Vec<TrainId> = vec![lead];
    ordered.extend(members.iter().map(|t| t.id).filter(|&id| id != lead));
    Some(LineTrain {
        line: key.clone(),
        lead,
        number,
        members: ordered,
        shown,
        stations,
        folded,
        next,
        at,
        berthed,
    })
}
