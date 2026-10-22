// Renders the site's generated images from HTML and SVG sources:
//   public/apple-touch-icon.png, icon-192.png, icon-512.png  (from favicon.svg)
//   public/og.png                                           (from media/og.html)
// Usage: node scripts/render.mjs [icons|og ...]   (both when none given)
import { chromium } from "playwright";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { serve } from "./serve.mjs";

const pub = (name) => fileURLToPath(new URL(`../public/${name}`, import.meta.url));
const wanted = new Set(process.argv.slice(2));
const want = (name) => !wanted.size || wanted.has(name);

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
    const page = await browser.newPage({
      viewport: { width: 1200, height: 630 },
      deviceScaleFactor: 2,
      colorScheme: "dark",
      reducedMotion: "reduce",
    });
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
} finally {
  await browser.close();
  server.close();
}
