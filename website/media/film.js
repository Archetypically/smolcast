// The demo film: a simulation of smolcast on a Windows 11 desktop, drawn
// to the app's own measurements and copy. Labels, commands, the calculator's
// answer and the emoji search results are the app's (src/app, src/core);
// timings are core::motion's. What it cannot reproduce: each app's own icon
// (plain drawings stand in) and Windows' fonts (Selawik and Fluent assets,
// see film/README.md). scripts/film.mjs records it.
//
// Around the simulation, and never inside it: a title card the film starts
// and ends on (so it loops), a caption per chapter, the viewer's key caps,
// and a camera that leans in while the palette is open.
"use strict";

const params = new URLSearchParams(location.search);
const zoom = params.get("zoom");
if (zoom) document.documentElement.style.setProperty("--zoom", zoom);
const format = params.get("format") ?? "wide";
document.documentElement.dataset.format = format;
// Timers run this many times slower while recording (slowmo.js).
const slow = Number(params.get("slow")) || 1;
// One phrase of the music, in seconds (PHRASE in scripts/music.mjs): the
// film's length is a whole number of them, and the backdrop drifts one
// repeat of its tile in each.
const PHRASE = Number(params.get("phrase")) || 12;

const $ = (id) => document.getElementById(id);
const wait = (ms) => new Promise((done) => setTimeout(done, ms));

// ---------------------------------------------------------------- data

// Root entries in usage order, as on a desktop used for a while. Commands
// and system actions carry Windows icon-font glyphs (launcher.rs).
const ENTRIES = [
  { name: "Terminal", kind: "Application", app: "terminal", text: ">_" },
  { name: "Clipboard History", kind: "Command", glyph: "clipboard_paste", view: "clipboard" },
  { name: "Search Emoji", kind: "Command", glyph: "emoji", view: "emoji" },
  { name: "Paint", kind: "Application", app: "paint", text: "" },
  { name: "Search Files", kind: "Command", glyph: "search" },
  { name: "Calculator", kind: "Application", app: "calculator", text: "=" },
  { name: "Lock Screen", kind: "System", glyph: "lock_closed" },
  { name: "Search Snippets", kind: "Command", glyph: "note" },
  { name: "smolcast settings", kind: "Command", glyph: "settings" },
  { name: "Toggle Dark Mode", kind: "System", glyph: "weather_sunny" },
  { name: "Sleep", kind: "System", glyph: "weather_moon" },
  { name: "About smolcast", kind: "Command", glyph: "info" },
];

// core::calc::evaluate("40cm to in"); every shorter prefix is not a
// calculation.
const ANSWERS = {
  "40cm to in": { input: "40 cm", result: "15.7480315 in", inputLabel: "Centimeters", resultLabel: "Inches" },
};

// core::emoji::search for each prefix of "party" (film/emoji-search.json):
// the first 72 results, as the 6 × 12 grid shows them, and the totals.
let SEARCH = null;
let AVAILABLE = new Set();

// Clipboard history, newest first.
const CLIPS = [
  { text: "https://github.com/Archetypically/smolcast", age: "4m" },
  { text: "Lunch at 12:30 works for me!", age: "18m" },
  { image: "Image 1280 × 720", age: "1h" },
  { text: "cargo build --release --target x86_64-pc-windows-msvc", age: "2h" },
  { text: "Meeting notes: ship the website, then the release notes.", age: "5h" },
];

// ---------------------------------------------------------------- drawing

function el(tag, cls, ...children) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  for (const child of children) if (child != null) node.append(child);
  return node;
}

const SVG = {
  search:
    '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="6.75" cy="6.75" r="4.75"/><path d="m10.4 10.4 3.85 3.85"/></svg>',
  back: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M13 8H3.5M7.5 3.75 3.25 8l4.25 4.25"/></svg>',
  arrow:
    '<svg viewBox="0 0 24 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M1 6h20M16 1.5 21 6l-5 4.5"/></svg>',
};

function svg(name) {
  const span = el("span");
  span.innerHTML = SVG[name];
  return span.firstChild;
}

