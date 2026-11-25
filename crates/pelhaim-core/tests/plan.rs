use std::collections::{BTreeMap, BTreeSet};

use pelhaim_core::plan::{
    Check, FlagReason, Plan, PlanError, PlanStatus, PlanStep, SignoffFlag, apply_derived_flags,
    derive_flags, effective_deps, layout, projected_finish, requires_signoff, validate,
};
use pelhaim_core::{PlanId, Tool};
use proptest::prelude::*;

fn step(id: &str, deps: &[&str]) -> PlanStep {
    PlanStep {
        id: id.into(),
        title: format!("Step {id}"),
        detail: String::new(),
        depends_on: deps.iter().map(|d| (*d).into()).collect(),
        tool: Tool::Claude,
        model: None,
        estimate_hours: Some(1.0),
        design: false,
        security: false,
        signoff: Vec::new(),
        acceptance: Vec::new(),
        keyword_override: false,
        skip: false,
    }
}

fn plan(steps: Vec<PlanStep>) -> Plan {
    Plan {
        id: PlanId(1),
        project: "cabn".into(),
        goal: "Ship it".into(),
        steps,
        status: PlanStatus::Draft,
        revision: 0,
    }
}

fn fork() -> Plan {
    plan(vec![
        step("a", &[]),
        step("b", &["a"]),
        step("c", &["a"]),
        step("d", &["b", "c"]),
    ])
}

fn titled(title: &str) -> PlanStep {
    PlanStep {
        title: title.into(),
        ..step("s", &[])
    }
}

fn with_id(id: &str) -> PlanStep {
    PlanStep {
        title: "Do it".into(),
        ..step(id, &[])
    }
}

fn errors(p: &Plan) -> Vec<PlanError> {
    validate(p).expect_err("plan should be invalid")
}

#[test]
fn valid_four_step_fork_plan_passes() {
    assert_eq!(validate(&fork()), Ok(()));
}

#[test]
fn structural_errors_reported_together() {
    let p = plan(vec![
        step("a", &[]),
        step("a", &[]),
        step("b", &["zz"]),
        step("c", &["c"]),
        step("x", &["y"]),
        step("y", &["x"]),
    ]);
    assert_eq!(
        errors(&p),
        vec![
            PlanError::DuplicateId { step: "a".into() },
            PlanError::UnknownDependency {
                step: "b".into(),
                dep: "zz".into()
            },
            PlanError::SelfDependency { step: "c".into() },
            PlanError::Cycle {
                steps: vec!["x".into(), "y".into()]
            },
        ]
    );
}

#[test]
fn cycle_excludes_steps_merely_downstream_of_it() {
    let p = plan(vec![
        step("a", &[]),
        step("x", &["a", "y"]),
        step("y", &["x"]),
        step("z", &["y"]),
    ]);
    assert_eq!(
        errors(&p),
        vec![PlanError::Cycle {
            steps: vec!["x".into(), "y".into()]
        }]
    );
}

#[test]
fn no_root_reported() {
    let p = plan(vec![step("x", &["y"]), step("y", &["x"])]);
    assert_eq!(
        errors(&p),
        vec![
            PlanError::Cycle {
                steps: vec!["x".into(), "y".into()]
            },
            PlanError::NoRoot,
        ]
    );
}

#[test]
fn empty_plan() {
    assert_eq!(errors(&plan(vec![])), vec![PlanError::Empty]);
}

#[test]
fn invalid_step_id_and_bad_title() {
    for id in ["Bad Id", "", "under_score", &"a".repeat(41)] {
        assert_eq!(
            errors(&plan(vec![with_id(id)])),
            vec![PlanError::InvalidId { step: id.into() }],
            "{id:?}"
        );
    }
    assert_eq!(validate(&plan(vec![with_id(&"a".repeat(40))])), Ok(()));
    for title in [
        "t".repeat(33),
        "a\tb".into(),
        String::new(),
        " lead".into(),
        "Do it ~2h".into(),
        "~2h".into(),
    ] {
        assert_eq!(
            errors(&plan(vec![titled(&title)])),
            vec![PlanError::BadTitle { step: "s".into() }],
            "{title:?}"
        );
    }
    assert_eq!(validate(&plan(vec![titled(&"é".repeat(32))])), Ok(()));
}

