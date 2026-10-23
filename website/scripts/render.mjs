// Renders the site's generated images from HTML and SVG sources:
//   public/apple-touch-icon.png, icon-192.png, icon-512.png  (from favicon.svg)
//   public/og.png                                           (from media/og.html)
//   public/og/<page>.png, one per page in sitemap.xml but home
//                                                           (from media/og-page.html)
// Usage: node scripts/render.mjs [icons|og|pages ...]   (all when none given)
// The demo film is recorded separately: see scripts/film.mjs.
import { chromium } from "playwright";
import { mkdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { serve } from "./serve.mjs";

const pub = (name) => fileURLToPath(new URL(`../public/${name}`, import.meta.url));
const wanted = new Set(process.argv.slice(2));
const want = (name) => !wanted.size || wanted.has(name);
// The social cards: 1200 x 630 at 2x, in the dark theme, without motion.
const cardOptions = {
  viewport: { width: 1200, height: 630 },
  deviceScaleFactor: 2,
  colorScheme: "dark",
  reducedMotion: "reduce",
};

const server = await serve();
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch();

try {
  if (want("icons")) {
    const svg = await readFile(pub("favicon.svg"), "utf8");
    // Apple pads and masks touch icons itself, so that one sits on the
    // tile's own dark color instead of transparency.
    for (const [file, size, bg] of [
      ["apple-touch-icon.png", 180, "#1c1c1e"],
      ["icon-192.png", 192, null],
      ["icon-512.png", 512, null],
    ]) {
      const page = await browser.newPage({ viewport: { width: size, height: size } });
      await page.setContent(
        `<style>html,body{margin:0;background:${bg ?? "transparent"}}svg{display:block;width:${size}px;height:${size}px}</style>${svg}`,
      );
      await page.screenshot({ path: pub(file), omitBackground: !bg });
      await page.close();
      console.log(`wrote public/${file}`);
    }
  }

  if (want("og")) {
    const page = await browser.newPage(cardOptions);
    await page.goto(`${base}/_media/og.html`);
    await page.evaluate(() => {
      window.smolcast.setQuery("40cm to in");
      window.smolcast.input.blur();
    });
    await page.waitForTimeout(300);
    await page.screenshot({ path: pub("og.png") });
    await page.close();
    console.log("wrote public/og.png");
  }
  if (want("pages")) {
    await mkdir(pub("og"), { recursive: true });
    const sitemap = await readFile(pub("sitemap.xml"), "utf8");
    for (const [, loc] of sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)) {
      const path = new URL(loc).pathname.replace(/^\/|\/$/g, "");
      if (!path) continue;
      const file = `og/${path.replaceAll("/", "-")}.png`;
      const page = await browser.newPage(cardOptions);
      await page.goto(`${base}/_media/og-page.html?page=${path}${loc.endsWith("/") ? "/" : ""}`);
      await page.waitForSelector("body[data-ready]");
      await page.screenshot({ path: pub(file) });
      await page.close();
      console.log(`wrote public/${file}`);
    }
  }
} finally {
  await browser.close();
  server.close();
}
