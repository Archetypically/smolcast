// Records the demo film from media/film.html, a simulation of smolcast drawn
// to the app's own measurements and copy (see media/film.js), in three
// frames: wide for the page (public/media/smolcast-demo.{mp4,webm,jpg}), and
// square and vertical cuts for social posts in media/out/. Each starts and
// ends on the same title card, and its length is snapped to whole phrases of
// the original music (scripts/music.mjs), so picture and sound loop
// seamlessly.
//
// Chrome's screencast delivers about 30 frames a second and ignores the
// device scale factor, so the page runs 4× slower while it is recorded
// (timers via media/slowmo.js, animations via CDP) and is zoomed to 1080p
// with CSS; the frames are re-timed to real speed and encoded at 60 fps.
// Usage: node scripts/film.mjs [wide] [square] [vertical]   (needs ffmpeg;
// all three by default)
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { PHRASE, compose, wav } from "./music.mjs";
import { serve } from "./serve.mjs";

const SLOW = 4;
const ZOOM = 1.5;
const FORMATS = {
  wide: { width: 1920, height: 1080 },
  square: { width: 1080, height: 1080 },
  vertical: { width: 1080, height: 1920 },
};

const site = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
const picked = process.argv.slice(2);
for (const name of picked) if (!FORMATS[name]) throw new Error(`unknown format ${name}`);
const server = await serve();
const browser = await chromium.launch();

try {
  await mkdir(site("public/media"), { recursive: true });
  await mkdir(site("media/out"), { recursive: true });
  for (const name of picked.length ? picked : Object.keys(FORMATS)) {
    await record(name, FORMATS[name]);
  }
} finally {
  await browser.close();
  server.close();
}

async function record(name, { width, height }) {
  const work = await mkdtemp(join(tmpdir(), "smolcast-film-"));
  try {
    const page = await browser.newPage({ viewport: { width, height } });
    const marks = {};
    await page.exposeFunction("filmMark", (mark) => (marks[mark] = Date.now() / 1000));
    const query = `zoom=${ZOOM}&slow=${SLOW}&phrase=${PHRASE}&format=${name}`;
    await page.goto(`http://127.0.0.1:${server.address().port}/_media/film.html?${query}`);
    await page.evaluate(() => window.ready);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Animation.enable");
    await cdp.send("Animation.setPlaybackRate", { playbackRate: 1 / SLOW });

    const frames = [];
    cdp.on("Page.screencastFrame", async ({ data, metadata, sessionId }) => {
      frames.push({ data, t: metadata.timestamp, wall: Date.now() / 1000 });
      await cdp.send("Page.screencastFrameAck", { sessionId }).catch(() => {});
    });
    await cdp.send("Page.startScreencast", {
      format: "jpeg",
      quality: 92,
      maxWidth: width,
      maxHeight: height,
    });
    await page.evaluate(() => window.play());
    await cdp.send("Page.stopScreencast");
    const ended = Date.now() / 1000;
    await page.close();

    // The film starts where the page started playing (on the frame then on
    // screen) and each frame lasts until the next, at real speed. The page
    // holds its last frame, the title card it opened on, until the film is
    // whole phrases long; the cut lands exactly there, dropping the frames
    // after it, so the drifting backdrop ends where it began.
    const first = frames.findLastIndex((frame) => frame.t <= marks.start);
    if (first > 0) frames.splice(0, first);
    frames[0].t = Math.max(frames[0].t, marks.start);
    const last = frames[frames.length - 1];
    const played = (last.t - frames[0].t + ended - last.wall) / SLOW;
    const seconds = Math.round(played / PHRASE) * PHRASE;
    const cut = frames[0].t + seconds * SLOW;
    const after = frames.findIndex((frame, i) => i > 0 && frame.t >= cut);
    if (after > 0) frames.splice(after);
    const spans = frames.map(
      (frame, i) => ((i + 1 < frames.length ? frames[i + 1].t : cut) - frame.t) / SLOW,
    );
    const list = [];
    for (const [i, frame] of frames.entries()) {
      const file = join(work, `${String(i).padStart(5, "0")}.jpg`);
      await writeFile(file, Buffer.from(frame.data, "base64"));
      list.push(`file '${file}'`, `duration ${spans[i].toFixed(5)}`);
    }
    list.push(list[list.length - 2]);
    await writeFile(join(work, "frames.txt"), list.join("\n"));
    await writeFile(join(work, "music.wav"), wav(compose(seconds, { loop: true })));
    console.log(`${name}: ${frames.length} frames, ${played.toFixed(2)} s played, cut to ${seconds} s`);

    const input = [
      "-f",
      "concat",
      "-safe",
      "0",
      "-i",
      join(work, "frames.txt"),
      "-i",
      join(work, "music.wav"),
    ];
    // A fixed gain, not loudnorm, whose gain rides would break the loop's
    // seam: the music is deterministic and measures -13.7 LUFS, so this
    // brings it to -16.
    const audio = ["-af", "volume=-2.3dB", "-c:a", "aac", "-b:a", "160k", "-shortest"];
    // The page's copy is lighter (the drifting stipple is costly to encode);
    // the social cuts keep more, since each service re-encodes them.
    const h264 = [
      "-c:v",
      "libx264",
      "-preset",
      "slow",
      "-crf",
      name === "wide" ? "23" : "18",
      "-movflags",
      "+faststart",
    ];
    const encode = (out) => ffmpeg([...input, "-vf", "fps=60,format=yuv420p", ...h264, ...audio, out]);
    if (name !== "wide") {
      const out = `media/out/smolcast-demo-${name}.mp4`;
      await encode(site(out));
      console.log(`wrote ${out}`);
      return;
    }

    const mp4 = site("public/media/smolcast-demo.mp4");
    await encode(mp4);
    // The poster is the conversion answered, with its caption.
    const poster = (marks.poster - frames[0].t) / SLOW + 1;
    // Both copies read the MP4 and not each other, so they encode at once.
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
        poster.toFixed(2),
        "-i",
        mp4,
        "-frames:v",
        "1",
        "-q:v",
        "3",
        site("public/media/smolcast-demo.jpg"),
      ]),
    ]);
    console.log("wrote public/media/smolcast-demo.{mp4,webm,jpg}");
  } finally {
    await rm(work, { recursive: true });
  }
}

function ffmpeg(args) {
  return new Promise((done, fail) => {
    const child = spawn("ffmpeg", ["-y", "-loglevel", "error", ...args], { stdio: "inherit" });
    child.on("error", () => fail(new Error("ffmpeg failed")));
    child.on("close", (code) => (code === 0 ? done() : fail(new Error("ffmpeg failed"))));
  });
}
