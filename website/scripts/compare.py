#!/usr/bin/env python3
"""Renders the comparison pages in public/compare/ (decision 033).

The facts live in compare.jsonnet, next to this file (decision 043): one
object per tool, each checked against the sources it lists on the date it
carries. This script only builds HTML from them. Run `task web:compare`
(this, then the partials stamp and Prettier) after changing a fact, and
`task web:render -- pages` if a headline, lead or table row on a card
changed. Rules for the copy and the bolding are in docs/DESIGN.md
(Website, Taste).

Needs the Jsonnet Python binding: pip install jsonnet.

Usage: python3 scripts/compare.py public
"""
import html, json, os, re, sys
import _jsonnet

OUT = sys.argv[1]
HERE = os.path.dirname(os.path.abspath(__file__))
DATA = json.loads(_jsonnet.evaluate_file(os.path.join(HERE, "compare.jsonnet")))

SITE = DATA["site"]
DATE = DATA["date"]; DATE_TEXT = DATA["date_text"]
Y = "yes"  # the only cell status the renderer itself tests for

def _cells(d): return {k: tuple(v) for k, v in d.items()}
def _pairs(xs): return [tuple(x) for x in xs]

ROWS = [(g, _pairs(rs)) for g, rs in DATA["rows"]]
HUBROWS = [(g, _pairs(rs)) for g, rs in DATA["hub_rows"]]
HUBMETA = DATA["hub"]
HUBUS = _cells(DATA["hub_us"])
PAGES = DATA["pages"]
for p in PAGES:
  p["us"] = _cells(p["us"]); p["them"] = _cells(p["them"])
  p["sources"] = _pairs(p["sources"]); p["maps"] = _pairs(p["maps"])
RELATED = DATA["related"]
OS = DATA["os"]
SMOL_SRC = tuple(DATA["smol_src"])

def esc_attr(s): return html.escape(s, quote=True)
ICON='<img src="/favicon.svg" alt="" width="24" height="24" />'

def cell(c, ours=False, os=None):
  st,label,note,w = c
  w = w and ours  # only smolcast's wins are bold
  if os: return f'<td data-os="{os}">{label}</td>'
  glyph = '<svg aria-hidden="true"><use href="/icons.svg#check" /></svg>' if st == Y else ""
  inner = f'<span class="status {st}">{glyph}{label}</span>' if st else label
  n = f'<span class="note">{note}</span>' if note else ""
  if w: return f'<td class="win"><strong>{inner}</strong>{n}<span class="sr-only"> (the better pick)</span></td>'
  return f"<td>{inner}{n}</td>"

def table(cols, rows=ROWS, wide=False):
  us = ' class="us"'
  head = '<tr><td></td>' + "".join(f'<th scope="col"{us if i == 0 else ""}>{n}</th>' for i,(n,_) in enumerate(cols)) + '</tr>'
  out=[f'        <table class="compare{" wide" if wide else ""}">', f'          <thead>{head}</thead>','          <tbody>']
  for group, rs in rows:
    out.append(f'            <tr class="group"><th scope="rowgroup" colspan="{len(cols)+1}">{group}</th></tr>')
    for key,label in rs:
      out.append(f'            <tr><th scope="row">{label}</th>' + "".join(cell(d[key], i == 0, OS[n] if key == "os" else None) for i,(n,d) in enumerate(cols)) + '</tr>')
  out += ['          </tbody>','        </table>']
  return "\n".join(out)

def tags(ts): return '<ul class="tags">' + "".join(f'<li class="tag">{t}</li>' for t in ts) + '</ul>'

