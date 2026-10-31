// The site's palette: a working recreation of smolcast's root view
// (docs/DESIGN.md). Rows are the home page's sections and the site's other
// pages; the calculator, the emoji picker, a quicklink and Toggle Dark Mode
// work as they do in the app. On the home page it is the hero; elsewhere
// Ctrl+K opens it over the page.
"use strict";

const REPO = "https://github.com/Archetypically/smolcast";
const ICONS = "/icons.svg";
const KEEP_STATE_MS = 30_000;
// This page's path as the rows name it: "/privacy", "/compare/".
const HERE = location.pathname.replace(/(index)?\.html$/, "");
const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

document.documentElement.classList.add("js");

// ---------------------------------------------------------------- rows

// A row that opens another page of the site; a comparison gets the doc icon.
function pageRow(name, path, words, icon = "doc") {
  return { name, type: "Page", icon, page: path, words };
}

const ROWS = [
  {
    name: "Download smolcast",
    type: "Install",
    icon: "download",
    jump: "#install",
    words: "install get exe release setup",
  },
  { name: "Features", type: "Section", icon: "grid", jump: "#features", words: "what can it do" },
  {
    name: "App Launcher",
    type: "Feature",
    icon: "search",
    jump: "#launcher",
    words: "apps programs start menu favorites",
  },
  {
    name: "Calculator and Conversions",
    type: "Feature",
    icon: "calc",
    jump: "#calculator",
    words: "math units convert",
  },
  {
    name: "Clipboard History",
    type: "Feature",
    icon: "clipboard",
    jump: "#clipboard",
    words: "copy paste win+v",
  },
  {
    name: "Emoji & Symbols",
    type: "Command",
    icon: "emoji",
    view: "emoji",
    words: "picker emoticon smiley symbols",
  },
  {
    name: "Search Files",
    type: "Feature",
    icon: "folder",
    jump: "#files",
    words: "explorer documents index",
  },
  {
    name: "System Commands",
    type: "Feature",
    icon: "power",
    jump: "#system",
    words: "lock sleep restart shut down sign out recycle mute",
  },
  {
    name: "Quicklinks and Web Search",
    type: "Feature",
    icon: "link",
    jump: "#quicklinks",
    words: "url google bing duckduckgo kagi",
  },
  {
    name: "Scripts and Snippets",
    type: "Feature",
    icon: "terminal",
    jump: "#scripts",
    words: "powershell text expansion",
  },
  { name: "In Motion", type: "Section", icon: "star", jump: "#motion", words: "video demo animation" },
  { name: "Keyboard Shortcuts", type: "Section", icon: "keyboard", jump: "#keyboard", words: "keys hotkey" },
  {
    name: "Small by Design",
    type: "Section",
    icon: "feather",
    jump: "#small",
    words: "size cpu memory native rust electron",
  },
  { name: "Privacy", type: "Section", icon: "shield", jump: "#privacy", words: "data telemetry network" },
  pageRow("Privacy Policy", "/privacy", "cookies analytics logs contact", "shield"),
  pageRow(
    "Code Signing Policy",
    "/code-signing",
    "signpath certificate signature smartscreen verify",
    "check",
  ),
  { name: "Configuration", type: "Section", icon: "gear", jump: "#config", words: "settings toml config" },
  {
    name: "Build from Source",
    type: "Section",
    icon: "code",
    jump: "#install-source",
    words: "rust cargo task compile",
  },
  {
    name: "Questions and Answers",
    type: "Section",
    icon: "info",
    jump: "#faq",
    words: "faq help questions uninstall free",
  },
  pageRow("Compare Windows Launchers", "/compare/", "alternatives versus vs", "grid"),
  pageRow("smolcast vs Raycast", "/compare/raycast", "raycast for windows alternative"),
  pageRow(
    "smolcast vs PowerToys Command Palette",
    "/compare/powertoys-command-palette",
    "cmdpal powertoys run microsoft alt space",
  ),
  pageRow("smolcast vs Flow Launcher", "/compare/flow-launcher", "wox alternative"),
  pageRow("smolcast vs Tinycast", "/compare/tinycast", "mac macos for windows"),
  pageRow("smolcast vs Fluent Search", "/compare/fluent-search", "search alternative"),
  pageRow("smolcast vs Listary", "/compare/listary", "files dialogs alternative"),
  pageRow("smolcast vs Wox", "/compare/wox", "cross platform ai alternative"),
  pageRow("smolcast vs Alfred", "/compare/alfred", "mac macos for windows alternative"),
  pageRow("smolcast vs Keypirinha", "/compare/keypirinha", "portable python alternative"),
  pageRow("smolcast vs Ueli", "/compare/ueli", "electron cross platform alternative"),
  { name: "Toggle Dark Mode", type: "System", icon: "moon", theme: true, words: "light theme appearance" },
  { name: "View Source on GitHub", type: "Link", icon: "external", href: REPO, words: "code repository" },
  {
    name: "Report an Issue",
    type: "Link",
    icon: "external",
    href: `${REPO}/issues/new`,
    words: "bug feedback",
  },
  { name: "About smolcast", type: "Command", icon: "info", jump: "#about", words: "credits version license" },
];

// Looked up with typed text, so no prototype ("constructor") can match.
const QUICKLINKS = Object.assign(Object.create(null), {
  gh: { name: "Search GitHub", url: "https://github.com/search?q={query}" },
});

// Fuzzy match: every query character in order, preferring word starts.
// Consecutive runs and word starts score higher; a keyword hit ranks below
// any name hit. Returns the matched indices of the name, or null.
function match(query, row) {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return { score: 0, hits: [] };
  const name = row.name.toLowerCase();
  const hits = [];
  let score = 0;
  let at = 0;
  let run = 0;
  for (const ch of q) {
    let i = name.indexOf(ch, at);
    if (i === -1) return keywords(query, row);
    if (i !== at || !hits.length) {
      let ws = i;
      while (ws !== -1 && ws > 0 && name[ws - 1] !== " ") ws = name.indexOf(ch, ws + 1);
      if (ws !== -1) i = ws;
    }
    run = hits.length && i === at ? run + 1 : 0;
    score += 1 + run * 2 + (i === 0 || name[i - 1] === " " ? 3 : 0);
    hits.push(i);
    at = i + 1;
  }
  return { score: score - name.length * 0.02, hits };
}

// A typed term that starts one of the words, or sits inside them.
function wordHit(words, term) {
  return words.split(" ").some((w) => w.startsWith(term)) || words.includes(term);
}

