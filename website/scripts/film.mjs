// Records the demo film from media/film.html, a simulation of smolcast drawn
// to the app's own measurements and copy (see media/film.js), and writes
// public/media/smolcast-demo.{mp4,webm,jpg} plus a square cut for social
// posts in media/out/. Music is original (scripts/music.mjs).
//
// Chrome's screencast delivers about 30 frames a second and ignores the
// device scale factor, so the page runs 4× slower while it is recorded
// (timers via media/slowmo.js, animations via CDP) and is zoomed to 1080p
// with CSS; the frames are re-timed to real speed and encoded at 60 fps.
// Usage: node scripts/film.mjs   (needs ffmpeg)
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { compose, wav } from "./music.mjs";
import { serve } from "./serve.mjs";

const SLOW = 4;
const WIDTH = 1920;
const HEIGHT = 1080;
const ZOOM = 1.5;
const POSTER_AT = 4.6;

const site = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
const work = await mkdtemp(join(tmpdir(), "smolcast-film-"));
const server = await serve();
const browser = await chromium.launch();

try {
  const page = await browser.newPage({ viewport: { width: WIDTH, height: HEIGHT } });
  await page.goto(`http://127.0.0.1:${server.address().port}/_media/film.html?zoom=${ZOOM}&slow=${SLOW}`);
  await page.evaluate(() => window.ready);
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Animation.enable");
  await cdp.send("Animation.setPlaybackRate", { playbackRate: 1 / SLOW });

  const frames = [];
  cdp.on("Page.screencastFrame", async ({ data, metadata, sessionId }) => {
    frames.push({ data, t: metadata.timestamp, wall: Date.now() / 1000 });
    await cdp.send("Page.screencastFrameAck", { sessionId }).catch(() => {});
  });
  await cdp.send("Page.startScreencast", { format: "jpeg", quality: 92, maxWidth: WIDTH, maxHeight: HEIGHT });
  await page.evaluate(() => window.play());
  await cdp.send("Page.stopScreencast");
  const ended = Date.now() / 1000;

  // Each frame lasts until the next, at real speed; the last holds to the end.
  const list = [];
  let seconds = 0;
  for (const [i, frame] of frames.entries()) {
    const file = join(work, `${String(i).padStart(5, "0")}.jpg`);
    await writeFile(file, Buffer.from(frame.data, "base64"));
    const span = Math.max(
      0.001,
      (i + 1 < frames.length ? frames[i + 1].t - frame.t : ended - frame.wall) / SLOW,
    );
    seconds += span;
    list.push(`file '${file}'`, `duration ${span.toFixed(5)}`);
  }
  list.push(list[list.length - 2]);
  await writeFile(join(work, "frames.txt"), list.join("\n"));
  await writeFile(join(work, "music.wav"), wav(compose(seconds)));
  console.log(`${frames.length} frames, ${seconds.toFixed(1)} s`);

  await mkdir(site("public/media"), { recursive: true });
  await mkdir(site("media/out"), { recursive: true });
  const input = ["-f", "concat", "-safe", "0", "-i", join(work, "frames.txt"), "-i", join(work, "music.wav")];
  const audio = ["-af", "loudnorm=I=-16:TP=-1.5:LRA=7", "-c:a", "aac", "-b:a", "160k", "-shortest"];
  const h264 = ["-c:v", "libx264", "-preset", "slow", "-crf", "18", "-movflags", "+faststart"];
  const mp4 = site("public/media/smolcast-demo.mp4");
  await ffmpeg([...input, "-vf", "fps=60,format=yuv420p", ...h264, ...audio, mp4]);
  // The three copies all read the MP4 and not each other, so they encode at once.
  await Promise.all([
    ffmpeg([
      "-i",
      mp4,
      ...["-c:v", "libvpx-vp9", "-crf", "34", "-b:v", "0", "-row-mt", "1"],
      ...["-c:a", "libopus", "-b:a", "128k"],
      site("public/media/smolcast-demo.webm"),
    ]),
    ffmpeg([
      "-ss",
      String(POSTER_AT),
      "-i",
      mp4,
      "-frames:v",
      "1",
      "-q:v",
      "3",
      site("public/media/smolcast-demo.jpg"),
    ]),
    // Square: the middle of the frame, where the palette is.
    ffmpeg([
      "-i",
      mp4,
      "-vf",
      `crop=${HEIGHT}:${HEIGHT}`,
      ...h264,
      "-c:a",
      "copy",
      site("media/out/smolcast-demo-square.mp4"),
    ]),
  ]);
  console.log("wrote public/media/smolcast-demo.{mp4,webm,jpg} and media/out/smolcast-demo-square.mp4");
} finally {
  await browser.close();
  server.close();
  await rm(work, { recursive: true });
}

function ffmpeg(args) {
  return new Promise((done, fail) => {
    const child = spawn("ffmpeg", ["-y", "-loglevel", "error", ...args], { stdio: "inherit" });
    child.on("error", () => fail(new Error("ffmpeg failed")));
    child.on("close", (code) => (code === 0 ? done() : fail(new Error("ffmpeg failed"))));
  });
}