def rows_html(current):
  items = [("/compare/", "Every alternative, side by side", "Alternatives")] if current != "hub" else []
  for p in PAGES:
    if p["slug"] != current: items.append((f"/compare/{p['slug']}", f"smolcast vs {p['full']}", "Alternative"))
  items.append(("/privacy", "What smolcast keeps on your PC", "Privacy"))
  out = ['        <ul class="rows">']
  for href, name, trail in items:
    out.append(f'          <li><a href="{href}"><svg aria-hidden="true"><use href="/icons.svg#doc" /></svg><span class="name">{name}</span><span class="trail">{trail}</span></a></li>')
  out.append('        </ul>')
  return "\n".join(out)

CTA = '''      <section class="cta" aria-labelledby="get-title">
        <img src="/favicon.svg" alt="" width="56" height="56" />
        <div>
          <h2 id="get-title">Try smolcast</h2>
          <ul class="tags"><li class="tag">Windows 11, x64</li><li class="tag">8 MB</li><li class="tag">Free and open source</li></ul>
        </div>
        <a class="button" href="https://github.com/Archetypically/smolcast/releases/latest"><svg aria-hidden="true"><use href="/icons.svg#download" /></svg>Download</a>
      </section>'''

def head(title, desc, path, ld, ogtype="article", alt=""):
  img = f"{SITE}/og/{path.strip('/').replace('/', '-')}.png"
  url = SITE + path
  return f'''<!doctype html>
<html lang="en">
  <head>
    <!-- partial:head -->
    <!-- /partial:head -->
    <title>{title} · smolcast</title>
    <meta name="description" content="{esc_attr(desc)}" />
    <link rel="canonical" href="{url}" />
    <meta name="robots" content="index, follow, max-snippet:-1" />
    <meta property="og:type" content="{ogtype}" />
    <meta property="og:site_name" content="smolcast" />
    <meta property="og:locale" content="en_US" />
    <meta property="og:url" content="{url}" />
    <meta property="og:title" content="{esc_attr(title)}" />
    <meta property="og:description" content="{esc_attr(desc)}" />
    <meta property="og:image" content="{img}" />
    <meta property="og:image:type" content="image/png" />
    <meta property="og:image:width" content="1200" />
    <meta property="og:image:height" content="630" />
    <meta property="og:image:alt" content="{esc_attr(alt)}" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:title" content="{esc_attr(title)}" />
    <meta name="twitter:description" content="{esc_attr(desc)}" />
    <meta name="twitter:image" content="{img}" />
    <meta name="twitter:image:alt" content="{esc_attr(alt)}" />

    <script type="application/ld+json">
{ld}
    </script>
  </head>
  <body>
    <a class="skip" href="#content">Skip to the comparison</a>

    <!-- partial:topbar -->
    <!-- /partial:topbar -->

    <main class="doc" id="content" tabindex="-1">'''

TAIL = '''    </main>

    <!-- partial:footer -->
    <!-- /partial:footer -->
  </body>
</html>
'''

def ldjson(graph):
  s = json.dumps({"@context":"https://schema.org","@graph":graph}, indent=2, ensure_ascii=False)
  return "\n".join("      "+l for l in s.split("\n"))

def crumbs(items):
  out=['        <ol class="crumbs">']
  for name,href in items:
    out.append(f'          <li><a href="{href}">{name}</a></li>' if href else f'          <li aria-current="page">{name}</li>')
  out.append('        </ol>'); return "\n".join(out)

def bc_ld(items):
  return {"@type":"BreadcrumbList","itemListElement":[dict({"@type":"ListItem","position":i+1,"name":n}, **({"item":SITE+h} if h else {})) for i,(n,h) in enumerate(items)]}

def stamp(what):
  return f'        <p class="stamp"><span class="tag">Updated <time datetime="{DATE}">{DATE_TEXT}</time></span> Checked against {what}. <a href="#sources">Sources</a></p>'

def footnotes(sources):
  items = "".join(f'<li>{name}: <a href="{url}">{label}</a></li>' for name,url,label in [SMOL_SRC]+sources)
  return f'''      <footer class="footnotes" id="sources" aria-labelledby="sources-title">
        <h2 id="sources-title">Sources</h2>
        <ol>{items}</ol>
        <p>Checked against these versions on <time datetime="{DATE}">{DATE_TEXT}</time>, which is also when this page was last updated. Found something stale? <a href="https://github.com/Archetypically/smolcast/issues">Open an issue</a>.</p>
      </footer>'''