function glyph(name) {
  const g = el("span", "glyph");
  g.style.setProperty("--icon", `url("film/icons/${name}.svg")`);
  return g;
}

// Fluent artwork for an emoji. Segoe UI Emoji has no flags, so Windows
// draws their regional-indicator letters; the newest emoji have no art yet.
function emojiImg(code, cls) {
  if (!AVAILABLE.has(code)) {
    const letters = code
      .split("-")
      .map((cp) => parseInt(cp, 16))
      .filter((cp) => cp >= 0x1f1e6 && cp <= 0x1f1ff)
      .map((cp) => String.fromCharCode(65 + cp - 0x1f1e6));
    return el("span", `missing ${cls ?? ""}`, letters.join(""));
  }
  const img = el("img", cls);
  img.src = `film/emoji/${code}.png`;
  img.alt = "";
  return img;
}

function icon(entry) {
  if (entry.app) return el("span", `app ${entry.app}`, entry.text);
  if (entry.emoji) return emojiImg(entry.emoji);
  return glyph(entry.glyph);
}

function row(entry) {
  return el(
    "div",
    "row",
    el("span", "icon", icon(entry)),
    el("span", "name", entry.name),
    el("span", "trail", entry.kind),
  );
}

function caps(...keys) {
  return keys.map((k) => el("span", "cap", k));
}

// ---------------------------------------------------------------- the palette

const ui = {
  view: "root",
  query: "",
  sel: 0,
  rows: [], // selectable elements in the current view
};

// Skim-like fuzzy filter: every query character in order, word starts and
// runs ranked first; usage order breaks ties.
function rank(query) {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return ENTRIES;
  return ENTRIES.map((entry, order) => {
    const name = entry.name.toLowerCase();
    let at = 0;
    let score = 0;
    let run = 0;
    for (const ch of q) {
      const i = name.indexOf(ch, at);
      if (i === -1) return null;
      run = i === at ? run + 1 : 0;
      score += 1 + run * 2 + (i === 0 || name[i - 1] === " " ? 4 : 0);
      at = i + 1;
    }
    return { entry, score, order };
  })
    .filter(Boolean)
    .sort((a, b) => b.score - a.score || a.order - b.order)
    .map((r) => r.entry);
}

