use pelhaim_core::{AlertKind, AlertState, Snapshot, TagResolution, Train, format_train_number};

use crate::input::ViewState;
use crate::scene::popup::pick_projects;
use crate::scene::{
    LineTrain, agents_words, line_name, line_trains, state_text, state_words, train_line,
};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct A11yNode {
    pub id: String,
    pub role: A11yRole,
    pub name: String,
    pub children: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum A11yRole {
    Window,
    Group,
    Train,
    Dialog,
    Checkbox,
    Button,
    TextInput,
}

pub const WINDOW_ID: &str = "window";
pub const CONSOLE_ID: &str = "console";

pub fn train_name(snap: &Snapshot, t: &Train) -> String {
    let mut s = format!(
        "Train {}, {}, {}",
        format_train_number(t.number),
        train_line(snap, t),
        state_words(t, snap)
    );
    // F31: a mis-tag is otherwise only a red outline.
    if let TagResolution::MisTagged(slug) = &t.tag {
        s.push_str(&format!(", tag: {slug} (unknown project)"));
    }
    s
}

/// A line train: its lead's number, the shown state, and the agents inside.
pub fn line_train_name(snap: &Snapshot, lt: &LineTrain) -> String {
    let lead = lt.lead(snap);
    let mut s = format!(
        "Train {}, {}, {}",
        format_train_number(lt.number),
        train_line(snap, lead),
        state_text(lt.state(snap), snap)
    );
    let members: Vec<&Train> = lt
        .members
        .iter()
        .filter_map(|id| snap.trains.get(id))
        .collect();
    if members.len() > 1 {
        let inside: Vec<String> = members
            .iter()
            .map(|m| {
                format!(
                    "{} {} ({})",
                    format_train_number(m.number),
                    m.label,
                    state_words(m, snap)
                )
            })
            .collect();
        s.push_str(&format!(
            ", {}: {}",
            agents_words(members.len()),
            inside.join("; ")
        ));
    }
    for m in &members {
        // F31: a mis-tag is otherwise only a red outline.
        if let TagResolution::MisTagged(slug) = &m.tag {
            s.push_str(&format!(", tag: {slug} (unknown project)"));
        }
    }
    s
}

/// The window node first; every other node is reachable from it through `children`.
pub fn a11y_tree(snap: &Snapshot, view: &ViewState) -> Vec<A11yNode> {
    let node = |id: String, role, name: String, children| A11yNode {
        id,
        role,
        name,
        children,
    };
    let mut out = vec![node(
        WINDOW_ID.into(),
        A11yRole::Window,
        "pelhaim".into(),
        vec![],
    )];
    let mut top = Vec::new();
    let trains = line_trains(snap);
    for (li, line) in snap.lines.iter().enumerate() {
        let id = format!("line:{li}");
        let mut kids = Vec::new();
        for lt in trains.iter().filter(|lt| lt.line == line.key) {
            let tid = format!("train:{}", lt.lead.0);
            kids.push(tid.clone());
            out.push(node(
                tid,
                A11yRole::Train,
                line_train_name(snap, lt),
                vec![],
            ));
        }
        top.push(id.clone());
        out.push(node(id, A11yRole::Group, line.name.clone(), kids));
    }

    if let Some(ps) = &view.popup
        && let Some(a) = snap.alerts.iter().find(|a| a.id == ps.alert)
    {
        let id = format!("dialog:{}", a.id.0);
        let train = a.train.and_then(|t| snap.trains.get(&t));
        let mut name = match train {
            Some(t) => format!(
                "{} alert, Train {}, {}",
                a.kind.tab(),
                format_train_number(t.number),
                train_line(snap, t)
            ),
            None => format!("{} alert, {}", a.kind.tab(), line_name(snap, &a.line)),
        };
        let labels: Vec<String> = if ps.picking_project {
            name.push_str(": log the decision under which project?");
            pick_projects(snap).into_iter().map(|(_, n)| n).collect()
        } else {
            match &a.kind {
                AlertKind::Question { questions } => {
                    if let Some(q) = questions.first() {
                        name.push_str(&format!(": {}", q.question));
                    }
                    questions
                        .iter()
                        .flat_map(|q| q.options.iter().map(|o| o.label.clone()))
                        .collect()
                }
                AlertKind::AssignProject { options, .. } => {
                    name.push_str(": which project is this train for?");
                    options.iter().map(|o| o.label.clone()).collect()
                }
                AlertKind::Failure { reason } => {
                    name.push_str(&format!(": {reason}"));
                    vec![]
                }
                AlertKind::Result { .. } | AlertKind::Stalled => vec![],
            }
        };
        let mut kids = Vec::new();
        for (i, label) in labels.iter().enumerate() {
            let cid = format!("{id}:option:{i}");
            let mark = if ps.highlighted == Some(i) {
                ", selected"
            } else {
                ""
            };
            kids.push(cid.clone());
            out.push(node(
                cid,
                A11yRole::Checkbox,
                format!("{} {label}{mark}", i + 1),
                vec![],
            ));
        }
        let mut button = |suffix: &str, label: &str| {
            let bid = format!("{id}:{suffix}");
            kids.push(bid.clone());
            out.push(node(bid, A11yRole::Button, label.into(), vec![]));
        };
        let answered = matches!(a.state, AlertState::Answered { .. });
        if answered && a.decision.is_none() && !matches!(a.kind, AlertKind::Result { .. }) {
            button("log", "Log decision");
        }
        if !a.answerable && train.is_some() && matches!(a.kind, AlertKind::Question { .. }) {
            button("copy", "Copy session id");
        }
        button("close", "Close to tray");
        top.push(id.clone());
        out.push(node(id, A11yRole::Dialog, name, kids));
    }

    top.push(CONSOLE_ID.into());
    out.push(node(
        CONSOLE_ID.into(),
        A11yRole::TextInput,
        "DISPATCH console".into(),
        vec![],
    ));
    out[0].children = top;
    out
}
