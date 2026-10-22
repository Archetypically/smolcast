// The demo film's music: an original piece synthesized here, so it carries
// no license but the project's own. Calm and small: 80 BPM in C, a
// descending Fmaj7 – Em7 – Dm7 – Cmaj7, a soft pad, an electric-piano
// arpeggio, a round bass and a little room. Deterministic. With `loop`, the
// length must be whole phrases (PHRASE): nothing fades, and what rings past
// the end is folded onto the start, so the piece plays into itself.
// Usage: node scripts/music.mjs <seconds> <out.wav>
import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const RATE = 48000;
const BPM = 80;
const BEAT = 60 / BPM;
const BAR = 4 * BEAT;
export const PHRASE = 4 * BAR;
// How long the last notes and the room ring on.
const TAIL = 4;

// MIDI notes: root (bass) and the chord tones (pad and arpeggio).
const CHORDS = [
  { root: 41, tones: [57, 60, 64, 65] }, // Fmaj7: A C E F
  { root: 40, tones: [55, 59, 62, 64] }, // Em7: G B D E
  { root: 38, tones: [57, 60, 62, 65] }, // Dm7: A C D F
  { root: 36, tones: [55, 59, 60, 64] }, // Cmaj7: G B C E
];
// Arpeggio pattern over eighth notes: indexes into the chord tones, an
// octave up; -1 rests.
const ARP = [0, 2, 1, 3, 2, -1, 1, 3];

const hz = (midi) => 440 * 2 ** ((midi - 69) / 12);

export function compose(seconds, { loop = false } = {}) {
  // A tiny deterministic random source for velocities, seeded per call.
  let seed = 7;
  const rand = () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;

  const length = Math.round(seconds * RATE);
  const n = loop ? length + TAIL * RATE : length;
  const left = new Float32Array(n);
  const right = new Float32Array(n);
  const add = (buf, i, v) => {
    if (i >= 0 && i < n) buf[i] += v;
  };

  const bars = loop ? Math.round(seconds / BAR) : Math.ceil(seconds / BAR) + 1;
  for (let b = 0; b < bars; b++) {
    const chord = CHORDS[b % CHORDS.length];
    const start = b * BAR;

    // Pad: detuned sines with a slow swell, overlapping the next bar.
    for (const tone of chord.tones) {
      for (const detune of [-0.06, 0.06]) {
        const f = hz(tone) * 2 ** (detune / 12);
        const len = BAR + 1.2;
        const s0 = Math.floor(start * RATE);
        for (let k = 0; k < len * RATE; k++) {
          const t = k / RATE;
          const env = Math.min(1, t / 0.9) * Math.min(1, (len - t) / 1.2);
          const v = 0.018 * env * (Math.sin(2 * Math.PI * f * t) + 0.18 * Math.sin(4 * Math.PI * f * t));
          add(left, s0 + k, v * (detune < 0 ? 1 : 0.7));
          add(right, s0 + k, v * (detune < 0 ? 0.7 : 1));
        }
      }
    }

    // Bass: the root, held, with a soft attack.
    {
      const f = hz(chord.root);
      const len = BAR * 0.95;
      const s0 = Math.floor(start * RATE);
      for (let k = 0; k < len * RATE; k++) {
        const t = k / RATE;
        const env = Math.min(1, t / 0.04) * Math.exp(-t / 2.4) * Math.min(1, (len - t) / 0.3);
        const v = 0.11 * env * (Math.sin(2 * Math.PI * f * t) + 0.25 * Math.sin(4 * Math.PI * f * t));
        add(left, s0 + k, v);
        add(right, s0 + k, v);
      }
    }

    // Electric piano: a sine with bell partials that fade faster.
    ARP.forEach((step, e) => {
      if (step < 0) return;
      const f = hz(chord.tones[step] + 12);
      const at = start + (e * BEAT) / 2 + (e % 2 ? 0.012 : 0);
      const velocity = 0.05 + 0.025 * rand();
      const pan = e % 2 ? 0.35 : -0.35;
      const len = 1.6;
      const s0 = Math.floor(at * RATE);
      for (let k = 0; k < len * RATE; k++) {
        const t = k / RATE;
        const env = Math.min(1, t / 0.004) * Math.exp(-t / 0.42);
        const v =
          velocity *
          env *
          (Math.sin(2 * Math.PI * f * t) +
            0.3 * Math.exp(-t / 0.12) * Math.sin(2 * Math.PI * f * 2 * t) +
            0.12 * Math.exp(-t / 0.05) * Math.sin(2 * Math.PI * f * 3.01 * t));
        add(left, s0 + k, v * (1 - pan) * 0.7);
        add(right, s0 + k, v * (1 + pan) * 0.7);
      }
    });
  }

  room(left, [1557, 1617, 1491, 1422], [225, 556]);
  room(right, [1601, 1557 + 23, 1491 + 31, 1356], [241, 579]);

  // A loop folds its tail onto its start; otherwise fade in and out.
  if (loop) {
    for (let i = length; i < n; i++) {
      left[i - length] += left[i];
      right[i - length] += right[i];
    }
  } else {
    const fadeIn = 1.2 * RATE;
    const fadeOut = 2.5 * RATE;
    for (let i = 0; i < n; i++) {
      const g = Math.min(1, i / fadeIn, (n - i) / fadeOut);
      left[i] *= g;
      right[i] *= g;
    }
  }
  // Leave headroom.
  let peak = 0;
  for (let i = 0; i < length; i++) peak = Math.max(peak, Math.abs(left[i]), Math.abs(right[i]));
  const gain = 0.7 / peak;
  for (let i = 0; i < length; i++) {
    left[i] *= gain;
    right[i] *= gain;
  }
  return { left: left.subarray(0, length), right: right.subarray(0, length) };
}