function render() {
  const content = $("content");
  content.replaceChildren();
  const emoji = ui.view === "emoji";
  const clip = ui.view === "clipboard";
  const typed = ui.query.trim();

  // Search line: the magnifier, or the back button in a sub-view.
  const lead = el("span", "slot");
  if (ui.view === "root") lead.append(svg("search"));
  else lead.append(el("span", "back", svg("back")));
  const placeholder = {
    root: "Search apps and commands…",
    emoji: "Search emoji…",
    clipboard: "Search clipboard history…",
  }[ui.view];
  const query = ui.query
    ? el("span", "query", ui.query, el("span", "caret"))
    : el("span", "query", el("span", "caret"), el("span", "placeholder", placeholder));
  content.append(el("div", "search", lead, query), el("div", "rule"));

  const body = el("div", "body");
  const sel = el("div", "sel");
  ui.rows = [];
  let crumb = null;

  if (ui.view === "root") {
    const answer = ANSWERS[typed];
    const ranked = answer ? [] : rank(typed);
    const fallbacks =
      typed && !answer
        ? [
            {
              name: `Search Google for “${typed}”`,
              kind: "Web Search",
              glyph: "globe",
              fallback: "Search Web",
            },
            {
              name: `Search Files for “${typed}”`,
              kind: "Files",
              glyph: "search",
              fallback: "Search Files",
            },
          ]
        : [];
    body.append(el("div", "header", answer ? "Calculator" : typed ? "Results" : "Suggestions"));
    if (answer) {
      const card = el(
        "div",
        "card",
        el("div", "side", el("span", "value", answer.input), el("span", "pill", answer.inputLabel)),
        el("div", "side", el("span", "value", answer.result), el("span", "pill", answer.resultLabel)),
      );
      const arrow = svg("arrow");
      arrow.setAttribute("class", "arrow");
      card.append(arrow);
      body.append(card);
      ui.rows.push({ el: card, action: "Paste Answer" });
    }
    for (const entry of ranked.slice(0, 7)) {
      const r = row(entry);
      body.append(r);
      const label = { Application: "Open Application", Command: "Open Command", System: "Run Command" }[
        entry.kind
      ];
      ui.rows.push({ el: r, action: label, entry });
    }
    if (ranked.length && fallbacks.length) {
      body.append(el("div", "header", `Search for “${typed}”`));
    }
    for (const entry of fallbacks.slice(0, Math.max(0, 7 - ranked.length))) {
      const r = row(entry);
      body.append(r);
      ui.rows.push({ el: r, action: entry.fallback, entry });
    }
    // The last row in view dims while more wait below.
    if (ranked.length && ranked.length + fallbacks.length > 7) {
      ui.rows[ui.rows.length - 1].el.classList.add("edge");
    }
  } else if (emoji) {
    crumb = "Search Emoji";
    const found = SEARCH[typed] ?? { total: 0, items: [] };
    const grid = el("div", "grid");
    for (const [code, name, group] of found.items) {
      const cell = el("div", "cell", emojiImg(code));
      grid.append(cell);
      ui.rows.push({ el: cell, action: "Paste", code, name, group });
    }
    body.append(grid);
    const current = ui.rows[ui.sel];
    if (current) {
      body.append(
        el(
          "div",
          "details",
          emojiImg(current.code),
          el(
            "div",
            null,
            el("b", null, current.name),
            el("span", null, `${current.group} · ${ui.sel + 1} of ${found.total}`),
          ),
        ),
      );
    }
  } else if (clip) {
    crumb = "Clipboard History";
    body.append(el("div", "source", "smolcast history · 1.4 KiB"));
    const list = el("div", "list");
    for (const item of CLIPS) {
      const r = row({
        name: item.image ?? item.text,
        kind: item.age,
        emoji: item.image ? "1f5bc" : "1f4c4",
      });
      list.append(r);
      ui.rows.push({ el: r, action: "Paste", item });
    }
    const current = CLIPS[ui.sel];
    const preview = current.image
      ? el("div", "preview", el("small", null, "1280 × 720"))
      : el("div", "preview", current.text, el("small", null, `${[...current.text].length} characters`));
    body.append(el("div", "split", list, el("div", "vsep"), preview));
    body.append(el("div", "buttons", el("span", null, "Delete"), el("span", null, "Clear all")));
  }

  if (!ui.rows.length) sel.classList.add("off");
  (clip ? body.querySelector(".list") : body).prepend(sel);
  ui.selEl = sel;
  content.append(body, el("div", "rule"));

  // Footer: where you are and the summon key on the left; the action on the
  // right, then Back in a sub-view.
  const footer = el("div", "footer", el("span", null, crumb ?? "smolcast"));
  if (!crumb) footer.append(...caps("Ctrl", "Alt", "Space"));
  const right = el("div", "right");
  const action = ui.rows[ui.sel]?.action;
  if (action) right.append(el("span", "label", action), ...caps("Enter"));
  if (crumb) right.append(el("span", "bar", "|"), el("span", "label", "Back"), ...caps("Esc"));
  footer.append(right);
  content.append(footer);
  place(false);
}

// Move the selection fill to the selected row; it glides unless `animate`
// is false (a new list starts in place).
function place(animate = true) {
  const target = ui.rows[ui.sel]?.el;
  const sel = ui.selEl;
  if (!target || !sel) return;
  if (!animate) sel.style.transition = "none";
  if (ui.view === "emoji") {
    sel.classList.add("cell");
    sel.style.width = "44px";
    sel.style.height = "44px";
    sel.style.transform = `translate(${target.offsetLeft - 8}px, ${target.offsetTop}px)`;
  } else {
    sel.style.height = `${target.offsetHeight}px`;
    sel.style.transform = `translateY(${target.offsetTop}px)`;
  }
  for (const [i, r] of ui.rows.entries()) {
    r.el.classList.toggle("selected", i === ui.sel);
    if (i === ui.sel) r.el.classList.remove("edge");
  }
  if (!animate) {
    void sel.offsetWidth;
    sel.style.transition = "";
  }
}

