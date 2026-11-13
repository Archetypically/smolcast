// Applies the saved theme before first paint, so a returning visitor whose
// saved theme differs from the system never sees the wrong scheme flash.
// Loaded synchronously from partials/head.html because the site's CSP
// forbids inline scripts; app.js setTheme() keeps the saved value and the
// color-scheme meta in sync when the toggle is used.
try {
  const saved = localStorage.getItem("theme");
  if (saved === "dark" || saved === "light") {
    document.documentElement.dataset.theme = saved;
    const scheme = document.querySelector('meta[name="color-scheme"]');
    if (scheme) scheme.content = saved;
  }
} catch {
  /* storage may be unavailable */
}