function keywords(query, row) {
  return wordHit(row.words, query.toLowerCase().trim()) ? { score: 0.5, hits: [] } : null;
}

// ---------------------------------------------------------------- calculator

const UNITS = Object.create(null);
function unit(dim, factor, name, ...aliases) {
  for (const a of aliases) UNITS[a] = { dim, factor, name };
}
unit("len", 0.001, "Millimeters", "mm", "millimeter", "millimeters");
unit("len", 0.01, "Centimeters", "cm", "centimeter", "centimeters");
unit("len", 1, "Meters", "m", "meter", "meters", "metre", "metres");
unit("len", 1000, "Kilometers", "km", "kilometer", "kilometers");
unit("len", 0.0254, "Inches", "in", "inch", "inches", '"');
unit("len", 0.3048, "Feet", "ft", "foot", "feet", "'");
unit("len", 0.9144, "Yards", "yd", "yard", "yards");
unit("len", 1609.344, "Miles", "mi", "mile", "miles");
unit("mass", 1, "Grams", "g", "gram", "grams");
unit("mass", 1000, "Kilograms", "kg", "kilogram", "kilograms", "kilo", "kilos");
unit("mass", 28.349523125, "Ounces", "oz", "ounce", "ounces");
unit("mass", 453.59237, "Pounds", "lb", "lbs", "pound", "pounds");
unit("vol", 0.001, "Milliliters", "ml", "milliliter", "milliliters");
unit("vol", 1, "Liters", "l", "liter", "liters", "litre", "litres");
unit("vol", 3.785411784, "Gallons", "gal", "gallon", "gallons");
unit("vol", 0.2365882365, "Cups", "cup", "cups");
unit("time", 1, "Seconds", "s", "sec", "secs", "second", "seconds");
unit("time", 60, "Minutes", "min", "mins", "minute", "minutes");
unit("time", 3600, "Hours", "h", "hr", "hrs", "hour", "hours");
unit("time", 86400, "Days", "d", "day", "days");
unit("time", 604800, "Weeks", "wk", "week", "weeks");
unit("data", 1, "Bytes", "b", "byte", "bytes");
unit("data", 1e3, "Kilobytes", "kb");
unit("data", 1e6, "Megabytes", "mb");
unit("data", 1e9, "Gigabytes", "gb");
unit("data", 1e12, "Terabytes", "tb");
unit("data", 1024, "Kibibytes", "kib");
unit("data", 1024 ** 2, "Mebibytes", "mib");
unit("data", 1024 ** 3, "Gibibytes", "gib");
unit("data", 1024 ** 4, "Tebibytes", "tib");
unit("speed", 1, "Meters per Second", "m/s", "mps");
unit("speed", 1 / 3.6, "Kilometers per Hour", "km/h", "kmh", "kph");
unit("speed", 0.44704, "Miles per Hour", "mph");
unit("angle", 1, "Degrees", "deg", "degree", "degrees", "°");
unit("angle", 180 / Math.PI, "Radians", "rad", "radian", "radians");
unit("temp", 0, "Celsius", "c", "°c", "celsius");
unit("temp", 0, "Fahrenheit", "f", "°f", "fahrenheit");
unit("temp", 0, "Kelvin", "k", "kelvin");

const SYMBOL = { Celsius: "°C", Fahrenheit: "°F", Kelvin: "K" };

function toKelvin(v, name) {
  if (name === "Celsius") return v + 273.15;
  if (name === "Fahrenheit") return ((v - 32) * 5) / 9 + 273.15;
  return v;
}

function fromKelvin(v, name) {
  if (name === "Celsius") return v - 273.15;
  if (name === "Fahrenheit") return ((v - 273.15) * 9) / 5 + 32;
  return v;
}

// A small recursive-descent evaluator: + - * / % ^, parentheses, pi, e.
function evaluate(src) {
  const tokens = src.match(/\d*\.?\d+(?:e[+-]?\d+)?|pi|π|e|[-+*/%^()×÷]|\S/gi);
  if (!tokens) return null;
  let pos = 0;
  const peek = () => tokens[pos];
  const next = () => tokens[pos++];

  function primary() {
    const t = next();
    if (t === undefined) throw new Error("end");
    if (t === "(") {
      const v = sum();
      if (next() !== ")") throw new Error("paren");
      return v;
    }
    if (t === "-") return -power();
    if (t === "+") return power();
    if (/^(pi|π)$/i.test(t)) return Math.PI;
    if (/^e$/i.test(t)) return Math.E;
    if (/^\d*\.?\d+/.test(t)) return parseFloat(t);
    throw new Error("token");
  }
  function power() {
    const base = primary();
    if (peek() === "^") {
      next();
      return base ** power();
    }
    return base;
  }
  function product() {
    let v = power();
    while (/^[*/%×÷]$/.test(peek() ?? "")) {
      const op = next();
      const r = power();
      v = op === "*" || op === "×" ? v * r : op === "%" ? v % r : v / r;
    }
    return v;
  }
  function sum() {
    let v = product();
    while (peek() === "+" || peek() === "-") {
      v = next() === "+" ? v + product() : v - product();
    }
    return v;
  }
  try {
    const v = sum();
    return pos === tokens.length && Number.isFinite(v) ? v : null;
  } catch {
    return null;
  }
}

// As core::calc::format_number: up to 10 significant digits, trailing zeros
// trimmed, scientific notation outside 1e-6 to 1e15.
function format(v) {
  if (v === 0) return "0";
  const magnitude = Math.floor(Math.log10(Math.abs(v)));
  if (magnitude < -6 || magnitude >= 15) {
    const [mantissa, exponent] = v.toExponential(6).split("e");
    return `${trim(mantissa)}e${exponent.replace("+", "")}`;
  }
  const text = trim(v.toFixed(Math.max(0, 9 - magnitude)));
  return text === "-0" ? "0" : text;
}

function trim(text) {
  return text.includes(".") ? text.replace(/0+$/, "").replace(/\.$/, "") : text;
}