function select(i) {
  ui.sel = Math.max(0, Math.min(ui.rows.length - 1, i));
  if (ui.view === "root") {
    place();
    $("content").querySelector(".footer .label").textContent = ui.rows[ui.sel].action;
  } else {
    // The details strip and preview follow the selection.
    const keep = ui.sel;
    const from = ui.selEl.style.transform;
    render();
    ui.sel = keep;
    ui.selEl.style.transition = "none";
    ui.selEl.style.transform = from;
    void ui.selEl.offsetWidth;
    ui.selEl.style.transition = "";
    place();
  }
}

// ---------------------------------------------------------------- motion

const easeOut = "cubic-bezier(0.33, 1, 0.68, 1)";
const easeIn = "cubic-bezier(0.32, 0, 0.67, 0)";

async function summon() {
  ui.view = "root";
  ui.query = "";
  ui.sel = 0;
  render();
  const palette = $("palette");
  palette.classList.add("shown");
  $("notes").classList.add("inactive");
  palette.style.opacity = "1";
  // APPEAR: content fades in and scales 97% → 100%; the frame and backdrop
  // appear at once.
  $("content").animate(
    [
      { opacity: 0, transform: "scale(0.97)" },
      { opacity: 1, transform: "none" },
    ],
    { duration: 160, easing: easeOut },
  );
  await wait(160);
}

// A completed action hides the window at once.
function hideNow() {
  const palette = $("palette");
  palette.classList.remove("shown");
  palette.style.opacity = "0";
  $("notes").classList.remove("inactive");
}

// DISMISS: content fades out and scales to 97%, then the window hides.
async function dismiss() {
  await $("content").animate(
    [
      { opacity: 1, transform: "none" },
      { opacity: 0, transform: "scale(0.97)" },
    ],
    { duration: 90, easing: easeIn, fill: "forwards" },
  ).finished;
  hideNow();
}

// NAVIGATE: the body slides 12 pt in and fades in; the search line fades.
function navigate(view, deeper) {
  ui.view = view;
  ui.query = "";
  ui.sel = 0;
  render();
  const content = $("content");
  content.querySelector(".body").animate(
    [
      { opacity: 0, transform: `translateX(${deeper ? 12 : -12}px)` },
      { opacity: 1, transform: "none" },
    ],
    { duration: 150, easing: easeOut },
  );
  content
    .querySelector(".search")
    .animate([{ opacity: 0.2 }, { opacity: 1 }], { duration: 150, easing: easeOut });
}

// ---------------------------------------------------------------- the viewer's key caps

let keysTimer;
function showKeys(...keys) {
  const box = $("keys");
  box.replaceChildren(...keys.map((k) => el("span", "cap", k)));
  box.classList.add("show");
  clearTimeout(keysTimer);
  keysTimer = setTimeout(() => box.classList.remove("show"), 750);
  // The palette's own caps light while their keys are held.
  for (const cap of $("content").querySelectorAll(".footer .cap")) {
    if (keys.includes(cap.textContent)) {
      cap.classList.add("held");
      setTimeout(() => cap.classList.remove("held"), 140);
    }
  }
}

// ---------------------------------------------------------------- the notes window

const notes = { parts: [""] };

function drawNotes() {
  const body = $("notes-body");
  body.replaceChildren();
  for (const part of notes.parts) {
    if (typeof part === "string") body.append(part);
    else body.append(emojiImg(part.emoji, "emoji"));
  }
  body.append(el("span", "caret"));
}

function writeNotes(text) {
  const last = notes.parts[notes.parts.length - 1];
  if (typeof last === "string") notes.parts[notes.parts.length - 1] = last + text;
  else notes.parts.push(text);
  drawNotes();
}

async function typeNotes(text) {
  for (const ch of text) {
    writeNotes(ch);
    await wait(ch === " " ? 70 : 85);
  }
}

async function typeQuery(text) {
  for (const ch of text) {
    ui.query += ch;
    render();
    await wait(95);
  }
}

// ---------------------------------------------------------------- the script

