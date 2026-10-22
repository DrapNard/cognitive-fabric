use super::*;
use crate::{
    config::Config,
    planning::{Plan, Subtask},
};
use std::{
    future::{poll_fn, Future},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

fn graph(raw: &str) -> Vec<Subtask> {
    Plan::parse(raw)
        .unwrap()
        .steps
        .into_iter()
        .map(|step| Subtask {
            step,
            state: "pending".into(),
            result: None,
        })
        .collect()
}

#[test]
fn configuration_omission_preserves_legacy_serial_defaults_and_caps_are_bounded() {
    let default = Config::default();
    let encoded = serde_json::to_value(&default).unwrap();
    assert!(encoded.get("subtask_parallelism").is_none());
    let legacy: Config = serde_json::from_value(encoded).unwrap();
    assert_eq!(legacy.subtask_parallelism, None);
    assert_eq!(validate_parallelism(legacy.subtask_parallelism).unwrap(), 1);

    for value in 1..=4 {
        let config = Config {
            subtask_parallelism: Some(value),
            ..Config::default()
        };
        config.validate().unwrap();
        let roundtrip: Config =
            serde_json::from_value(serde_json::to_value(config).unwrap()).unwrap();
        assert_eq!(roundtrip.subtask_parallelism, Some(value));
        assert_eq!(validate_parallelism(Some(value)).unwrap(), value);
    }
    for value in [0, 5, usize::MAX] {
        let config = Config {
            subtask_parallelism: Some(value),
            ..Config::default()
        };
        assert!(config.validate().is_err());
        assert!(validate_parallelism(Some(value)).is_err());
    }
}

#[test]
fn ready_waves_skip_blocked_nodes_then_retain_scheduler_order() {
    let nodes = graph(
        r#"{"steps":[{"id":"a","objective":"A","depends_on":[]},{"id":"c","objective":"C","depends_on":["a"]},{"id":"d","objective":"D","depends_on":[]}]}"#,
    );
    let order = Plan {
        steps: nodes.iter().map(|node| node.step.clone()).collect(),
    }
    .order()
    .unwrap();
    assert_eq!(order, vec![0, 1, 2]);
    assert_eq!(
        ready_waves(&nodes, &order, 2).unwrap(),
        vec![vec![0, 2], vec![1]]
    );
}

#[test]
fn ready_wave_width_respects_each_cap_without_reordering() {
    let nodes = graph(
        r#"{"steps":[{"id":"a","objective":"A","depends_on":[]},{"id":"b","objective":"B","depends_on":[]},{"id":"c","objective":"C","depends_on":[]},{"id":"d","objective":"D","depends_on":[]}]}"#,
    );
    for (cap, expected) in [
        (1, vec![vec![0], vec![1], vec![2], vec![3]]),
        (2, vec![vec![0, 1], vec![2, 3]]),
        (3, vec![vec![0, 1, 2], vec![3]]),
        (4, vec![vec![0, 1, 2, 3]]),
    ] {
        assert_eq!(ready_waves(&nodes, &[0, 1, 2, 3], cap).unwrap(), expected);
    }
}

#[test]
fn ready_waves_accept_completed_dependencies_and_reject_invalid_orders_or_states() {
    let mut nodes = graph(
        r#"{"steps":[{"id":"a","objective":"A","depends_on":[]},{"id":"b","objective":"B","depends_on":["a"]}]}"#,
    );
    nodes[0].state = "unverified".into();
    nodes[0].result = Some("untrusted advice".into());
    assert_eq!(ready_waves(&nodes, &[0, 1], 2).unwrap(), vec![vec![1]]);

    let pending = graph(
        r#"{"steps":[{"id":"a","objective":"A","depends_on":[]},{"id":"b","objective":"B","depends_on":["a"]}]}"#,
    );
    assert!(ready_waves(&pending, &[0, 0], 2).is_err());
    assert!(ready_waves(&pending, &[0, 2], 2).is_err());
    assert!(ready_waves(&pending, &[1, 0], 2).is_err());
    let mut running = pending;
    running[0].state = "running".into();
    assert!(ready_waves(&running, &[0, 1], 2).is_err());
}

#[test]
fn ready_waves_reject_invalid_caps() {
    let nodes = graph(r#"{"steps":[{"id":"a","objective":"A","depends_on":[]}] }"#);
    assert!(ready_waves(&nodes, &[0], 0).is_err());
    assert!(ready_waves(&nodes, &[0], 5).is_err());
}

#[tokio::test]
async fn locally_joined_future_results_keep_input_order_when_completion_is_reversed() {
    let futures: Vec<Pin<Box<dyn Future<Output = &'static str> + Send>>> = vec![
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            "scheduled first"
        }),
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(1)).await;
            "scheduled second"
        }),
    ];
    assert_eq!(
        join_all(futures).await,
        vec!["scheduled first", "scheduled second"]
    );
}

struct PendingDrop {
    polled: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}

impl Future for PendingDrop {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.polled.fetch_add(1, Ordering::SeqCst);
        Poll::Pending
    }
}

impl Drop for PendingDrop {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn dropping_local_join_drops_every_incomplete_backend_future() {
    let polled = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let futures: Vec<Pin<Box<dyn Future<Output = ()> + Send>>> = (0..3)
        .map(|_| {
            Box::pin(PendingDrop {
                polled: polled.clone(),
                dropped: dropped.clone(),
            }) as Pin<Box<dyn Future<Output = ()> + Send>>
        })
        .collect();
    let mut joined = Box::pin(join_all(futures));
    poll_fn(|context| {
        assert!(joined.as_mut().poll(context).is_pending());
        Poll::<()>::Ready(())
    })
    .await;
    assert_eq!(polled.load(Ordering::SeqCst), 3);
    drop(joined);
    assert_eq!(dropped.load(Ordering::SeqCst), 3);
}
