// wado bridge — a Game mode's shadow (crate ux `Profile::overlay`, `game_mode.rs`): while a
// session runs for a game, how it opens, its input face, the pad over the picture and the pad's
// layout come from the game. Nothing is written to this device's prefs on the computer; a change
// made in the game comes back to Rust as `gameKept` and is kept for that game.
//
//   W.gameShadow({open, face, pad, layout})   take the game's
//   W.gameShadow(null)                        give the device's own back (`pad`: its own switch)

W.gameShadow = (o) => {
  if (W.consolePrefs) W.consolePrefs.shadow(o ? { open: o.open, face: o.face } : null);
  if (W.gpadPrefs) W.gpadPrefs.shadow(o ? o.layout : null);
  if (W.setGamepad && o && "pad" in o) W.setGamepad({ on: !!o.pad });
};