// The summon key shows in the key caps, then the palette opens.
async function summonKey(settle) {
  showKeys("Ctrl", "Alt", "Space");
  await summon();
  lean(true);
  await wait(settle);
}

// Enter shows in the key caps, then the action completes and the window hides.
async function pressEnter() {
  showKeys("Enter");
  await wait(120);
  hideNow();
  lean(false);
}

// ---------------------------------------------------------------- framing

// The camera: a slow lean toward the palette while it is open, and back to
// the whole desktop when it hides. `scale` about the point (x, y) of the
// desktop, which stays put.
function camera(scale, x, y, instant = false) {
  const cam = $("camera");
  if (instant) cam.style.transition = "none";
  cam.style.transform = `translate(${x * (1 - scale)}px, ${y * (1 - scale)}px) scale(${scale})`;
  if (instant) {
    void cam.offsetWidth;
    cam.style.transition = "";
  }
}

const lean = (open) => camera(open ? 1.05 : 1, 640, 320);

// A chapter caption: the numeral fades in, then the words rise one by one.
// Captions run one at a time, in order, however they are called.
let captions = Promise.resolve();
function caption(step, text) {
  return (captions = captions.then(() => showCaption(step, text)));
}

async function showCaption(step, text) {
  await captionOut();
  const box = $("caption");
  const num = el("span", "step", step);
  const words = text.split(" ").map((w) => el("span", "word", el("span", null, w)));
  box.replaceChildren(num, ...words.flatMap((w, i) => (i ? [" ", w] : [w])));
  box.style.opacity = "1";
  num.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 500, easing: "ease-out" });
  for (const [i, w] of words.entries()) {
    w.firstChild.animate([{ transform: "translateY(105%)" }, { transform: "none" }], {
      duration: 700,
      delay: 120 + i * 55,
      easing: "cubic-bezier(0.22, 1, 0.36, 1)",
      fill: "backwards",
    });
  }
}

async function captionOut() {
  const box = $("caption");
  if (!box.childElementCount) return;
  // `translate` lifts it on top of each layout's own transform.
  await box.animate(
    [
      { opacity: 1, translate: "0 0" },
      { opacity: 0, translate: "0 -6px" },
    ],
    { duration: 320, easing: "ease-in", fill: "forwards" },
  ).finished;
  box.getAnimations().forEach((a) => a.cancel());
  box.style.opacity = "0";
  box.replaceChildren();
}

const CARD = () => [...$("end").children];

// The backdrop drifts up and to the right, one diagonal repeat of its tile
// per phrase, from the film's first frame; returns the moment it started.
function drift() {
  $("backdrop").animate([{ maskPosition: "0 0" }, { maskPosition: "96px -96px" }], {
    duration: PHRASE * 1000,
    iterations: Infinity,
    pseudoElement: "::before",
  });
  return performance.now();
}

// Fade the desktop in or out under the title card.
function stage(shown, duration) {
  const s = $("stage");
  return s
    .animate([{ opacity: shown ? 0 : 1 }, { opacity: shown ? 1 : 0 }], {
      duration,
      easing: "ease-in-out",
      fill: "forwards",
    })
    .finished.then(() => {
      s.style.opacity = shown ? "1" : "0";
      s.getAnimations().forEach((a) => a.cancel());
    });
}

// The film opens on the title card, as it ends, then the card lifts away
// and the desktop settles in beneath.
async function titleOut() {
  await wait(1300);
  camera(1.08, 640, 360, true);
  const end = $("end");
  CARD().forEach((child, i) =>
    child.animate(
      [
        { opacity: 1, transform: "none" },
        { opacity: 0, transform: "translateY(-10px)" },
      ],
      { duration: 420, delay: i * 45, easing: "ease-in", fill: "forwards" },
    ),
  );
  await wait(260);
  camera(1, 640, 360);
  await stage(true, 800);
  end.style.visibility = "hidden";
  end.getAnimations({ subtree: true }).forEach((a) => a.cancel());
}

