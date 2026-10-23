// Keeps what the site shares in one place without a build step, so public/
// stays deployable as-is:
//
// - Page chrome: every page carries its own copy of the head links, top bar
//   and footer between `<!-- partial:NAME -->` and `<!-- /partial:NAME -->`
//   markers; partials/NAME.html is stamped into each one. Partials are
//   written as Prettier formats them at a page's four-space indent.
// - Colors: public/tokens.css is generated from the app's own palette in
//   src/app/theme.rs, so the site, the film and the app change together.
// - Markdown: every indexable page has a twin, public/NAME.md, written from
//   its <main> (scripts/markdown.mjs) and named by a
//   `<link rel="alternate" type="text/markdown">` after its canonical link.
//   _headers serves the twins as markdown that names the page as canonical.
//
// With --check it changes nothing and fails on any drift, and on an
// indexable page or twin missing from sitemap.xml, a page missing from
// llms.txt or home's links, or one missing its social card and the tags that
// describe it.
import { readFile, readdir, stat, writeFile } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { markdownPath, pageMarkdown } from "./markdown.mjs";

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

/** Write `text` to `path` unless it is there already; in --check, report `stale`. */
async function keep(path, text, stale) {
  const old = await readFile(path, "utf8").catch(() => null);
  if (old === text) return;
  const name = relative(site, path);
  if (check) problems.push(stale ?? `${name}: out of date (run \`task web:sync\`)`);
  else {
    await writeFile(path, text);
    console.log(`wrote ${name}`);
  }
}

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
await keep(
  join(pub, "tokens.css"),
  tokens,
  "public/tokens.css differs from src/app/theme.rs (run `task web:sync`)",
);

// ---------------------------------------------------------------- pages

const sitemap = await readFile(join(pub, "sitemap.xml"), "utf8");
const llms = await readFile(join(pub, "llms.txt"), "utf8");
// Every page is linked from home in plain HTML, so crawlers find it.
const home = await readFile(join(pub, "index.html"), "utf8");

// Each twin's canonical page, for the _headers block.
const twins = [];

for (const path of await pages(pub)) {
  const name = relative(site, path);
  const before = await readFile(path, "utf8");
  for (const m of before.matchAll(marker)) await partial(m[2]);
  let after = before.replace(marker, (_, indent, n) => {
    const body = partials.get(n).map((line) => (line ? indent + line : line));
    return [`${indent}<!-- partial:${n} -->`, ...body, `${indent}<!-- /partial:${n} -->`].join("\n");
  });
  const indexable = !/<meta name="robots" content="noindex/.test(before);
  const canonical = before.match(/<link rel="canonical" href="([^"]+)"/)?.[1];
  const twin = indexable && canonical && new URL(markdownPath(canonical), canonical).href;
  if (twin) {
    twins.push([twin, canonical]);
    // Matched across line breaks: Prettier wraps a long one.
    const alternate = /^[ \t]*<link\s+rel="alternate"\s+type="text\/markdown"\s+href="([^"]*)"\s*\/>\n/m;
    if (after.match(alternate)?.[1] !== twin)
      after = after
        .replace(alternate, "")
        .replace(
          /^([ \t]*)(<link rel="canonical"[^\n]*\n)/m,
          `$1$2$1<link rel="alternate" type="text/markdown" href="${twin}" />\n`,
        );
    await keep(path.replace(/\.html$/, ".md"), pageMarkdown(after, canonical));
  }

  if (after !== before) {
    if (check) problems.push(`${name}: shared chrome or markdown link out of date (run \`task web:sync\`)`);
    else {
      await writeFile(path, after);
      console.log(`stamped ${name}`);
    }
  }

  if (!indexable) continue;
  // Every indexable page carries all of the shared chrome.
  for (const n of ["head", "topbar", "footer"])
    if (!before.includes(`<!-- partial:${n} -->`)) problems.push(`${name}: no partial:${n} markers`);
  if (!canonical) {
    problems.push(`${name}: no canonical link`);
    continue;
  }
  for (const loc of [canonical, twin])
    if (!sitemap.includes(`<loc>${loc}</loc>`)) problems.push(`${name}: ${loc} is not in sitemap.xml`);
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

// A twin whose page is gone.
const twinFiles = new Set(twins.map(([twin]) => join(pub, new URL(twin).pathname)));
for (const path of (await readdir(pub, { recursive: true })).map((p) => join(pub, p)))
  if (path.endsWith(".md") && !twinFiles.has(path))
    problems.push(`${relative(site, path)}: no page of its own`);

// _headers: twins are markdown, and each names its page as canonical so
// search engines index the page, not the twin.
const headersPath = join(pub, "_headers");
const headers = await readFile(headersPath, "utf8");
const block = [
  "# markdown:start (written by scripts/sync.mjs)",
  "/*.md",
  "  Content-Type: text/markdown; charset=utf-8",
  ...twins.flatMap(([twin, canonical]) => [
    "",
    new URL(twin).pathname,
    `  Link: <${canonical}>; rel="canonical"`,
  ]),
  "# markdown:end",
].join("\n");
const blockPattern = /# markdown:start[\s\S]*?# markdown:end/;
await keep(
  headersPath,
  blockPattern.test(headers) ? headers.replace(blockPattern, block) : `${headers.trimEnd()}\n\n${block}\n`,
);

for (const p of problems) console.error(p);
process.exit(problems.length ? 1 : 0);
