// Keeps what the site shares in one place without a build step, so public/
// stays deployable as-is:
//
// - Page chrome: every page carries its own copy of the head links, top bar
//   and footer between `<!-- partial:NAME -->` and `<!-- /partial:NAME -->`
//   markers; partials/NAME.html is stamped into each one. Partials are
//   written as Prettier formats them at a page's four-space indent.
// - Colors: public/tokens.css is generated from the app's own palette in
//   src/app/theme.rs, so the site, the film and the app change together.
//
// With --check it changes nothing and fails on any drift, and on an
// indexable page missing from sitemap.xml, llms.txt or home's links, or
// missing its social card and the tags that describe it.
import { readFile, readdir, stat, writeFile } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const site = resolve(fileURLToPath(new URL("..", import.meta.url)));
const themeRs = join(site, "..", "src", "app", "theme.rs");
const pub = join(site, "public");
const check = process.argv.includes("--check");

const marker = /^([ \t]*)<!-- partial:([a-z-]+) -->\n[\s\S]*?^[ \t]*<!-- \/partial:\2 -->$/gm;

async function pages(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory() && entry.name !== "media") out.push(...(await pages(path)));
    else if (entry.name.endsWith(".html")) out.push(path);
  }
  return out;
}

const partials = new Map();
async function partial(name) {
  if (!partials.has(name)) {
    const text = await readFile(join(site, "partials", `${name}.html`), "utf8");
    partials.set(name, text.trimEnd().split("\n"));
  }
  return partials.get(name);
}

const problems = [];

// ---------------------------------------------------------------- tokens

// egui's Color32 constructors as CSS colors. Premultiplied colors in the
// theme are gray (black or white at some alpha), so un-premultiplying is exact.
function cssColor(ctor, args) {
  const [r, g, b, a = 255] = args.split(",").map((n) => Number(n.trim()));
  if (ctor === "from_rgb") return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
  const un = (n) => Math.round((n * 255) / a);
  const pct = Math.round((a / 255) * 1000) / 10;
  return `rgb(${un(r)} ${un(g)} ${un(b)} / ${pct}%)`;
}

const theme = await readFile(themeRs, "utf8");
const constant = (name) => {
  const m = theme.match(new RegExp(`const ${name}: Color32 = Color32::(\\w+)\\(([^)]*)\\)`));
  if (!m) throw new Error(`theme.rs: no ${name} color`);
  return cssColor(m[1], m[2]);
};
const scheme = (name) => {
  const m = theme.match(new RegExp(`const ${name}: Palette = Palette \\{([\\s\\S]*?)\\n\\};`));
  if (!m) throw new Error(`theme.rs: no ${name} palette`);
  const out = {};
  for (const c of m[1].matchAll(/(\w+): Color32::(\w+)\(([^)]*)\)/g)) out[c[1]] = cssColor(c[2], c[3]);
  return out;
};
// theme.rs field → CSS token.
const names = [
  ["background", "--surface"],
  ["text", "--text"],
  ["weak", "--weak"],
  ["selection", "--sel"],
  ["hover", "--hover"],
  ["control", "--control"],
  ["divider", "--divider"],
];
const decls = (p, indent) =>
  names
    .map(([from, to]) => {
      if (!p[from]) throw new Error(`theme.rs: palette has no ${from}`);
      return `${indent}${to}: ${p[from]};`;
    })
    .join("\n");
const [light, dark] = [scheme("LIGHT"), scheme("DARK")];
const tokens = `/* Generated from src/app/theme.rs by scripts/sync.mjs (\`task web:sync\`).
   Do not edit: change the app's palette and run the sync. */

:root {
  --accent: ${constant("ACCENT")};
  --warning: ${constant("WARNING")};
${decls(light, "  ")}
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
${decls(dark, "    ")}
  }
}

:root[data-theme="dark"] {
${decls(dark, "  ")}
}
`;
const tokensPath = join(pub, "tokens.css");
const oldTokens = await readFile(tokensPath, "utf8").catch(() => "");
if (oldTokens !== tokens) {
  if (check) problems.push("public/tokens.css differs from src/app/theme.rs (run `task web:sync`)");
  else {
    await writeFile(tokensPath, tokens);
    console.log("wrote public/tokens.css");
  }
}

// ---------------------------------------------------------------- pages

const sitemap = await readFile(join(pub, "sitemap.xml"), "utf8");
const llms = await readFile(join(pub, "llms.txt"), "utf8");
// Every page is linked from home in plain HTML, so crawlers find it.
const home = await readFile(join(pub, "index.html"), "utf8");

for (const path of await pages(pub)) {
  const name = relative(site, path);
  const before = await readFile(path, "utf8");
  for (const m of before.matchAll(marker)) await partial(m[2]);
  const after = before.replace(marker, (_, indent, n) => {
    const body = partials.get(n).map((line) => (line ? indent + line : line));
    return [`${indent}<!-- partial:${n} -->`, ...body, `${indent}<!-- /partial:${n} -->`].join("\n");
  });

  if (after !== before) {
    if (check) problems.push(`${name}: shared chrome differs from partials/ (run \`task web:sync\`)`);
    else {
      await writeFile(path, after);
      console.log(`stamped ${name}`);
    }
  }

  if (/<meta name="robots" content="noindex/.test(before)) continue;
  // Every indexable page carries all of the shared chrome.
  for (const n of ["head", "topbar", "footer"])
    if (!before.includes(`<!-- partial:${n} -->`)) problems.push(`${name}: no partial:${n} markers`);
  const canonical = before.match(/<link rel="canonical" href="([^"]+)"/)?.[1];
  if (!canonical) {
    problems.push(`${name}: no canonical link`);
    continue;
  }
  if (!sitemap.includes(`<loc>${canonical}</loc>`))
    problems.push(`${name}: ${canonical} is not in sitemap.xml`);
  if (!llms.includes(`(${canonical})`)) problems.push(`${name}: ${canonical} is not linked from llms.txt`);
  // Its own social card, described, and shared to every network.
  const image = before.match(/<meta\s+property="og:image"\s+content="([^"]+)"/)?.[1];
  const card = image && new URL(image).pathname.slice(1);
  if (!card) problems.push(`${name}: no og:image`);
  else if (!(await stat(join(pub, card)).catch(() => null)))
    problems.push(`${name}: ${card} is missing (run \`task web:render -- pages\`)`);
  for (const tag of ["og:image:alt", "twitter:title", "twitter:image"])
    if (!new RegExp(`<meta\\s+(?:property|name)="${tag}"`).test(before)) problems.push(`${name}: no ${tag}`);
  const url = new URL(canonical).pathname;
  if (url !== "/" && !home.includes(`href="${url}"`))
    problems.push(`${name}: ${url} is not linked from the home page`);
}

for (const p of problems) console.error(p);
process.exit(problems.length ? 1 : 0);