// The desktop fades away and the title card returns, element by element,
// coming to rest exactly as the film's first frame once the film is whole
// phrases long (`started`, from drift()).
async function titleIn(started) {
  const end = $("end");
  end.style.visibility = "visible";
  camera(1.04, 640, 360);
  stage(false, 800);
  const rise = { duration: 900, easing: "cubic-bezier(0.22, 1, 0.36, 1)", fill: "backwards" };
  const [img, h1, line, url] = CARD();
  img.animate(
    [
      { opacity: 0, transform: "scale(0.94)" },
      { opacity: 1, transform: "none" },
    ],
    { ...rise, delay: 350 },
  );
  for (const [i, node] of [h1, line, url].entries()) {
    node.animate(
      [
        { opacity: 0, transform: "translateY(12px)" },
        { opacity: 1, transform: "none" },
      ],
      { ...rise, delay: 480 + i * 110 },
    );
  }
  // The underline draws under "smol", then the full stop lands.
  line.querySelector(".smol").animate([{ backgroundSize: "0% 0.09em" }, { backgroundSize: "100% 0.09em" }], {
    ...rise,
    duration: 700,
    delay: 1150,
  });
  line.querySelector(".dot").animate(
    [
      { opacity: 0, transform: "translateY(-0.3em)" },
      { opacity: 1, transform: "none" },
    ],
    { ...rise, duration: 600, delay: 1500 },
  );
  await wait(2100);
  camera(1, 640, 360, true);
  // Hold on the rest frame until the next phrase ends (and at least 0.6 s).
  const played = (performance.now() - started) / slow / 1000;
  await wait((Math.ceil((played + 0.6) / PHRASE) * PHRASE - played) * 1000);
}

async function play() {
  drawNotes();
  const started = drift();
  window.filmMark?.("start");
  await titleOut();
  $("notes").classList.remove("inactive");
  await caption("01", "Answers as you type.");
  await wait(900);
  await typeNotes("Desk depth: ");
  await wait(500);

  // A conversion, pasted where you were typing.
  await summonKey(350);
  await typeQuery("40cm to in");
  window.filmMark?.("poster");
  await wait(1600);
  await pressEnter();
  writeNotes(ANSWERS["40cm to in"].result);
  await wait(1500);
  writeNotes("\n");
  caption("02", "Every emoji, by name.");
  await typeNotes("Party on Friday ");
  await wait(400);

  // An emoji, found by name.
  await summonKey(300);
  await typeQuery("emoji");
  await wait(800);
  showKeys("Enter");
  await wait(100);
  navigate("emoji", true);
  await wait(650);
  await typeQuery("party");
  await wait(1100);
  showKeys("→");
  select(1);
  await wait(900);
  await pressEnter();
  notes.parts.push({ emoji: ui.rows[ui.sel].code });
  drawNotes();
  await wait(1500);
  writeNotes("\n");
  caption("03", "Your clipboard, remembered.");
  await typeNotes("Repo: ");
  await wait(400);

  // Something copied earlier, from clipboard history.
  await summonKey(300);
  await typeQuery("clip");
  await wait(700);
  showKeys("Enter");
  await wait(100);
  navigate("clipboard", true);
  await wait(1100);
  showKeys("↓");
  select(1);
  await wait(700);
  showKeys("↑");
  select(0);
  await wait(800);
  await pressEnter();
  writeNotes(CLIPS[0].text);
  await wait(1300);

  // Anything else falls through to the web or your files.
  await caption("04", "No match? The web or your files.");
  await wait(500);
  await summonKey(300);
  await typeQuery("rust egui");
  await wait(1400);
  showKeys("↓");
  select(1);
  await wait(1300);
  showKeys("Esc");
  await wait(80);
  await dismiss();
  lean(false);
  await wait(600);
  await captionOut();
  await wait(700);
  await titleIn(started);
}

// Ready once the search results and the fonts are in.
window.ready = Promise.all([
  fetch("film/emoji-search.json")
    .then((r) => r.json())
    .then((data) => {
      SEARCH = data.results;
      AVAILABLE = new Set(data.available);
    }),
  document.fonts.load('14px "Selawik"'),
  document.fonts.load('600 14px "Selawik"'),
]).then(() => drawNotes());
window.play = play;
