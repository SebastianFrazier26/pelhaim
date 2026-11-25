use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use pelhaim_board::a11y::{CONSOLE_ID, WINDOW_ID};
use pelhaim_board::{A11yNode, A11yRole, ViewState};

/// A stable AccessKit id for a board node id, so a node keeps its identity across updates.
pub fn node_id(id: &str) -> NodeId {
    // FNV-1a: deterministic across runs, unlike std's randomly seeded hasher.
    let h = id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    NodeId(h)
}

/// The node keyboard input goes to. The console keeps focus while a popup is open
/// (Review Focus 5), matching `on_input`, which routes keys to a focused console first.
pub fn focus_id(view: &ViewState, nodes: &[A11yNode]) -> String {
    let present = |id: &str| nodes.iter().any(|n| n.id == id);
    let dialog = view.popup.as_ref().map(|p| format!("dialog:{}", p.alert.0));
    let train = view.selected.map(|t| format!("train:{}", t.0));
    if view.console_focused {
        return CONSOLE_ID.into();
    }
    [dialog, train]
        .into_iter()
        .flatten()
        .find(|id| present(id))
        .unwrap_or_else(|| WINDOW_ID.into())
}

pub fn tree_update(nodes: &[A11yNode], focus: &str) -> TreeUpdate {
    let nodes = nodes
        .iter()
        .map(|n| {
            let mut node = Node::new(role(n.role));
            node.set_label(n.name.as_str());
            node.set_children(n.children.iter().map(|c| node_id(c)).collect::<Vec<_>>());
            (node_id(&n.id), node)
        })
        .collect();
    TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(node_id(WINDOW_ID))),
        tree_id: TreeId::ROOT,
        focus: node_id(focus),
    }
}

fn role(r: A11yRole) -> Role {
    match r {
        A11yRole::Window => Role::Window,
        A11yRole::Group => Role::List,
        A11yRole::Train => Role::ListItem,
        A11yRole::Dialog => Role::Dialog,
        A11yRole::Checkbox => Role::CheckBox,
        A11yRole::Button => Role::Button,
        A11yRole::TextInput => Role::TextInput,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelhaim_board::{PopupState, a11y_tree, sync};
    use pelhaim_core::testkit::sample_board;

    #[test]
    fn tree_update_names_match_board() {
        let snap = sample_board();
        let view = ViewState::default();
        let nodes = a11y_tree(&snap, &view);
        let up = tree_update(&nodes, &focus_id(&view, &nodes));
        assert_eq!(up.nodes.len(), nodes.len());
        assert_eq!(up.tree.as_ref().map(|t| t.root), Some(node_id(WINDOW_ID)));
        assert_eq!(up.focus, node_id(WINDOW_ID));
        let named = |want: &str| {
            up.nodes
                .iter()
                .find(|(_, n)| n.label() == Some(want))
                .map(|(id, n)| (*id, n.role()))
        };
        let (train, role) = named(
            "Train 1423, SAFEASCENT, waiting on you: Proceed to C1?, 2 agents: \
             1423 Claude session (waiting on you: Proceed to C1?); \
             1424 Risk model check (working)",
        )
        .expect("train node");
        assert_eq!(role, Role::ListItem);
        let line = up
            .nodes
            .iter()
            .find(|(_, n)| n.children().contains(&train))
            .expect("the train hangs off its line");
        assert_eq!(line.1.role(), Role::List);
        assert_eq!(
            named("DISPATCH console").map(|n| n.1),
            Some(Role::TextInput)
        );
    }

    #[test]
    fn node_ids_are_stable_and_distinct() {
        assert_eq!(node_id("train:3"), node_id("train:3"));
        assert_ne!(node_id("train:3"), node_id("train:4"));
        assert_ne!(node_id(WINDOW_ID), node_id(CONSOLE_ID));
    }

    #[test]
    fn popup_never_takes_focus_from_the_console() {
        let snap = sample_board();
        let mut view = ViewState {
            console_focused: true,
            ..ViewState::default()
        };
        sync(&mut view, &snap);
        assert!(view.popup.is_some(), "sample_board auto-opens an alert");
        assert!(view.console_focused);
        let nodes = a11y_tree(&snap, &view);
        assert_eq!(focus_id(&view, &nodes), CONSOLE_ID);

        view.console_focused = false;
        let nodes = a11y_tree(&snap, &view);
        let ps: &PopupState = view.popup.as_ref().unwrap();
        assert_eq!(focus_id(&view, &nodes), format!("dialog:{}", ps.alert.0));
    }
}
