//! The devices messages over the sealed relay link, for a device holding `devices` (`delegate`).
//! Every request is checked against the gate at the moment it arrives, so a grant taken away on
//! the computer stops the next one.

use wado_protocol::relay::{DeviceRow, PairAsk, RelayMsg};

use super::delegate::NOT_MANAGER;
use super::{Gate, Grants};

fn list(gate: &Gate, me: &str, error: Option<String>) -> RelayMsg {
    let devices = gate
        .devices()
        .into_iter()
        .map(|d| DeviceRow {
            mine: !me.is_empty() && d.term.by == me,
            me: d.key == me,
            grants: gate.grants(&d.key).tokens(),
            until: d.term.until,
            name: d.name,
            key: d.key,
        })
        .collect();
    let asks = gate
        .asking_for(me)
        .into_iter()
        .map(|a| PairAsk {
            left: a.left(),
            typed: a.typed,
            words: a.words,
            device: a.device,
        })
        .collect();
    RelayMsg::DeviceList {
        devices,
        asks,
        error,
    }
}

/// The answer to one devices message from the device `key` (`name`), or `None` if `msg` is not one.
pub fn handle(msg: &RelayMsg, gate: &Gate, key: &str, name: &str) -> Option<RelayMsg> {
    let manager = gate.grants(key).devices;
    let refused = || list(gate, key, Some(NOT_MANAGER.into()));
    Some(match msg {
        RelayMsg::DevicesRequest if manager => list(gate, key, None),
        RelayMsg::DeviceSignOut { key: who } if manager => {
            let err = gate.sign_out_as(key, who).err().map(String::from);
            list(gate, key, err)
        }
        RelayMsg::PairAnswer { typed, yes } if manager => {
            let err = (!gate.answer_as(key, typed, *yes))
                .then(|| "that question has gone — it was answered, or ran out".to_string());
            list(gate, key, err)
        }
        RelayMsg::PairMint {
            grants,
            trust_for_s,
        } => {
            let fail = |e: String| RelayMsg::PairMinted {
                link: String::new(),
                typed: String::new(),
                until: 0,
                error: Some(e),
            };
            if !manager {
                return Some(fail(NOT_MANAGER.into()));
            }
            // Letting a device in is terminal-equivalent power: a fresh passkey unlock, always.
            if let Some(why) = crate::passkey::refused(key) {
                return Some(fail(why.into()));
            }
            let want = match Grants::parse(&grants.join(",")) {
                Ok(g) => g,
                Err(e) => return Some(fail(e)),
            };
            let made = crate::cli::qr::link_for(None, None, || {
                gate.mint_for(key, &want, *trust_for_s)
                    .map_err(String::from)
            });
            match made {
                Ok(p) => {
                    crate::passkey::approve_notify::notice(
                        "A phone made a pairing code",
                        format!(
                            "{name} made a code that lets one more device in, for up to {}. \
                             See it in wado tui → Devices; wado unpair removes it.",
                            crate::cli::devices::span(
                                (*trust_for_s).clamp(60, super::grants::DELEGATE_MAX_S)
                            )
                        ),
                    );
                    RelayMsg::PairMinted {
                        link: p.link,
                        typed: super::typed_code::display(&p.code.typed),
                        until: p.code.until,
                        error: None,
                    }
                }
                Err(e) => fail(e),
            }
        }
        RelayMsg::DevicesRequest | RelayMsg::DeviceSignOut { .. } | RelayMsg::PairAnswer { .. } => {
            refused()
        }
        _ => return None,
    })
}