def card(name, by, ts, best, us=False, href=None, visit=None):
  icon = ICON if us else ""
  links = []
  if href: links.append(f'<a href="{href}">Read the comparison</a>')
  if visit: links.append(f'<a href="{visit}">Get {name}</a>')
  more = f'\n          <p class="links">{"".join(links)}</p>' if links else ""
  return f'''        <article class="card{' us' if us else ''}">
          <h2>{icon}{name}</h2>
          <p class="by">{by}</p>
          {tags(ts)}
          <p class="best"><b>Best for</b> {best}</p>{more}
        </article>'''

def curl(text):
  # Typographic apostrophes in text, never inside tags or the JSON-LD block.
  top, sep, rest = text.partition("</script>")
  return top + sep + re.sub(r">[^<]+<", lambda m: re.sub(r"(?<=\w)'(?=\w)", "\u2019", m.group(0)), rest)
def write(path, text):
  with open(path, "w", encoding="utf-8") as f:
    f.write(curl(text))

os.makedirs(os.path.join(OUT,"compare"), exist_ok=True)
for p in PAGES:
  path=f"/compare/{p['slug']}"
  bc=[("smolcast","/"),("Alternatives","/compare/"),(p["name"],None)]
  us = p["us"]
  graph=[{"@type":"WebPage","@id":SITE+path,"url":SITE+path,"name":p["title"],"description":p["desc"],
    "datePublished":DATE,"dateModified":DATE,"isPartOf":{"@id":SITE+"/#website"},
    "about":[{"@id":SITE+"/#app"},{"@type":"SoftwareApplication","name":p["full"],"url":p["url"]}],
    "author":{"@id":SITE+"/#author"}}, bc_ld(bc)]
  body=[head(p["title"],p["desc"],path,ldjson(graph),alt=f"smolcast vs {p['full']}: account, clipboard history, snippets, emoji, extensions, AI and download, side by side."),
   '      <header class="doc-hero">', crumbs(bc), f'        <h1>smolcast vs {p["full"]}</h1>',
   f'        <p class="lead">{p["lead"]}</p>', stamp(p["checked"]), '      </header>', '',
   '      <div class="cards">', card("smolcast","Open source",p["us_tags"],p["us_best"],us=True), card(p["full"],p["them_by"],p["them_tags"],p["them_best"],visit=p["visit"]), '      </div>','',
   '      <section class="doc-section" aria-labelledby="table-title">','        <h2 id="table-title">Feature by feature</h2>',
   '        <p>Bold marks where smolcast comes out ahead, and only where the other side publishes the fact.</p>',
   '        <div class="table-wrap">', table([("smolcast",us),(p["full"],p["them"])]), '        </div>','      </section>','',
   '      <section class="doc-section" aria-labelledby="choose-title">','        <h2 id="choose-title">Pick the one that fits</h2>','        <div class="choose">',
   '          <div class="card us"><h3>Go with smolcast if</h3><ul class="checks">'+"".join(f"<li>{x}</li>" for x in p["us_if"])+'</ul></div>',
   f'          <div class="card"><h3>Go with {p["name"]} if</h3><ul class="checks">'+"".join(f"<li>{x}</li>" for x in p["them_if"])+f'</ul><p class="links"><a href="{p["visit"]}">Get {p["full"]}</a></p></div>',
   '        </div>', f'        <p class="related">{RELATED[p["slug"]]}</p>', '      </section>','',
   '      <section class="doc-section" aria-labelledby="moving-title">',f'        <h2 id="moving-title">Coming from {p["name"]}</h2>','        <ul class="maps">']
  body += [f'          <li><span class="from">{a}</span><span>{b}</span></li>' for a,b in p["maps"]]
  body += ['        </ul>','      </section>','',CTA,'',
   '      <section class="doc-section" aria-labelledby="more-title">','        <h2 id="more-title">More alternatives</h2>', rows_html(p["slug"]),'      </section>','',footnotes(p["sources"]), TAIL]
  write(os.path.join(OUT,"compare",p["slug"]+".html"), "\n".join(body))