#[test]
fn estimate_bounds() {
    for bad in [0.0, -1.0, f64::NAN, 41.0, f64::INFINITY] {
        let s = PlanStep {
            estimate_hours: Some(bad),
            ..step("s", &[])
        };
        assert_eq!(
            errors(&plan(vec![s])),
            vec![PlanError::BadEstimate { step: "s".into() }],
            "{bad}"
        );
    }
    for good in [Some(0.25), Some(40.0), None] {
        let s = PlanStep {
            estimate_hours: good,
            ..step("s", &[])
        };
        assert_eq!(validate(&plan(vec![s])), Ok(()), "{good:?}");
    }
}

#[test]
fn errors_display_names_the_step() {
    let e = PlanError::UnknownDependency {
        step: "b".into(),
        dep: "zz".into(),
    };
    let text = e.to_string();
    assert!(text.contains("b") && text.contains("zz") && !text.contains('\n'));
}

#[test]
fn derived_flags_are_added_not_errors() {
    let mut p = plan(vec![
        PlanStep {
            design: true,
            ..step("a", &[])
        },
        PlanStep {
            title: "Add OAuth token refresh".into(),
            ..step("b", &["a"])
        },
        step("c", &["a"]),
    ]);
    assert_eq!(validate(&p), Ok(()));
    let added = apply_derived_flags(&mut p);
    assert_eq!(
        added,
        BTreeMap::from([
            (
                "a".to_string(),
                vec![(SignoffFlag::Decision, FlagReason::DesignField)]
            ),
            (
                "b".to_string(),
                vec![(
                    SignoffFlag::Security,
                    FlagReason::SecurityKeyword("token".into())
                )]
            ),
        ])
    );
    assert_eq!(p.steps[0].signoff, vec![SignoffFlag::Decision]);
    assert_eq!(p.steps[1].signoff, vec![SignoffFlag::Security]);
    assert!(p.steps[2].signoff.is_empty());
    assert_eq!(validate(&p), Ok(()));
    assert!(apply_derived_flags(&mut p).is_empty());
}

#[test]
fn flags_already_signed_off_are_not_derived_again() {
    let s = PlanStep {
        security: true,
        signoff: vec![SignoffFlag::Security],
        ..step("s", &[])
    };
    assert!(derive_flags(&s).is_empty());
}

#[test]
fn keyword_override_suppresses_keywords_only() {
    let s = PlanStep {
        title: "Rotate the secret and deploy".into(),
        security: true,
        keyword_override: true,
        ..step("s", &[])
    };
    assert_eq!(
        derive_flags(&s),
        vec![(SignoffFlag::Security, FlagReason::SecurityField)]
    );
}

#[test]
fn keywords_match_whole_words_only() {
    for title in ["authority figure", "prompt", "approachable"] {
        assert!(derive_flags(&titled(title)).is_empty(), "{title}");
    }
    let s = PlanStep {
        detail: "Sketch the DATA\tMODEL first".into(),
        ..titled("Plan")
    };
    assert_eq!(
        derive_flags(&s),
        vec![(
            SignoffFlag::Decision,
            FlagReason::DesignKeyword("data model".into())
        )]
    );
    assert!(derive_flags(&titled("data then model")).is_empty());
}

#[test]
fn ship_steps_get_merge_or_prod() {
    assert_eq!(
        derive_flags(&titled("Deploy to production")),
        vec![(
            SignoffFlag::Prod,
            FlagReason::ActionKeyword("deploy".into())
        )]
    );
    assert_eq!(
        derive_flags(&titled("Open PR")),
        vec![(SignoffFlag::Merge, FlagReason::ActionKeyword("pr".into()))]
    );
}

#[test]
fn manual_check_forces_signoff() {
    let mut s = step("s", &[]);
    s.acceptance = vec![Check::Command {
        argv: vec!["cargo".into(), "test".into()],
    }];
    assert!(!requires_signoff(&s));
    s.acceptance.push(Check::Manual {
        text: "Looks right on the board".into(),
    });
    assert!(requires_signoff(&s));
    let flagged = PlanStep {
        signoff: vec![SignoffFlag::Merge],
        ..step("t", &[])
    };
    assert!(requires_signoff(&flagged));
}

