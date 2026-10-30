//! The core against a real relay and a release daemon (`scripts/core-e2e.mjs` starts both and
//! runs these with `--ignored --test-threads=1`). Inputs, from the script:
//!
//! - `WADO_E2E_LINK` … `WADO_E2E_LINK4`: four single-use pairing links minted by `wado qr` for the
//!   local relay, one or two per test so each stands alone;
//! - `WADO_E2E_CFG`: the daemon's config dir (its `wado/trusted_clients`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use wado_client_core::core::Core;
use wado_client_core::event::CoreEvent;
use wado_client_core::link::state::LinkState;
use wado_client_core::pair::flow::PairStep;
use wado_client_core::secret::SecretBox;

struct Xor;
impl SecretBox for Xor {
    fn wrap(&self, p: Vec<u8>) -> Result<Vec<u8>, String> {
        Ok(p.into_iter().map(|b| b ^ 0x3c).collect())
    }
    fn unwrap(&self, s: Vec<u8>) -> Result<Vec<u8>, String> {
        self.wrap(s)
    }
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("{k} is set by scripts/core-e2e.mjs"))
}

fn fresh(tag: &str) -> Core {
    let dir = PathBuf::from(env("WADO_E2E_CFG")).join(format!("core-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    Core::new(dir, Arc::new(Xor), format!("Core e2e {tag}")).unwrap()
}

/// Wait up to `ms` for an event `pick` accepts.
fn wait<T>(core: &Core, ms: u64, mut pick: impl FnMut(&CoreEvent) -> Option<T>) -> Option<T> {
    let until = Instant::now() + Duration::from_millis(ms);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            let Ok(ev) = tokio::time::timeout(left, core.next_event()).await else {
                break;
            };
            eprintln!("  event: {ev:?}");
            if let Some(t) = pick(&ev) {
                return Some(t);
            }
        }
        None
    })
}

/// Pair `core` with the link in `var`: the key, or why not.
fn pair(core: &Core, var: &str, ms: u64) -> Result<String, String> {
    let op = core.pair(env(var));
    wait(core, ms, |e| match e {
        CoreEvent::Pair {
            op: o,
            step: PairStep::Done { key },
        } if *o == op => Some(Ok(key.clone())),
        CoreEvent::Pair {
            op: o,
            step: PairStep::Failed { why },
        } if *o == op => Some(Err(why.clone())),
        _ => None,
    })
    .unwrap_or_else(|| Err("no outcome in time".into()))
}

fn open_within(core: &Core, key: &str, ms: u64) -> Option<u128> {
    let t0 = Instant::now();
    wait(core, ms, |e| match e {
        CoreEvent::Link { key: k, view } if k == key => {
            matches!(view.state, LinkState::Open { .. }).then(|| t0.elapsed().as_millis())
        }
        _ => None,
    })
}

#[test]
#[ignore = "needs a relay and a release daemon: scripts/core-e2e.mjs"]
fn pairs_then_watches_and_reads_host() {
    let a = fresh("a");
    let t0 = Instant::now();
    let key = pair(&a, "WADO_E2E_LINK", 10_000).expect("pairs within 10 s");
    eprintln!("budget: paired in {} ms", t0.elapsed().as_millis());
    let t1 = Instant::now();
    a.watch(vec![key.clone()]);
    let open = open_within(&a, &key, 5000).expect("Link{Open} within 5 s");
    eprintln!("budget: control room open in {open} ms");
    let host = wait(&a, 5000, |e| match e {
        CoreEvent::Host { key: k, view } if *k == key => Some(view.clone()),
        _ => None,
    })
    .expect("Host within 5 s");
    eprintln!(
        "budget: watch → host state in {} ms",
        t1.elapsed().as_millis()
    );
    assert!(!host.os.is_empty(), "{host:?}");
    assert!(!host.logo.is_empty(), "{host:?}");
    let trusted =
        std::fs::read_to_string(PathBuf::from(env("WADO_E2E_CFG")).join("wado/trusted_clients"))
            .unwrap();
    assert!(
        trusted.lines().any(|l| l.starts_with(&a.client_key())),
        "the core's key on the trust list:\n{trusted}"
    );
    a.shutdown();
}

#[test]
#[ignore = "needs a relay and a release daemon: scripts/core-e2e.mjs"]
fn second_core_cannot_use_spent_code() {
    let first = fresh("spend");
    pair(&first, "WADO_E2E_LINK2", 10_000).expect("the first use pairs");
    first.shutdown();
    let second = fresh("spent");
    let why = pair(&second, "WADO_E2E_LINK2", 15_000).expect_err("a spent code does not pair");
    eprintln!("refused: {why}");
    assert!(second.computers().is_empty());
    second.shutdown();
}

#[test]
#[ignore = "needs a relay and a release daemon: scripts/core-e2e.mjs"]
fn control_room_takes_no_seat() {
    let a = fresh("watcher");
    let key = pair(&a, "WADO_E2E_LINK3", 10_000).expect("A pairs");
    a.watch(vec![key.clone()]);
    open_within(&a, &key, 5000).expect("A's control room opens");
    // While A holds its control room, a second device pairs — which takes the seat. Had A's room
    // been a seat, this would be refused as busy.
    let b = fresh("b");
    let k2 = pair(&b, "WADO_E2E_LINK4", 10_000).expect("B pairs while A watches");
    assert_eq!(k2, key, "the same computer");
    b.shutdown();
    a.shutdown();
}
