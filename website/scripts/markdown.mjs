// The markdown twin of a page (public/NAME.md beside public/NAME.html), for
// agents that read markdown: the page's <main> without the working palette,
// icons and controls, with site links made absolute. sync.mjs writes and
// checks the twins like the shared chrome.
import TurndownService from "turndown";
import { tables } from "turndown-plugin-gfm";

const turndown = new TurndownService({
  headingStyle: "atx",
  bulletListMarker: "-",
  codeBlockStyle: "fenced",
  emDelimiter: "*",
});
turndown.use(tables);

// What a reader without a browser has no use for: the working palette and
// its prompts, icons, buttons, breadcrumbs, and anything hidden from
// assistive technology.
const dropped = new Set(["SCRIPT", "TEMPLATE", "BUTTON", "VIDEO", "NOSCRIPT", "svg", "SVG"]);
const droppedClasses = ["stage", "try", "crumbs"];
// Added rules run before turndown's own, so this one wins over the paragraph
// and list rules (`remove` would only run when no rule matched).
turndown.addRule("dropped", {
  filter: (node) => {
    if (dropped.has(node.nodeName)) return true;
    if (node.getAttribute("aria-hidden") === "true" || node.hasAttribute("hidden")) return true;
    return droppedClasses.some((c) => node.classList.contains(c));
  },
  replacement: () => "",
});

// A modifier cap joins the next cap as a chord (`Ctrl`+`Alt`+`Space`);
// other adjacent caps are separate keys (`↑` `↓`).
const modifiers = new Set(["Ctrl", "Alt", "Shift", "Win"]);
turndown.addRule("keys", {
  filter: "kbd",
  replacement: (content, node) => {
    const joiner = modifiers.has(content) ? "+" : " ";
    return `\`${content}\`${node.nextSibling?.nodeName === "KBD" ? joiner : ""}`;
  },
});

// A stat's figure leads its line: **7 MB** One executable.
turndown.addRule("figures", {
  filter: (node) => node.classList.contains("figure"),
  replacement: (content) => `**${content}** `,
});

// A two-part list row (a file and what it holds, a role and who has it)
// reads "first: second".
turndown.addRule("pairs", {
  filter: (node) =>
    node.parentNode?.nodeName === "LI" &&
    node.parentNode.parentNode.classList.contains("files") &&
    node === node.parentNode.firstElementChild,
  replacement: (content, node) => (node.nodeName === "CODE" ? `\`${content}\`: ` : `${content}: `),
});

// Each part of a page's stamp line ends as a sentence.
turndown.addRule("stamp", {
  filter: (node) => node.nodeName === "SPAN" && node.parentNode.classList.contains("stamp"),
  replacement: (content) => `${content.replace(/[.\s]+$/, "")}.`,
});

// A table cell's note follows its value in parentheses.
turndown.addRule("notes", {
  filter: (node) => node.classList.contains("note"),
  replacement: (content) => ` (${content})`,
});

// Images on these pages are decoration; only alt text stays.
turndown.addRule("images", {
  filter: "img",
  replacement: (_, node) => node.getAttribute("alt") ?? "",
});

/** The markdown path for a page's canonical URL: `/` is `/index.md`. */
export function markdownPath(canonical) {
  const path = new URL(canonical).pathname;
  return path.endsWith("/") ? `${path}index.md` : `${path}.md`;
}

/** The markdown for a page's `html`, whose canonical URL is `canonical`. */
export function pageMarkdown(html, canonical) {
  const main = html.match(/<main[^>]*>([\s\S]*)<\/main>/)?.[1];
  if (!main) throw new Error(`${canonical}: no <main>`);
  const absolute = main
    .replace(/(href|src)="([/#][^"]*)"/g, (_, attr, path) => `${attr}="${new URL(path, canonical)}"`)
    // Two links side by side are laid out apart; in text they need a mark.
    .replace(/<\/a\s*>\s*<a\s/g, "</a> · <a ")
    // A table's group heading spans its row; markdown tables have no spans.
    .replace(
      /<th scope="rowgroup" colspan="(\d+)">([\s\S]*?)<\/th>/g,
      (_, span, text) => `<th scope="rowgroup"><b>${text}</b></th>${"<td></td>".repeat(Number(span) - 1)}`,
    );
  const body = turndown
    .turndown(absolute)
    .replace(/\u00a0/g, " ")
    .replace(/^(\s*)- {3}/gm, "$1- ")
    .replace(/[ \t]+$/gm, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  return `${body}\n\n---\n\nThe markdown version of ${canonical}\n`;
}