// A small Schroeder room: parallel combs into series all-passes, mixed low.
function room(buf, combs, allpasses) {
  const wet = new Float32Array(buf.length);
  for (const delay of combs) {
    const line = new Float32Array(delay);
    let p = 0;
    for (let i = 0; i < buf.length; i++) {
      const out = line[p];
      line[p] = buf[i] + out * 0.78;
      wet[i] += out / combs.length;
      p = (p + 1) % delay;
    }
  }
  for (const delay of allpasses) {
    const line = new Float32Array(delay);
    let p = 0;
    for (let i = 0; i < wet.length; i++) {
      const delayed = line[p];
      const input = wet[i];
      line[p] = input + delayed * 0.5;
      wet[i] = delayed - input * 0.5;
      p = (p + 1) % delay;
    }
  }
  for (let i = 0; i < buf.length; i++) buf[i] += wet[i] * 0.22;
}

export function wav({ left, right }) {
  const n = left.length;
  const data = Buffer.alloc(44 + n * 4);
  data.write("RIFF", 0);
  data.writeUInt32LE(36 + n * 4, 4);
  data.write("WAVEfmt ", 8);
  data.writeUInt32LE(16, 16);
  data.writeUInt16LE(1, 20);
  data.writeUInt16LE(2, 22);
  data.writeUInt32LE(RATE, 24);
  data.writeUInt32LE(RATE * 4, 28);
  data.writeUInt16LE(4, 32);
  data.writeUInt16LE(16, 34);
  data.write("data", 36);
  data.writeUInt32LE(n * 4, 40);
  // Clamp to [-1, 1] and scale to 16 bits.
  const pcm = (v) => Math.round(Math.max(-1, Math.min(1, v)) * 32767);
  for (let i = 0; i < n; i++) {
    data.writeInt16LE(pcm(left[i]), 44 + i * 4);
    data.writeInt16LE(pcm(right[i]), 46 + i * 4);
  }
  return data;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [seconds, out] = process.argv.slice(2);
  await writeFile(out, wav(compose(Number(seconds))));
  console.log(`wrote ${out}`);
}