function calculate(query) {
  const q = query.trim();
  const conv = q.match(/^(-?[\d.]+(?:e[+-]?\d+)?)\s*([a-z°"'/µ]+)\s+(?:to|in|as|into|=)\s+([a-z°"'/µ]+)$/i);
  if (conv) {
    const from = UNITS[conv[2].toLowerCase()] ?? UNITS[conv[2]];
    const to = UNITS[conv[3].toLowerCase()] ?? UNITS[conv[3]];
    const v = parseFloat(conv[1]);
    if (!from || !to || from.dim !== to.dim || !Number.isFinite(v)) return null;
    const out =
      from.dim === "temp" ? fromKelvin(toKelvin(v, from.name), to.name) : (v * from.factor) / to.factor;
    const label = (u, raw) => SYMBOL[u.name] ?? raw;
    return {
      input: `${format(v)} ${label(from, conv[2])}`,
      inCap: from.name,
      output: `${format(out)} ${label(to, conv[3])}`,
      outCap: to.name,
      answer: format(out),
    };
  }
  if (!/[-+*/%^()×÷]/.test(q.replace(/^-/, "")) || !/\d/.test(q)) return null;
  const v = evaluate(q);
  if (v === null) return null;
  return { input: q, inCap: "Expression", output: format(v), outCap: "Result", answer: format(v) };
}

// ---------------------------------------------------------------- emoji

// A sample of the full Unicode set the app ships, enough to search.
const EMOJI =
  `😀 grinning smile happy|😂 joy laugh tears|🙂 slight smile|😉 wink|😍 heart eyes love|🥰 smiling hearts love|😎 sunglasses cool|🤔 thinking hmm|🫡 salute|🙃 upside down|😅 sweat smile|😭 crying sob|😴 sleeping|🤯 mind blown exploding|🥲 smiling tear|😬 grimacing|🤗 hugging|🫠 melting|🥳 partying party face|😇 halo innocent|👍 thumbs up yes ok|👎 thumbs down no|👏 clap applause|🙌 raised hands hooray|🙏 folded hands please thanks|👋 wave hello bye|✌️ victory peace|🤞 crossed fingers luck|💪 flexed biceps strong|👀 eyes look|🧠 brain|🫶 heart hands|✨ sparkles|🔥 fire lit|💯 hundred perfect|❤️ red heart love|🧡 orange heart|💛 yellow heart|💚 green heart|💙 blue heart|💜 purple heart|🖤 black heart|🤍 white heart|💔 broken heart|⭐ star|🌟 glowing star|⚡ zap lightning|☕ coffee hot beverage|🍵 tea|🍕 pizza|🍔 burger hamburger|🌮 taco|🍣 sushi|🍩 doughnut donut|🍪 cookie|🎂 birthday cake party|🍾 champagne bottle party|🎉 party popper tada celebrate|🎊 confetti ball party|🎈 balloon party|🪅 pinata party|🪩 mirror ball disco party|🎁 gift present|🏆 trophy win|🥇 gold medal first|🚀 rocket launch ship|✈️ airplane travel|🚗 car|🚲 bicycle bike|🏠 house home|🌍 globe earth world|🌙 crescent moon night|☀️ sun sunny|🌈 rainbow|❄️ snowflake cold|🌧️ rain cloud|🌱 seedling plant|🌸 cherry blossom flower|🍀 four leaf clover luck|🐶 dog puppy|🐱 cat kitten|🦊 fox|🐼 panda|🐧 penguin|🦀 crab rust ferris|🐙 octopus|🦄 unicorn|🐝 bee|🦋 butterfly|💻 laptop computer|⌨️ keyboard|🖥️ desktop computer|🖱️ mouse|📱 phone mobile|🔋 battery|🔌 plug electric|💡 bulb idea|🔍 magnifying glass search|🔒 lock locked|🔑 key|🛠️ tools hammer wrench|⚙️ gear settings|🧪 test tube|📦 package box|📎 paperclip|📌 pushpin pin|📝 memo note|📅 calendar date|⏰ alarm clock|⏳ hourglass wait|✅ check mark done|❌ cross mark no|⚠️ warning|❓ question|💬 speech balloon chat|📣 megaphone|🔔 bell notification|🎵 music note|🎧 headphones|🎮 video game controller|♟️ chess pawn|🎯 direct hit target bullseye|🧩 puzzle piece|🪄 magic wand|🫖 teapot|🧋 bubble tea|🐢 turtle slow|🐇 rabbit fast|🦆 duck|🫧 bubbles|🍂 fallen leaf autumn|🎃 jack o lantern pumpkin halloween|👻 ghost halloween|🤖 robot|👾 alien monster game|🧑‍💻 technologist coder developer|🏳️‍🌈 rainbow flag pride`
    .split("|")
    .map((entry) => {
      const [char, ...words] = entry.split(" ");
      return { char, name: words.join(" ") };
    });

// ---------------------------------------------------------------- palette

const STRIDE = 42;
const ROW = 38;
const HEADER = 30;
const CARD = 2 * STRIDE + ROW - HEADER + 4;
const CELL = 48;

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else node.setAttribute(k, v);
  }
  for (const child of children) if (child != null) node.append(child);
  return node;
}

function svgEl(tag, attrs = {}) {
  const node = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  return node;
}

function icon(id) {
  const svg = svgEl("svg", { "aria-hidden": "true" });
  svg.append(svgEl("use", { href: `${ICONS}#${id}` }));
  return svg;
}

// The arrow between a calculator card's two values.
function arrow() {
  const svg = svgEl("svg", { class: "arrow", viewBox: "0 0 24 12", "aria-hidden": "true" });
  svg.append(svgEl("path", { d: "M1 6h20M16 1.5 21 6l-5 4.5" }));
  return svg;
}

function caps(...keys) {
  const frag = document.createDocumentFragment();
  for (const [label, key] of keys) frag.append(el("kbd", { class: "cap", "data-key": key, text: label }));
  return frag;
}

function highlight(name, hits) {
  const span = el("span", { class: "name" });
  if (!hits.length) {
    span.textContent = name;
    return span;
  }
  const set = new Set(hits);
  let buf = "";
  let inMark = false;
  const flush = () => {
    if (!buf) return;
    span.append(inMark ? el("mark", { text: buf }) : buf);
    buf = "";
  };
  [...name].forEach((ch, i) => {
    if (set.has(i) !== inMark) {
      flush();
      inMark = set.has(i);
    }
    buf += ch;
  });
  flush();
  return span;
}

class Palette {
  constructor(root, { onJump, onPage, onTheme, onDismiss } = {}) {
    this.root = root;
    this.onJump = onJump;
    this.onPage = onPage;
    this.onTheme = onTheme;
    this.onDismiss = onDismiss;
    this.view = "root";
    this.query = "";
    this.sel = 0;
    this.offset = 0;
    this.build();
    this.render();
  }

  build() {
    this.root.replaceChildren();
    this.root.setAttribute("role", "search");
    this.native = matchMedia("(pointer: coarse)").matches;
    this.root.classList.toggle("native", this.native);
    this.glyphSlot = el("span", { class: "pal-glyph" }, icon("search"));
    this.input = el("input", {
      class: "pal-input",
      type: "text",
      role: "combobox",
      "aria-expanded": "true",
      "aria-controls": "pal-list",
      "aria-autocomplete": "list",
      "aria-label": "Search this site",
      autocomplete: "off",
      autocapitalize: "off",
      spellcheck: "false",
      enterkeyhint: "go",
    });
    this.search = el("div", { class: "pal-search" }, this.glyphSlot, this.input);
    this.header = el("div", { class: "pal-header" });
    this.selBox = el("div", { class: "pal-sel" });
    this.list = el("div", { class: "pal-list", id: "pal-list", role: "listbox", "aria-label": "Results" });
    this.content = el("div", { class: "pal-content" }, this.selBox, this.list);
    this.clip = el("div", { class: "pal-clip" }, this.content);
    this.viewport = el("div", { class: "pal-viewport" }, this.header, this.clip);
    this.footerLeft = el("div", { class: "left" });
    this.footerRight = el("div", { class: "right" });
    this.footer = el("div", { class: "pal-footer" }, this.footerLeft, this.footerRight);
    this.root.append(
      this.search,
      el("div", { class: "rule" }),
      el("div", { class: "pal-body" }, this.viewport),
      el("div", { class: "rule" }),
      this.footer,
    );

    this.input.addEventListener("input", () => {
      this.query = this.input.value;
      this.sel = 0;
      this.offset = 0;
      this.render();
    });
    this.input.addEventListener("keydown", (e) => this.key(e));
    this.viewport.addEventListener("wheel", (e) => this.wheel(e), { passive: false });
    this.list.addEventListener("click", (e) => {
      const target = e.target.closest("[data-index]");
      if (!target) return;
      this.sel = Number(target.dataset.index);
      this.render();
      this.activate();
    });
    // Touch screens scroll the list natively (snapping to rows, handing the
    // swipe to the page at either end); the offset follows the scroll so
    // edge rows still dim.
    if (this.native) {
      let queued = false;
      this.clip.addEventListener(
        "scroll",
        () => {
          if (queued) return;
          queued = true;
          requestAnimationFrame(() => {
            queued = false;
            this.offset = this.clip.scrollTop;
            this.updateEdges();
          });
        },
        { passive: true },
      );
    }
  }

  // ---- model

  items() {
    if (this.view === "emoji") {
      const q = this.query.trim().toLowerCase();
      const cells = EMOJI.filter((e) => !q || wordHit(e.name, q));
      return { header: q ? "Results" : "Emoji", items: cells.map((e) => ({ kind: "cell", ...e })) };
    }

    const q = this.query;
    const text = q.trim();
    const items = [];
    let header = text ? "Results" : "Suggestions";

    const calc = calculate(q);
    if (calc) {
      header = "Calculator";
      items.push({ kind: "card", calc });
      items.push({ kind: "header", text: "Results" });
    }

    const ql = q.match(/^(\S+)(?:\s+(.*))?$/);
    const link = ql && QUICKLINKS[ql[1].toLowerCase()];
    if (link) {
      const rest = (ql[2] ?? "").trim();
      items.push({
        kind: "row",
        icon: "link",
        name: rest ? `${link.name} "${rest}"` : link.name,
        type: "Quicklink",
        hits: [],
        quicklink: { ...link, id: ql[1].toLowerCase(), rest },
      });
    }

    const ranked = ROWS.filter((row) => row.page !== HERE)
      .map((row) => ({ row, m: match(text, row) }))
      .filter((r) => r.m)
      .sort((a, b) => (text ? b.m.score - a.m.score : 0));
    for (const { row, m } of ranked) items.push({ kind: "row", ...row, hits: m.hits });

    if (text && !calc && !link) {
      const short = text.length > 40 ? `${text.slice(0, 40)}…` : text;
      if (items.length) items.push({ kind: "header", text: `Search for "${short}"` });
      items.push({
        kind: "row",
        icon: "globe",
        name: `Search Issues for "${short}"`,
        type: "Web Search",
        hits: [],
        href: `${REPO}/issues?q=${encodeURIComponent(text)}`,
      });
    }
    if (calc && items.length === 2) items.pop();
    return { header, items };
  }

  selectable(items) {
    return items.map((it, i) => (it.kind === "header" ? -1 : i)).filter((i) => i >= 0);
  }

  // ---- layout

  columns() {
    const width = this.viewport.clientWidth || 600;
    return Math.max(6, Math.min(12, Math.floor((width + 4) / CELL)));
  }

  layout(items) {
    if (this.view === "emoji") {
      const cols = this.columns();
      return items.map((_, i) => ({
        top: Math.floor(i / cols) * CELL,
        left: i % cols,
        h: CELL - 4,
      }));
    }
    let top = 0;
    return items.map((it) => {
      const h = it.kind === "header" ? HEADER : it.kind === "card" ? CARD : STRIDE;
      const box = { top, h: h - (it.kind === "header" ? 0 : 4) };
      top += h;
      return box;
    });
  }

  visibleHeight() {
    return this.clip.clientHeight;
  }

  ensureVisible(boxes) {
    const box = boxes[this.sel];
    if (!box) return;
    const h = this.visibleHeight();
    if (box.top < this.offset) this.offset = box.top;
    else if (box.top + box.h + 4 > this.offset + h) this.offset = box.top + box.h + 4 - h;
    // Keep a header that leads the selected row in view with it.
    const prev = this.model.items[this.sel - 1];
    if (prev?.kind === "header" && boxes[this.sel - 1].top < this.offset)
      this.offset = boxes[this.sel - 1].top;
    if (this.sel === this.selectable(this.model.items)[0]) this.offset = 0;
  }

  maxOffset() {
    const last = this.boxes[this.boxes.length - 1];
    if (!last) return 0;
    return Math.max(0, last.top + last.h + 4 - this.visibleHeight());
  }

  // ---- render

  // fresh: rows enter with a short stagger (a view opening).
  // keepOffset: a scroll set the offset; do not snap it to the selection.
  render(fresh = false, keepOffset = false) {
    this.model = this.items();
    const { header, items } = this.model;
    const picks = this.selectable(items);
    if (!picks.includes(this.sel)) this.sel = picks[0] ?? -1;

    const emoji = this.view === "emoji";
    this.root.classList.toggle("emoji", emoji);
    this.glyphSlot.replaceChildren();
    if (emoji) {
      const back = el(
        "button",
        { class: "pal-back", type: "button", "aria-label": "Back (Esc)" },
        icon("back"),
      );
      back.addEventListener("click", () => this.back());
      this.glyphSlot.append(back);
    } else {
      this.glyphSlot.append(icon("search"));
    }
    this.input.placeholder = emoji ? "Search emoji & symbols…" : "Search apps and commands…";
    if (this.input.value !== this.query) this.input.value = this.query;

    this.header.replaceChildren(el("span", { text: header }));
    const boxes = this.layout(items);
    this.boxes = boxes;
    if (!keepOffset) this.ensureVisible(boxes);
    this.offset = Math.min(this.offset, this.maxOffset());

    this.list.replaceChildren();
    this.list.classList.toggle("pal-grid", emoji);
    if (emoji) this.list.style.setProperty("--cols", this.columns());

    if (!items.length) {
      this.list.append(el("p", { class: "pal-empty", text: emoji ? "No emoji match." : "Nothing matches." }));
    }

    items.forEach((it, i) => {
      const id = `pal-opt-${i}`;
      if (it.kind === "header") {
        this.list.append(el("div", { class: "pal-header", role: "presentation", text: it.text }));
        return;
      }
      if (it.kind === "cell") {
        this.list.append(
          el("div", {
            class: `pal-cell${i === this.sel ? " sel" : ""}`,
            role: "option",
            id,
            "data-index": i,
            "aria-selected": String(i === this.sel),
            "aria-label": it.name,
            text: it.char,
          }),
        );
        return;
      }
      if (it.kind === "card") {
        const { calc } = it;
        this.list.append(
          el(
            "div",
            {
              class: `pal-card${i === this.sel ? " sel" : ""}`,
              role: "option",
              id,
              "data-index": i,
              "aria-selected": String(i === this.sel),
              "aria-label": `${calc.input} equals ${calc.output}`,
            },
            el(
              "div",
              { class: "val" },
              el("span", { class: "big", text: calc.input }),
              el("span", { class: "pill", text: calc.inCap }),
            ),
            arrow(),
            el(
              "div",
              { class: "val result" },
              el("span", { class: "big", text: calc.output }),
              el("span", { class: "pill", text: calc.outCap }),
            ),
          ),
        );
        return;
      }
      const row = el(
        "div",
        {
          class: `pal-row${i === this.sel ? " sel" : ""}${fresh && i < 12 ? " enter" : ""}`,
          role: "option",
          id,
          "data-index": i,
          "aria-selected": String(i === this.sel),
        },
        el("span", { class: "icon" }, icon(it.icon)),
        highlight(it.name, it.hits),
        el("span", { class: "trail", text: it.type }),
      );
      if (fresh) row.style.animationDelay = `${Math.min(i, 8) * 12}ms`;
      this.list.append(row);
    });

    // Rebuilding the list clamps a native scroll to the top; put it back.
    if (!this.native) this.content.style.transform = `translateY(${-this.offset}px)`;
    else if (Math.abs(this.clip.scrollTop - this.offset) > 1) this.clip.scrollTop = this.offset;
    this.updateEdges();
    const box = boxes[this.sel];
    if (box) {
      this.selBox.style.opacity = "1";
      this.selBox.classList.toggle("grid", emoji);
      if (emoji) {
        const cols = this.columns();
        const w = (this.list.clientWidth - (cols - 1) * 4) / cols;
        this.selBox.style.width = `${w}px`;
        this.selBox.style.right = "auto";
        this.selBox.style.transform = `translate(${box.left * (w + 4)}px, ${box.top}px)`;
      } else {
        this.selBox.style.width = "";
        this.selBox.style.right = "0";
        this.selBox.style.transform = `translateY(${box.top}px)`;
      }
      this.selBox.style.height = `${box.h}px`;
      this.input.setAttribute("aria-activedescendant", `pal-opt-${this.sel}`);
    } else {
      this.selBox.style.opacity = "0";
      this.input.removeAttribute("aria-activedescendant");
    }

    this.renderFooter();
  }

  // Dim the first or last row in view while more rows wait beyond it.
  updateEdges() {
    const boxes = this.boxes;
    const picks = this.selectable(this.model.items);
    const max = this.maxOffset();
    const h = this.visibleHeight();
    const first = picks.find((i) => boxes[i].top >= this.offset - 1);
    const last = [...picks].reverse().find((i) => boxes[i].top + boxes[i].h <= this.offset + h + 1);
    for (const row of this.list.querySelectorAll(".pal-row")) {
      const i = Number(row.dataset.index);
      row.classList.toggle(
        "edge",
        max > 0 && ((i === first && this.offset > 0) || (i === last && this.offset < max)),
      );
    }
  }

  current() {
    return this.model.items[this.sel];
  }

  actionLabel(it) {
    if (!it) return null;
    if (it.kind === "cell") return "Copy Emoji";
    if (it.kind === "card") return "Copy Answer";
    if (it.quicklink && !it.quicklink.rest) return "Type Query";
    if (it.quicklink || it.href) return it.type === "Web Search" ? "Search" : "Open in Browser";
    if (it.theme) return "Toggle";
    if (it.view) return "Open Command";
    if (it.page) return "Open Page";
    return "Jump to Section";
  }

  renderFooter(done) {
    const emoji = this.view === "emoji";
    this.footerLeft.replaceChildren(
      el("span", { class: "where", text: emoji ? "Emoji & Symbols" : "smolcast" }),
    );
    if (!emoji) this.footerLeft.append(caps(["Ctrl", "Control"], ["Alt", "Alt"], ["Space", " "]));
    const label = this.actionLabel(this.current());
    this.footerRight.replaceChildren();
    if (done) {
      this.footerRight.append(el("span", { class: "act done" }, icon("check"), done));
    } else if (label) {
      const key = label === "Type Query" ? ["Tab", "Tab"] : ["↵", "Enter"];
      this.footerRight.append(el("span", { class: "act", text: label }), caps(key));
    }
    if (emoji)
      this.footerRight.append(
        el("span", { class: "sep" }),
        el("span", { class: "back", text: "Back" }),
        caps(["Esc", "Escape"]),
      );
  }

  flash(text) {
    this.renderFooter(text);
    clearTimeout(this.flashTimer);
    this.flashTimer = setTimeout(() => this.renderFooter(), 1400);
  }

  // ---- input

  move(delta) {
    const { items } = this.model;
    if (this.view === "emoji") {
      const cols = this.columns();
      const step = Math.abs(delta) === 1 ? delta : Math.sign(delta) * cols;
      this.sel = Math.max(0, Math.min(items.length - 1, this.sel + step));
    } else {
      const picks = this.selectable(items);
      const at = picks.indexOf(this.sel);
      this.sel = picks[Math.max(0, Math.min(picks.length - 1, at + Math.sign(delta)))] ?? this.sel;
    }
    this.render();
  }

  scrollBy(rows) {
    const max = this.maxOffset();
    const step = this.view === "emoji" ? CELL : STRIDE;
    const next = Math.max(0, Math.min(max, this.offset + rows * step));
    if (next === this.offset) return false;
    this.offset = next;
    // Keep the selection inside the viewport, as the app's list cursor does.
    const h = this.visibleHeight();
    const box = this.boxes[this.sel];
    if (box && (box.top < next || box.top + box.h > next + h)) {
      const picks = this.selectable(this.model.items);
      const inside = picks.filter(
        (i) => this.boxes[i].top >= next && this.boxes[i].top + this.boxes[i].h <= next + h,
      );
      if (inside.length) this.sel = rows > 0 ? inside[0] : inside[inside.length - 1];
    }
    this.render(false, true);
    return true;
  }

  wheel(e) {
    if (this.native) return;
    this.wheelAcc = (this.wheelAcc ?? 0) + e.deltaY;
    const rows = Math.trunc(this.wheelAcc / 40);
    if (!rows) {
      if (this.maxOffset() > 0 && !this.atEnd(Math.sign(e.deltaY))) e.preventDefault();
      return;
    }
    this.wheelAcc -= rows * 40;
    if (this.scrollBy(rows)) e.preventDefault();
  }

  atEnd(dir) {
    return dir > 0 ? this.offset >= this.maxOffset() : this.offset <= 0;
  }

  key(e) {
    if (e.isComposing) return;
    const emoji = this.view === "emoji";
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        this.move(emoji ? 12 : 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        this.move(emoji ? -12 : -1);
        break;
      case "ArrowRight":
      case "ArrowLeft":
        if (emoji) {
          e.preventDefault();
          this.move(e.key === "ArrowRight" ? 1 : -1);
        }
        break;
      case "PageDown":
      case "PageUp":
        e.preventDefault();
        this.scrollBy(e.key === "PageDown" ? 6 : -6);
        break;
      case "Enter":
        e.preventDefault();
        this.activate();
        break;
      case "Tab": {
        const it = this.current();
        if (it?.quicklink && !it.quicklink.rest) {
          e.preventDefault();
          this.setQuery(`${it.quicklink.id} `);
        }
        break;
      }
      case "Escape":
        e.preventDefault();
        if (emoji) this.back();
        else this.onDismiss?.();
        break;
      case "Backspace":
        if (emoji && !this.input.value) {
          e.preventDefault();
          this.back();
        }
        break;
      default:
    }
  }

  setQuery(text) {
    this.input.value = text;
    this.query = text;
    this.sel = 0;
    this.offset = 0;
    this.render();
  }

  open(view) {
    this.view = view;
    this.rootQuery = this.query;
    this.query = "";
    this.sel = 0;
    this.offset = 0;
    this.slide("forward");
    this.render(true);
  }

  back() {
    this.view = "root";
    this.query = this.rootQuery ?? "";
    this.sel = 0;
    this.offset = 0;
    this.slide("back");
    this.render(true);
  }

  slide(direction) {
    if (reducedMotion.matches) return;
    this.viewport.animate(
      [
        { opacity: 0, transform: `translateX(${direction === "forward" ? 14 : -14}px)` },
        { opacity: 1, transform: "none" },
      ],
      { duration: 180, easing: "cubic-bezier(0.2, 0.8, 0.2, 1)" },
    );
  }

  async copy(text, label) {
    try {
      await navigator.clipboard.writeText(text);
      this.flash(label);
    } catch {
      this.flash("Could not copy");
    }
  }

  activate() {
    const it = this.current();
    if (!it) return;
    this.selBox.classList.remove("blink");
    void this.selBox.offsetWidth;
    this.selBox.classList.add("blink");
    if (it.kind === "cell") return this.copy(it.char, `Copied ${it.char}`);
    if (it.kind === "card") return this.copy(it.calc.answer, "Copied Answer");
    if (it.quicklink) {
      if (!it.quicklink.rest) return this.setQuery(`${it.quicklink.id} `);
      return window.open(
        it.quicklink.url.replace("{query}", encodeURIComponent(it.quicklink.rest)),
        "_blank",
        "noopener",
      );
    }
    if (it.href) return window.open(it.href, "_blank", "noopener");
    if (it.theme) return this.onTheme?.();
    if (it.view) return this.open(it.view);
    if (it.page) return this.onPage?.(it.page);
    if (it.jump) return this.onJump?.(it.jump);
  }
}

// ---------------------------------------------------------------- page

const pressed = new Set();

function keyName(e) {
  if (e.key === "Control" || e.key === "Alt" || e.key === "Shift" || e.key === "Meta") return e.key;
  return e.key.length === 1 ? e.key.toLowerCase() : e.key;
}

function lightCaps(name, on) {
  for (const cap of document.querySelectorAll(`.cap[data-key="${CSS.escape(name)}"]`))
    cap.classList.toggle("down", on);
}

function pressCaps(names, ms = 140) {
  for (const n of names) lightCaps(n, true);
  setTimeout(() => {
    for (const n of names) if (!pressed.has(n)) lightCaps(n, false);
  }, ms);
}

function setTheme(next) {
  const root = document.documentElement;
  root.classList.add("theme-fade");
  root.dataset.theme = next;
  try {
    localStorage.setItem("theme", next);
  } catch {
    /* storage may be unavailable */
  }
  setTimeout(() => root.classList.remove("theme-fade"), 300);
}

function currentTheme() {
  const set = document.documentElement.dataset.theme;
  if (set) return set;
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

let arriveTimers = [];

function arrive(selector) {
  const target = document.querySelector(selector);
  if (!target) return;
  const smooth = !reducedMotion.matches;
  target.scrollIntoView({
    behavior: smooth ? "smooth" : "auto",
    block: target.classList.contains("tile") ? "center" : "start",
  });
  target.focus({ preventScroll: true });
  // A new arrival cancels the last one's timers, so they cannot touch it.
  for (const timer of arriveTimers) clearTimeout(timer);
  for (const old of document.querySelectorAll(".arrived")) old.classList.remove("arrived");
  // Draw the pip once the scroll has mostly landed.
  arriveTimers = [
    setTimeout(() => target.classList.add("arrived"), smooth ? 420 : 0),
    setTimeout(() => target.classList.remove("arrived"), 4000),
  ];
}

// Plays the palette's settle-in again; the reflow restarts the animation.
function replaySettle(node) {
  node.classList.remove("settle");
  void node.offsetWidth;
  node.classList.add("settle");
}

// The home page: the palette is the hero, and dismissing it leaves a note
// with a way back.
function initHero(root) {
  const stage = document.getElementById("stage");
  const dismissed = document.getElementById("dismissed");
  let dismissedAt = 0;

  const palette = new Palette(root, {
    onJump: arrive,
    onPage: (path) => location.assign(path),
    onTheme: () => toggleTheme(palette),
    onDismiss: () => {
      root.classList.add("away");
      palette.input.blur();
      dismissedAt = Date.now();
      setTimeout(() => {
        if (dismissedAt) dismissed.hidden = false;
      }, 120);
    },
  });
  window.smolcast = palette;

  // On touch screens, focusing the field opens the on-screen keyboard, so
  // only a tap on the field itself does that.
  const finePointer = matchMedia("(pointer: fine)").matches;

  const summon = ({ scroll = true, focus = true } = {}) => {
    const wasAway = root.classList.contains("away");
    if (wasAway && Date.now() - dismissedAt > KEEP_STATE_MS) {
      palette.view = "root";
      palette.setQuery("");
    }
    dismissedAt = 0;
    dismissed.hidden = true;
    root.classList.remove("away");
    if (wasAway) replaySettle(root);
    if (scroll && stage.getBoundingClientRect().top < 0) {
      window.scrollTo({ top: 0, behavior: reducedMotion.matches ? "auto" : "smooth" });
    }
    if (!focus) return;
    palette.input.focus({ preventScroll: true });
    palette.input.select();
  };

  // Settle in on load, then take focus only if nothing else has it.
  root.classList.add("settle");
  if (!location.hash && finePointer) palette.input.focus({ preventScroll: true });

  // Hero chips type their query into the palette.
  let typing = null;
  const typeInto = async (text, { speed = 55, signal } = {}) => {
    summon({ scroll: false, focus: finePointer });
    if (palette.view !== "root") palette.back();
    palette.setQuery("");
    for (const ch of text) {
      if (signal?.aborted) return;
      palette.setQuery(palette.query + ch);
      pressCaps([ch.toLowerCase()], 90);
      await wait(reducedMotion.matches ? 0 : speed + ((ch.charCodeAt(0) * 7) % 30));
    }
  };
  for (const chip of document.querySelectorAll("[data-type]")) {
    chip.addEventListener("click", () => {
      typing?.abort();
      typing = new AbortController();
      typeInto(chip.dataset.type, { signal: typing.signal });
    });
  }

  const tourButton = document.getElementById("tour");
  tourButton?.addEventListener("click", () => {
    typing?.abort();
    typing = new AbortController();
    tour(palette, { typeInto, summon, signal: typing.signal });
  });
  // A real key or click in the palette ends a tour.
  root.addEventListener("pointerdown", () => typing?.abort());
  palette.input.addEventListener("keydown", (e) => e.isTrusted && typing?.abort());

  // Top bar hairline and search pill once the palette has scrolled away.
  const topbar = document.querySelector(".topbar");
  if (topbar) {
    new IntersectionObserver(([entry]) => topbar.classList.toggle("scrolled", !entry.isIntersecting), {
      rootMargin: "-60px 0px 0px 0px",
    }).observe(stage);
  }

  // Typing on the page goes to the palette while it is on screen.
  const typeAhead = (e) => {
    const onScreen = stage.getBoundingClientRect().bottom > 80;
    if (onScreen && !root.classList.contains("away") && e.key.length === 1)
      palette.input.focus({ preventScroll: true });
    if (onScreen && root.classList.contains("away") && e.key === "Enter") summon();
  };
  return { summon, typeAhead };
}

// Every other page: the palette opens over the page, as smolcast opens over
// your work, and Esc or a click outside puts it away.
function initOverlay() {
  const root = el("div", { class: "palette", id: "palette" });
  const dialog = el(
    "div",
    { class: "stage", role: "dialog", "aria-modal": "true", "aria-label": "smolcast" },
    root,
  );
  const scrim = el("div", { class: "overlay", hidden: "" }, dialog);
  document.body.append(scrim);
  let palette = null;
  let returnTo = null;
  let dismissedAt = 0;

  // While the palette is open, the page behind it is inert, so Tab stays
  // in the dialog as aria-modal promises.
  const behind = () => [...document.body.children].filter((n) => n !== scrim);
  const dismiss = () => {
    if (scrim.hidden) return;
    scrim.hidden = true;
    for (const n of behind()) n.inert = false;
    dismissedAt = Date.now();
    returnTo?.focus?.({ preventScroll: true });
  };
  const summon = () => {
    if (!scrim.hidden) return palette.input.select();
    palette ??= new Palette(root, {
      onJump: (hash) => location.assign(`/${hash}`),
      onPage: (path) => location.assign(path),
      onTheme: () => toggleTheme(palette),
      onDismiss: dismiss,
    });
    if (Date.now() - dismissedAt > KEEP_STATE_MS) {
      palette.view = "root";
      palette.setQuery("");
    }
    returnTo = document.activeElement;
    scrim.hidden = false;
    for (const n of behind()) n.inert = true;
    palette.render();
    replaySettle(root);
    palette.input.focus({ preventScroll: true });
    palette.input.select();
  };
  scrim.addEventListener("pointerdown", (e) => e.target === scrim && dismiss());

  // The top bar always offers the palette here, and draws its hairline once
  // the page scrolls.
  const topbar = document.querySelector(".topbar");
  if (topbar) {
    topbar.classList.add("paged");
    const edge = () => topbar.classList.toggle("scrolled", window.scrollY > 4);
    window.addEventListener("scroll", edge, { passive: true });
    edge();
  }
  return { summon };
}

function toggleTheme(palette) {
  const next = currentTheme() === "dark" ? "light" : "dark";
  setTheme(next);
  palette.flash(next === "dark" ? "Dark Mode On" : "Dark Mode Off");
}

// The visitor's system, as the comparison tables name them: "win11",
// "win10", "windows" (a Windows the browser will not version) or "mac".
async function visitorOS() {
  const hints = navigator.userAgentData;
  const platform = hints?.platform || navigator.platform || "";
  const ua = navigator.userAgent;
  if (/mac/i.test(platform) || (/Macintosh/.test(ua) && !/Mobile/.test(ua))) return "mac";
  if (!/win/i.test(platform) && !/Windows NT/.test(ua)) return null;
  // Chromium reports Windows 11 as platform version 13 and up; others can't tell.
  try {
    const { platformVersion } = await hints.getHighEntropyValues(["platformVersion"]);
    return Number(platformVersion.split(".")[0]) >= 13 ? "win11" : "win10";
  } catch {
    return "windows";
  }
}

// In a comparison's "Runs on" row, bold the tools that run on this computer.
async function markYourOS() {
  const cells = document.querySelectorAll("td[data-os]");
  if (!cells.length) return;
  const os = await visitorOS();
  if (!os) return;
  for (const td of cells) {
    const runs = td.dataset.os.split(" ");
    const fits = os === "windows" ? runs.some((r) => r.startsWith("win")) : runs.includes(os);
    if (!fits) continue;
    td.classList.add("yours");
    td.append(el("span", { class: "note", text: "Runs on your computer" }));
  }
}

function init() {
  markYourOS();
  try {
    const saved = localStorage.getItem("theme");
    if (saved === "dark" || saved === "light") document.documentElement.dataset.theme = saved;
  } catch {
    /* storage may be unavailable */
  }

  const hero = document.getElementById("palette");
  const mode = hero ? initHero(hero) : initOverlay();
  for (const b of document.querySelectorAll("[data-summon]"))
    b.addEventListener("click", () => mode.summon());

  // On the home page the brand scrolls back up instead of reloading.
  if (hero) {
    for (const brand of document.querySelectorAll("a.brand")) {
      brand.addEventListener("click", (e) => {
        e.preventDefault();
        history.replaceState(null, "", "/");
        window.scrollTo({ top: 0, behavior: reducedMotion.matches ? "auto" : "smooth" });
      });
    }
  }

  // Global keys: Ctrl+K, Ctrl+Alt+Space and / summon; key caps follow your
  // keys; the mode decides where other typing goes.
  document.addEventListener("keydown", (e) => {
    const name = keyName(e);
    pressed.add(name);
    lightCaps(name, true);
    const inField = e.target instanceof HTMLElement && e.target.matches("input, textarea, [contenteditable]");
    const ctrlK = (e.ctrlKey || e.metaKey) && name === "k";
    const ctrlAltSpace = e.ctrlKey && e.altKey && e.code === "Space";
    if (ctrlK || ctrlAltSpace || (name === "/" && !inField)) {
      e.preventDefault();
      mode.summon();
      return;
    }
    if (inField || e.ctrlKey || e.metaKey || e.altKey) return;
    mode.typeAhead?.(e);
  });
  document.addEventListener("keyup", (e) => {
    const name = keyName(e);
    pressed.delete(name);
    lightCaps(name, false);
    if (name === "Control" || name === "Alt" || name === "Shift") {
      // A released modifier can swallow its keys' keyups.
      for (const k of [...pressed.keys()]) {
        pressed.delete(k);
        lightCaps(k, false);
      }
    }
  });
  window.addEventListener("blur", () => {
    for (const k of pressed.keys()) lightCaps(k, false);
    pressed.clear();
  });

  // Copy buttons.
  for (const block of document.querySelectorAll("[data-copy-target]")) {
    const button = block.querySelector(".copy");
    const label = button.querySelector("span");
    const use = button.querySelector("use");
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(block.querySelector("pre").innerText);
        label.textContent = "Copied";
        button.classList.add("done");
        use.setAttribute("href", `${ICONS}#check`);
      } catch {
        label.textContent = "Select and copy";
      }
      setTimeout(() => {
        label.textContent = "Copy";
        button.classList.remove("done");
        use.setAttribute("href", `${ICONS}#copy`);
      }, 1600);
    });
  }

  // Reveal sections once, as they arrive.
  const reveal = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        entry.target.classList.add("in");
        reveal.unobserve(entry.target);
      }
    },
    { rootMargin: "0px 0px -8% 0px" },
  );
  for (const node of document.querySelectorAll(
    ".section-head, .tile, .also, .film, .keys, .figures, .prose, .code, .install, .source",
  )) {
    node.classList.add("reveal");
    reveal.observe(node);
  }

  // The film plays muted while on screen (its controls unmute it), and not
  // on its own at all with reduced motion.
  const film = document.getElementById("film");
  if (film && !reducedMotion.matches) {
    new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) film.play().catch(() => {});
        else film.pause();
      },
      { threshold: 0.35 },
    ).observe(film);
  }

  // The footer heart beats once when it comes into view, like the About page.
  const heart = document.querySelector(".heart");
  if (heart) {
    new IntersectionObserver(([entry], obs) => {
      if (!entry.isIntersecting) return;
      heart.classList.add("beat");
      obs.disconnect();
    }).observe(heart);
    heart.addEventListener("animationend", () => heart.classList.remove("beat"));
  }

  if (location.hash) setTimeout(() => arrive(location.hash), 60);
  window.addEventListener("hashchange", () => arrive(location.hash));
}

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// The tour: the same script the demo video records (website/media/).
async function tour(palette, { typeInto, signal }) {
  const step = async (ms) => {
    await wait(reducedMotion.matches ? Math.min(ms, 400) : ms);
    if (signal.aborted) throw new Error("stopped");
  };
  const press = async (key, fn) => {
    pressCaps([key], 160);
    fn();
    await step(120);
  };
  try {
    await typeInto("40cm to in", { signal });
    await step(900);
    await press("Enter", () => palette.activate());
    await step(1400);
    await typeInto("emoji", { signal });
    await step(600);
    await press("Enter", () => palette.activate());
    await step(500);
    for (const ch of "party") {
      palette.setQuery(palette.query + ch);
      pressCaps([ch], 90);
      await step(70);
    }
    await step(600);
    await press("ArrowRight", () => palette.move(1));
    await step(350);
    await press("ArrowLeft", () => palette.move(-1));
    await step(400);
    await press("Enter", () => palette.activate());
    await step(1300);
    await press("Escape", () => palette.back());
    await step(500);
    await typeInto("dark", { signal });
    await step(700);
    await press("Enter", () => palette.activate());
    await step(1600);
    await press("Enter", () => palette.activate());
    await step(900);
    palette.setQuery("");
  } catch {
    /* stopped by the visitor */
  }
}

if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
else init();
