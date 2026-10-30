// wado bridge — why the trackpad was refused, as a card that says what to do (`.tperr`).
//
// The daemon closes the pad with one plain sentence (crates/server/src/pad/gate.rs, the platform's
// NO_UINPUT); this reads which refusal it is and lays out the fix: the passkey flow right there,
// the exact command to allow it with a Copy button, how to turn the trackpad on, how to give the
// computer /dev/uinput — or, on a computer wado cannot drive yet, says so. Never a blank pad.
//
//   W.padWhy(message)            → { cause, title, body, cmds: [..], alt, act } (pure; tested)
//   W.padWhyShow(el, message, retry)   fill `el` with that card; `retry` reopens the pad

(() => {
  // Each cause: how the daemon's words start or what they contain, and what to tell the person.
  W.padWhy = (message) => {
    const m = String(message || "");
    const grab = (re) => (m.match(re) || [])[1] || "";
    if (W.passkeyIsLock && W.passkeyIsLock(m)) {
      const none = /^This needs a passkey/.test(m);
      return {
        cause: none ? "no-passkey" : "locked",
        title: none ? "The trackpad needs a passkey" : "Locked",
        body: none
          ? "Your fingerprint, face or PIN becomes the key for the trackpad and keyboard on this computer. Nothing about it leaves this phone."
          : "Unlock with this phone's passkey to use the trackpad.",
        cmds: [],
        act: none ? "Add a passkey" : "Unlock",
      };
    }
    if (/may not use the trackpad/.test(m)) {
      const cmd = grab(/on the computer: (wado allow .+ input)\s*$/) || "wado allow <this device> input";
      return {
        cause: "no-grant",
        title: "This phone may not use the trackpad yet",
        body: "The trackpad and typing are off for each phone until the computer allows them. On the computer, run:",
        cmds: [cmd],
        alt: "Or open wado tui → Devices and tick input for this phone.",
      };
    }
    if (/trackpad is turned off on this computer/.test(m)) {
      return {
        cause: "off",
        title: "The trackpad is turned off on this computer",
        body: "Its config has trackpad { enabled #false }. To turn it on, run on the computer:",
        cmds: ["wado msg set trackpad.enabled true"],
        alt: "Or remove enabled #false from the trackpad section of ~/.config/wado/config.kdl.",
      };
    }
    if (/\/dev\/uinput/.test(m)) {
      return {
        cause: "uinput",
        title: "The computer can't make a trackpad yet",
        body: "wado moves the pointer and types through /dev/uinput, and this computer doesn't let it open that. On the computer, run these once:",
        cmds: [
          "sudo modprobe uinput",
          "echo 'KERNEL==\"uinput\", GROUP=\"uinput\", MODE=\"0660\", OPTIONS+=\"static_node=uinput\"' | sudo tee /etc/udev/rules.d/60-wado-uinput.rules",
          "sudo groupadd -f uinput && sudo usermod -aG uinput $USER",
          "sudo udevadm control --reload && sudo udevadm trigger",
        ],
        alt: "Then log out and back in, so the new group counts, and restart wado. On NixOS instead: hardware.uinput.enable = true; and add \"uinput\" to your user's extraGroups.",
      };
    }
    if (/isn't supported on this computer|nothing on this computer provides it/.test(m)) {
      return {
        cause: "unsupported",
        title: "Not on this computer yet",
        body: "wado can only drive the pointer and keyboard on Linux for now. On macOS and Windows the trackpad isn't supported yet — the screen and files still work.",
        cmds: [],
      };
    }
    return { cause: "other", title: "The trackpad stopped", body: m, cmds: [] };
  };

  const copyText = async (text) => {
    try { await navigator.clipboard.writeText(text); return true; } catch (_) {}
    try {
      const t = document.createElement("textarea");
      t.value = text;
      t.setAttribute("readonly", "");
      t.style.position = "fixed";
      t.style.opacity = "0";
      document.body.appendChild(t);
      t.select();
      const ok = document.execCommand("copy");
      t.remove();
      return ok;
    } catch (_) { return false; }
  };

  const node = (tag, cls, text) => {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  };

  W.padWhyShow = (el, message, retry) => {
    const w = W.padWhy(message);
    // The same refusal again (the relayed ping and the direct offer both hear it): leave the card
    // be. Drawn anew, the button under a finger is swapped mid-tap and the tap is lost.
    if (!el.hidden && el.dataset.msg === String(message || "")) return w;
    el.replaceChildren();
    el.dataset.msg = String(message || "");
    el.dataset.cause = w.cause;
    el.appendChild(node("b", "tpwhyt", w.title));
    if (w.body) el.appendChild(node("p", "tpwhyb", w.body));
    for (const cmd of w.cmds) {
      const row = node("div", "tpcmd");
      row.appendChild(node("code", "mono", cmd));
      const b = node("button", "btn tpcopy", "Copy");
      b.type = "button";
      b.setAttribute("aria-label", "Copy " + cmd);
      b.onclick = async () => {
        const ok = await copyText(cmd);
        b.textContent = ok ? "Copied" : "Select it";
        b.classList.toggle("done", ok);
        setTimeout(() => { b.textContent = "Copy"; b.classList.remove("done"); }, 1800);
      };
      row.appendChild(b);
      el.appendChild(row);
    }
    if (w.alt) el.appendChild(node("p", "tpwhya", w.alt));
    if (w.act && W.passkeyFor) {
      const b = node("button", "btn primary tpact", w.act);
      b.type = "button";
      b.onclick = () => W.passkeyFor("pad", retry);
      el.appendChild(b);
    } else if (w.cause !== "unsupported") {
      const b = node("button", "btn tpact", "Try again");
      b.type = "button";
      b.onclick = () => retry && retry();
      el.appendChild(b);
    }
    el.hidden = false;
    return w;
  };
})();
