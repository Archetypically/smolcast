// Slow motion for recording: with ?slow=N, every timer waits N times
// longer. The recorder slows CSS and Web Animations by the same factor over
// CDP and re-times the frames, so the film plays at real speed but is
// captured at N times the frame rate. Loaded before the page's scripts.
"use strict";

{
  const slow = Number(new URLSearchParams(location.search).get("slow")) || 1;
  if (slow !== 1) {
    const setTimeoutReal = window.setTimeout.bind(window);
    window.setTimeout = (fn, ms = 0, ...args) => setTimeoutReal(fn, ms * slow, ...args);
  }
}
