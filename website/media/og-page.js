// Builds a page's social card from the page itself (media/og-page.html).
"use strict";

// Matrix rows worth a glance, in the order the card shows them.
const ROWS = ["Account", "Clipboard history", "Snippets", "Emoji picker", "Extensions", "AI", "Download"];

async function build() {
  const page = new URLSearchParams(location.search).get("page");
  const html = await (await fetch(`/${page}`)).text();
  const doc = new DOMParser().parseFromString(html, "text/html");
  document.getElementById("title").textContent = doc.querySelector("h1").textContent.trim();
  document.getElementById("lead").textContent = doc.querySelector(".doc-hero .lead").textContent.trim();
  document.getElementById("url").textContent =
    new URL(doc.querySelector('link[rel="canonical"]').href).host + `/${page}`;
  const facts = document.getElementById("facts");

  const table = doc.querySelector("table.compare");
  if (table) {
    const small = document.createElement("table");
    small.className = "compare";
    small.append(table.tHead.cloneNode(true));
    const body = small.createTBody();
    for (const name of ROWS) {
      const row = [...table.tBodies[0].rows].find((r) => r.cells[0].textContent.trim() === name);
      if (!row) continue;
      const copy = row.cloneNode(true);
      for (const note of copy.querySelectorAll(".note, .sr-only")) note.remove();
      body.append(copy);
    }
    small.classList.toggle("wide", table.tHead.rows[0].cells.length > 3);
    facts.append(small);
  } else {
    const list = doc.querySelector(".facts");
    if (list) facts.append(list.cloneNode(true));
  }
  document.body.dataset.ready = "1";
}

build();
