// A small static server for local previews and rendering: public/ at /,
// and the render-only pages in media/ at /_media/. Cloudflare Pages serves
// public/ the same way (index.html for a folder, page.html for /page,
// 404.html for misses).
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const site = resolve(fileURLToPath(new URL("..", import.meta.url)));
const roots = [
  ["/_media/", join(site, "media")],
  ["/", join(site, "public")],
];
const types = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".ico": "image/x-icon",
  ".txt": "text/plain; charset=utf-8",
  ".md": "text/markdown; charset=utf-8",
  ".xml": "application/xml",
  ".webmanifest": "application/manifest+json",
  ".mp4": "video/mp4",
  ".webm": "video/webm",
};

async function resolveFile(pathname) {
  for (const [prefix, dir] of roots) {
    if (!pathname.startsWith(prefix)) continue;
    let file;
    try {
      file = join(dir, decodeURIComponent(pathname.slice(prefix.length)));
    } catch {
      return null;
    }
    if (file !== dir && !file.startsWith(dir + sep)) return null;
    try {
      if ((await stat(file)).isDirectory()) {
        file = join(file, "index.html");
        await stat(file);
      }
      return file;
    } catch {
      // Pretty URLs: /privacy is privacy.html.
      try {
        await stat(`${file}.html`);
        return `${file}.html`;
      } catch {
        return null;
      }
    }
  }
  return null;
}

export function serve(port = 0) {
  const server = createServer(async (req, res) => {
    let file = null;
    try {
      file = await resolveFile(new URL(req.url, "http://localhost").pathname);
    } catch {
      // A malformed request path ("//") is a miss, not a crash.
    }
    const path = file ?? join(site, "public", "404.html");
    const body = await readFile(path);
    res.writeHead(file ? 200 : 404, {
      "Content-Type": types[extname(path)] ?? "application/octet-stream",
    });
    res.end(body);
  });
  return new Promise((done) => server.listen(port, "127.0.0.1", () => done(server)));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const port = Number(process.env.PORT ?? 8788);
  await serve(port);
  console.log(`smolcast site on http://127.0.0.1:${port}`);
}