#[test]
fn layout_columns_and_chains() {
    let l = layout(&fork());
    assert_eq!(
        l.columns,
        BTreeMap::from([
            ("a".to_string(), 0),
            ("b".to_string(), 1),
            ("c".to_string(), 1),
            ("d".to_string(), 2),
        ])
    );
    assert_eq!(l.chains, vec![vec!["a", "b", "d"], vec!["c"]]);
}

#[test]
fn skipped_step_is_transparent() {
    let p = plan(vec![
        step("a", &[]),
        PlanStep {
            skip: true,
            ..step("b", &["a"])
        },
        step("c", &["b"]),
    ]);
    assert_eq!(effective_deps(&p, "c"), vec!["a"]);
    let l = layout(&p);
    assert_eq!(l.chains, vec![vec!["a", "c"]]);
    assert_eq!(
        l.columns,
        BTreeMap::from([("a".to_string(), 0), ("c".to_string(), 1)])
    );
}

#[test]
fn projected_finish_follows_critical_path() {
    let mut p = plan(vec![
        step("a", &[]),
        step("b", &["a"]),
        step("c", &["a"]),
        step("d", &["b", "c"]),
    ]);
    p.steps[1].estimate_hours = Some(2.0);
    p.steps[2].estimate_hours = Some(0.5);
    let none = BTreeMap::new();
    assert_eq!(projected_finish(&p, 1.5, &none)["d"], 6 * 3600);

    let fixed = BTreeMap::from([("a".to_string(), 1800)]);
    let got = projected_finish(&p, 1.5, &fixed);
    assert_eq!(got["a"], 1800);
    assert_eq!(got["d"], 1800 + 3 * 3600 * 3 / 2);

    p.steps[2].estimate_hours = None;
    let got = projected_finish(&p, 1.5, &fixed);
    assert!(!got.contains_key("c"));
    assert_eq!(got["d"], 1800 + 3 * 3600 * 3 / 2);
}

#[test]
fn projected_finish_skips_skipped_steps() {
    let mut p = plan(vec![step("a", &[]), step("b", &["a"]), step("c", &["b"])]);
    p.steps[1].skip = true;
    let got = projected_finish(&p, 1.0, &BTreeMap::new());
    assert_eq!(
        got,
        BTreeMap::from([("a".to_string(), 3600), ("c".to_string(), 7200)])
    );
}

fn arb_dag(max: usize) -> impl Strategy<Value = Plan> {
    (1..=max)
        .prop_flat_map(|n| {
            (
                Just(n),
                proptest::collection::vec(proptest::collection::vec(any::<bool>(), n), n),
                proptest::collection::vec(proptest::bool::weighted(0.2), n),
            )
        })
        .prop_map(|(n, edges, skips)| {
            let steps = (0..n)
                .map(|i| {
                    let id = format!("s{i}");
                    let deps: Vec<String> = (0..i)
                        .filter(|&j| edges[i][j])
                        .map(|j| format!("s{j}"))
                        .collect();
                    PlanStep {
                        depends_on: deps,
                        skip: skips[i],
                        ..step(&id, &[])
                    }
                })
                .collect();
            plan(steps)
        })
}

proptest! {
    #[test]
    fn layout_respects_edges(p in arb_dag(12)) {
        prop_assert_eq!(validate(&p), Ok(()));
        let l = layout(&p);
        let live: BTreeSet<String> = p.steps.iter().filter(|s| !s.skip).map(|s| s.id.clone()).collect();
        prop_assert_eq!(l.columns.keys().cloned().collect::<BTreeSet<_>>(), live.clone());
        for id in &live {
            for dep in effective_deps(&p, id) {
                prop_assert!(l.columns[&dep] < l.columns[id], "{} -> {}", dep, id);
            }
        }
        let mut seen = BTreeSet::new();
        for chain in &l.chains {
            prop_assert!(!chain.is_empty());
            for w in chain.windows(2) {
                prop_assert!(effective_deps(&p, &w[1]).contains(&w[0]));
            }
            for id in chain {
                prop_assert!(seen.insert(id.clone()), "{} twice", id);
            }
        }
        prop_assert_eq!(seen, live);
    }
}