# hub
HUB=PAGES[:3]
path="/compare/"
title=HUBMETA["title"]
desc=HUBMETA["desc"]
bc=[("smolcast","/"),("Alternatives",None)]
graph=[{"@type":"CollectionPage","@id":SITE+path,"url":SITE+path,"name":title,"description":desc,"datePublished":DATE,"dateModified":DATE,
  "isPartOf":{"@id":SITE+"/#website"},"about":[{"@id":SITE+"/#app"}]+[{"@type":"SoftwareApplication","name":p["full"],"url":p["url"]} for p in HUB],
  "hasPart":[{"@id":SITE+f"/compare/{p['slug']}"} for p in PAGES],
  # The same pages, in the order the visible "Alternatives, one on one" list gives them.
  "mainEntity":{"@type":"ItemList","name":"smolcast alternatives","numberOfItems":len(PAGES),
    "itemListElement":[{"@type":"ListItem","position":i+1,"name":f"smolcast vs {p['full']}","url":SITE+f"/compare/{p['slug']}"} for i,p in enumerate(PAGES)]}}, bc_ld(bc)]
hubus = HUBUS
# On the hub, as on every page, only smolcast's wins are bold.
cards=[card("smolcast","Open source",["Free","Open source","8 MB","No account"],"the essentials, small and private",us=True)]
for p in HUB: cards.append(card(p["full"],p["them_by"],p["them_tags"],p["them_best"],href=f"/compare/{p['slug']}",visit=p["visit"]))
body=[head(title,desc,path,ldjson(graph),"website",alt="Windows launcher alternatives compared: smolcast, Raycast for Windows, PowerToys Command Palette and Flow Launcher, feature by feature."),
 '      <header class="doc-hero">', crumbs(bc), f'        <h1>{title}</h1>',
 '        <p class="lead">smolcast and three alternatives, from one small app to a full extension platform.</p>',
 f'        <p class="stamp"><span class="tag">Updated <time datetime="{DATE}">{DATE_TEXT}</time></span> Checked against each project\'s own site. smolcast is ours, so the <a href="#sources">sources</a> are below.</p>',
 '      </header>','','      <div class="cards">', *cards, '      </div>','',
 '      <section class="doc-section" aria-labelledby="table-title">','        <h2 id="table-title">Feature by feature</h2>',
 '        <p>Bold marks where smolcast comes out ahead of all three, and only on published facts.</p>',
 '        <div class="table-wrap">', table([("smolcast",hubus)]+[(p["full"],p["them"]) for p in HUB], HUBROWS, wide=True), '        </div>','      </section>','',
 '      <section class="doc-section" aria-labelledby="mac-title">','        <h2 id="mac-title">Tinycast and Alfred are the Mac cousins</h2>',
 '        <p>Tinycast is free, open source and Mac only, and Alfred is the long-standing Mac favorite. smolcast is the closest Windows alternative to either: <a href="/compare/tinycast">smolcast vs Tinycast</a>, <a href="/compare/alfred">smolcast vs Alfred</a>.</p>','      </section>','',
 CTA,'','      <section class="doc-section" aria-labelledby="more-title">','        <h2 id="more-title">Alternatives, one on one</h2>', rows_html("hub"),'      </section>','',footnotes(list(dict.fromkeys(src for p in PAGES for src in p["sources"]))), TAIL]
write(os.path.join(OUT,"compare","index.html"), "\n".join(body))
print("wrote public/compare/")
