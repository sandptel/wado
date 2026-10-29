//! Wi-Fi changes on probation — the policy over the platform's [`WifiRadio`].
//!
//! The dangerous one: on a computer that reaches the internet by Wi-Fi alone, turning it off or
//! joining the wrong network cuts the very connection the viewer is using, and nobody can turn
//! it back from the phone. So every change is put on probation, the way a monitor change asks
//! "keep these settings?": joining a network is undone after [`PROBATION`] unless a viewer
//! confirms it, and turning Wi-Fi off can be for a few minutes only.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use wado_platform::network::WifiRadio;

pub const PROBATION: Duration = Duration::from_secs(60);

/// Bumped by every change and by Keep; a pending revert only acts if it is still current.
static GEN: AtomicU64 = AtomicU64::new(0);
static DEADLINE: Mutex<Option<Instant>> = Mutex::new(None);

fn deadline(at: Option<Instant>) {
    *DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) = at;
}

/// Seconds until a change on probation is undone.
pub fn revert_in() -> Option<u32> {
    let d = *DEADLINE.lock().unwrap_or_else(|e| e.into_inner());
    d.map(|t| t.saturating_duration_since(Instant::now()).as_secs() as u32)
}

/// The radio on or off. Off for `minutes` comes back on by itself.
pub async fn radio(wifi: Arc<dyn WifiRadio>, on: bool, minutes: Option<u32>) -> Result<(), String> {
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    wifi.radio(on).await?;
    if !on {
        if let Some(m) = minutes.filter(|m| *m > 0) {
            let back = Duration::from_secs(u64::from(m.min(240)) * 60);
            deadline(Some(Instant::now() + back));
            tokio::spawn(async move {
                tokio::time::sleep(back).await;
                if GEN.load(Ordering::SeqCst) == g {
                    tracing::info!("turning Wi-Fi back on, as asked");
                    let _ = wifi.radio(true).await;
                    deadline(None);
                }
            });
        }
    } else {
        deadline(None);
    }
    Ok(())
}

/// Join `ssid`, on probation: if nobody confirms within [`PROBATION`], go back to the network
/// this computer was on.
pub async fn connect(
    wifi: Arc<dyn WifiRadio>,
    ssid: String,
    password: Option<String>,
) -> Result<(), String> {
    wifi.fresh(Duration::ZERO).await;
    let previous = wifi
        .state()
        .borrow()
        .as_ref()
        .and_then(|w| w.connected.clone());
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let joined = wifi.join(ssid.clone(), password).await;
    if let (Some(prev), true) = (previous.clone(), previous.as_deref() != Some(ssid.as_str())) {
        deadline(Some(Instant::now() + PROBATION));
        tokio::spawn(async move {
            tokio::time::sleep(PROBATION).await;
            if GEN.load(Ordering::SeqCst) == g {
                tracing::warn!(network = prev, "Wi-Fi change not confirmed — going back");
                let _ = wifi.rejoin(prev).await;
                deadline(None);
            }
        });
    }
    joined
}

/// A viewer is back and says the change is good.
pub fn keep() {
    GEN.fetch_add(1, Ordering::SeqCst);
    deadline(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_platform::fake::Fakes;
    use wado_protocol::host::{Network, Wifi};

    fn on(ssid: &str) -> Option<Wifi> {
        Some(Wifi {
            enabled: true,
            connected: Some(ssid.into()),
            networks: vec![Network {
                ssid: ssid.into(),
                active: true,
                ..Default::default()
            }],
            ..Default::default()
        })
    }

    /// Unconfirmed, a new network is left for the old one; confirmed, it stays. (One test, as
    /// the probation is the daemon's one Wi-Fi.)
    #[tokio::test(start_paused = true)]
    async fn an_unconfirmed_change_goes_back_and_a_kept_one_stays() {
        let f = Fakes::new();
        f.wifi.state.will_read(on("Home"));
        let wifi: Arc<dyn WifiRadio> = f.wifi.clone();
        connect(wifi.clone(), "Cafe".into(), Some("pw".into()))
            .await
            .unwrap();
        assert!(matches!(revert_in(), Some(59 | 60)), "{:?}", revert_in());
        tokio::time::sleep(PROBATION + Duration::from_secs(1)).await;
        assert_eq!(
            f.log.calls(),
            ["wifi.join Cafe (password)", "wifi.rejoin Home"]
        );
        assert_eq!(revert_in(), None);

        f.log.clear();
        connect(wifi, "Cafe".into(), None).await.unwrap();
        keep();
        tokio::time::sleep(PROBATION + Duration::from_secs(1)).await;
        assert_eq!(f.log.calls(), ["wifi.join Cafe"], "kept: no going back");
    }
}
