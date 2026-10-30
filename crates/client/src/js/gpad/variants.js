// wado bridge — the gamepad's built-in layouts (docs/superpowers/specs/2026-10-30-virtual-input-design.md
// §6, §9.1). Data only: what each layout holds and where. Shipped in the client, never stored —
// a saved layout is a diff on top of one of these (`gpad/model.js`).
//
// A control: `id` (unique in the layout), `kind`, `bind` (what it sends as a controller), `label`,
// `g` (its move-together group), and `l` / `p` = `[x, y, size]` for landscape / portrait: its
// centre as a fraction of the reference play area, and a multiplier on the kind's base size in
// CSS px. Positions are relative, sizes physical (D8): a button is the same size under the thumb
// on a phone and a tablet.
//
// Face buttons bind by position (D9): south = a, east = b, west = x, north = y, whatever the label
// says — the host device is an Xbox 360 pad, and SDL and Steam map by position.
//
// Names carry no brand words (resolved Q1): the glyphs say which family each one is.
//
//   W.gpadVariants.BASE, MIN_PX, REF, NAMES, BUILT, ORDER, LOOK(o)

(() => {
  const V = (W.gpadVariants = {});
  // Base sizes in CSS px at 1×. Every default is ≥ 56 px on its short side except the 44 px centre
  // row, and nothing can be made smaller than 44 px (`MIN_PX`).
  V.BASE = {
    btn: [64, 64], small: [64, 44], bumper: [84, 46], trig: [84, 50],
    dpad: [150, 150], stick: [128, 128], pedal: [96, 170], slider: [230, 76],
  };
  V.KINDS = Object.keys(V.BASE);
  V.MIN_PX = 44;
  V.MAX_SCALE = 2.2;
  // The reference play area: the safe area of an 844×390 / 390×844 phone, minus 10 px a side.
  V.REF = { l: [824, 370], p: [370, 824] };
  // Every bind the model knows: buttons, triggers (analog as a pedal), the hat, the two sticks.
  V.BINDS = ["a", "b", "x", "y", "lb", "rb", "lt", "rt", "back", "start", "guide", "l3", "r3", "hat", "ls", "rs"];
  // A control's name, said by screen readers and the editor.
  V.NAMES = {
    a: "A", b: "B", x: "X", y: "Y", lb: "LB", rb: "RB", lt: "LT", rt: "RT",
    back: "Back", start: "Start", guide: "Guide", dpad: "D-pad",
    "stick-l": "Left stick", "stick-r": "Right stick", steer: "Steering",
  };

  V.LOOK = (o) => ({ shape: "circle", style: "solid", labels: "letters", press: "glow", op: 0.92, ...o });
  const C = (id, kind, bind, label, g, l, p, extra) => ({ id, kind, bind, label, g, l, p, ...(extra || {}) });

  const shoulders = (lab) => [
    C("lt", "trig", "lt", lab[0], "sh-l", [0.07, 0.11, 1], [0.13, 0.39, 1]),
    C("lb", "bumper", "lb", lab[1], "sh-l", [0.19, 0.11, 1], [0.38, 0.39, 1]),
    C("rb", "bumper", "rb", lab[2], "sh-r", [0.81, 0.11, 1], [0.62, 0.39, 1]),
    C("rt", "trig", "rt", lab[3], "sh-r", [0.93, 0.11, 1], [0.87, 0.39, 1]),
  ];
  const mids = (words) => [
    C("back", "small", "back", words ? "Select" : "", "mid", [0.41, 0.36, 1], [0.3, 0.5, 1], { icon: "view", word: !!words }),
    C("guide", "small", "guide", "", "mid", [0.5, 0.36, 1], [0.5, 0.5, 1], { icon: "home" }),
    C("start", "small", "start", words ? "Start" : "", "mid", [0.59, 0.36, 1], [0.7, 0.5, 1], { icon: "menu", word: !!words }),
  ];
  // The diamond: north / west / east / south around a centre, landscape and portrait.
  const face = (lab, cl, cp, s = 1) => [
    C("y", "btn", "y", lab.n, "face", [cl[0], cl[1] - 0.19, s], [cp[0], cp[1] - 0.075, s]),
    C("x", "btn", "x", lab.w, "face", [cl[0] - 0.075, cl[1], s], [cp[0] - 0.175, cp[1], s]),
    C("b", "btn", "b", lab.e, "face", [cl[0] + 0.075, cl[1], s], [cp[0] + 0.175, cp[1], s]),
    C("a", "btn", "a", lab.s, "face", [cl[0], cl[1] + 0.19, s], [cp[0], cp[1] + 0.075, s]),
  ];

  V.BUILT = {
    xbox: {
      name: "Standard", sub: "A B X Y · two sticks", look: V.LOOK({ labels: "colors" }), controls: [
        ...shoulders(["LT", "LB", "RB", "RT"]), ...mids(false),
        C("stick-l", "stick", "ls", "L", "stick-l", [0.12, 0.5, 1], [0.25, 0.87, 1]),
        C("dpad", "dpad", "hat", "", "dpad", [0.3, 0.76, 1], [0.23, 0.64, 1]),
        C("stick-r", "stick", "rs", "R", "stick-r", [0.68, 0.76, 1], [0.75, 0.87, 1]),
        ...face({ s: "A", e: "B", w: "X", n: "Y" }, [0.86, 0.59], [0.73, 0.64]),
      ],
    },
    ps: {
      name: "Shapes", sub: "△ ○ ✕ □ · sticks low", look: V.LOOK({ labels: "shapes" }), controls: [
        ...shoulders(["L2", "L1", "R1", "R2"]), ...mids(false),
        C("dpad", "dpad", "hat", "", "dpad", [0.13, 0.5, 1], [0.23, 0.64, 1]),
        C("stick-l", "stick", "ls", "L", "stick-l", [0.33, 0.78, 1], [0.28, 0.87, 1]),
        C("stick-r", "stick", "rs", "R", "stick-r", [0.67, 0.78, 1], [0.72, 0.87, 1]),
        ...face({ s: "", e: "", w: "", n: "" }, [0.87, 0.5], [0.73, 0.64]),
      ],
    },
    nintendo: {
      name: "Swapped", sub: "B A Y X · B below", look: V.LOOK({ labels: "letters" }), controls: [
        ...shoulders(["ZL", "L", "R", "ZR"]),
        C("back", "small", "back", "−", "mid", [0.41, 0.36, 1], [0.3, 0.5, 1], { word: true }),
        C("guide", "small", "guide", "", "mid", [0.5, 0.36, 1], [0.5, 0.5, 1], { icon: "home" }),
        C("start", "small", "start", "+", "mid", [0.59, 0.36, 1], [0.7, 0.5, 1], { word: true }),
        C("stick-l", "stick", "ls", "L", "stick-l", [0.12, 0.5, 1], [0.25, 0.64, 1]),
        C("dpad", "dpad", "hat", "", "dpad", [0.3, 0.76, 1], [0.25, 0.87, 1]),
        C("stick-r", "stick", "rs", "R", "stick-r", [0.68, 0.76, 1], [0.73, 0.87, 1]),
        // Labels swap, codes stay by position: B is drawn south and still sends the south button.
        ...face({ s: "B", e: "A", w: "Y", n: "X" }, [0.86, 0.5], [0.73, 0.64]),
      ],
    },
    minimal: {
      name: "Minimal", sub: "D-pad and two", look: V.LOOK(), controls: [
        C("dpad", "dpad", "hat", "", "dpad", [0.17, 0.64, 1.15], [0.27, 0.72, 1.15]),
        C("b", "btn", "b", "B", "face", [0.73, 0.72, 1.2], [0.62, 0.78, 1.2]),
        C("a", "btn", "a", "A", "face", [0.875, 0.55, 1.35], [0.82, 0.68, 1.35]),
        C("start", "small", "start", "", "mid", [0.5, 0.84, 1], [0.5, 0.92, 1], { icon: "menu" }),
      ],
    },
    racing: {
      name: "Racing", sub: "Steer, gas, brake", look: V.LOOK({ shape: "round" }), controls: [
        C("lb", "bumper", "lb", "LB", "sh-l", [0.1, 0.12, 1], [0.15, 0.4, 1]),
        C("rb", "bumper", "rb", "RB", "sh-r", [0.9, 0.12, 1], [0.85, 0.4, 1]),
        C("start", "small", "start", "", "mid", [0.5, 0.3, 1], [0.5, 0.4, 1], { icon: "menu" }),
        C("steer", "slider", "ls", "", "steer", [0.22, 0.74, 1], [0.27, 0.9, 0.8], { steer: "slide" }),
        C("x", "btn", "x", "X", "face", [0.585, 0.55, 0.95], [0.64, 0.56, 1]),
        C("a", "btn", "a", "A", "face", [0.585, 0.8, 0.95], [0.84, 0.56, 1]),
        C("lt", "pedal", "lt", "Brake", "pedals", [0.755, 0.66, 0.9], [0.59, 0.76, 0.85]),
        C("rt", "pedal", "rt", "Gas", "pedals", [0.905, 0.6, 1], [0.86, 0.76, 1]),
      ],
    },
    fighting: {
      name: "Fighting", sub: "Six buttons, 8-way", look: V.LOOK({ labels: "letters" }), controls: [
        C("back", "small", "back", "", "mid", [0.44, 0.3, 1], [0.38, 0.45, 1], { icon: "view" }),
        C("start", "small", "start", "", "mid", [0.56, 0.3, 1], [0.62, 0.45, 1], { icon: "menu" }),
        C("dpad", "dpad", "hat", "", "dpad", [0.17, 0.62, 1.1], [0.25, 0.74, 1.05], { dir: 8 }),
        C("x", "btn", "x", "X", "face", [0.67, 0.5, 0.95], [0.58, 0.66, 0.9]),
        C("y", "btn", "y", "Y", "face", [0.79, 0.44, 0.95], [0.75, 0.63, 0.9]),
        C("rb", "btn", "rb", "RB", "face", [0.91, 0.44, 0.95], [0.91, 0.63, 0.9]),
        C("a", "btn", "a", "A", "face", [0.65, 0.75, 0.95], [0.56, 0.8, 0.9]),
        C("b", "btn", "b", "B", "face", [0.77, 0.69, 0.95], [0.73, 0.77, 0.9]),
        C("rt", "btn", "rt", "RT", "face", [0.89, 0.69, 0.95], [0.9, 0.77, 0.9]),
      ],
    },
    retro: {
      name: "Retro", sub: "D-pad, A B, Start", look: V.LOOK(), controls: [
        C("dpad", "dpad", "hat", "", "dpad", [0.16, 0.58, 1.05], [0.25, 0.66, 1.05]),
        C("b", "btn", "b", "B", "face", [0.74, 0.66, 1.15], [0.64, 0.7, 1.15]),
        C("a", "btn", "a", "A", "face", [0.88, 0.52, 1.15], [0.84, 0.63, 1.15]),
        C("back", "small", "back", "Select", "mid", [0.42, 0.84, 1.1], [0.37, 0.87, 1.1], { word: true }),
        C("start", "small", "start", "Start", "mid", [0.58, 0.84, 1.1], [0.63, 0.87, 1.1], { word: true }),
      ],
    },
  };
  V.ORDER = ["xbox", "ps", "nintendo", "minimal", "racing", "fighting", "retro"];
  // The in-session default (spec §6), and the pointer "Away from a session, use the same".
  V.DEFAULT = "xbox";
  V.SAME = "same";
})();
