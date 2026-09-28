# Multipage website

The public site is a collection of real static HTML documents, not a JavaScript
router. GitHub Pages publishes **`build/site/`**, not the editorial `site/`
source directory. Build it with Python 3.10 or newer (standard library only):

```bash
python3 tools/build_site.py
python3 tools/check_site.py build/site
python3 -m http.server 8000 --directory build/site --bind 127.0.0.1
```

Open `http://127.0.0.1:8000/`. Ordinary links are relative and also work beneath
the production `/Vibrix/` prefix. To reproduce custom 404 assets and links at
arbitrary missing URLs, serve this output beneath `/Vibrix/`: the 404 document
intentionally uses project-absolute links because its requested path is unknown.

## Pages and editorial ownership

| Route | Content |
| --- | --- |
| `/` | Original hero/illustrative console, a six-page directory and closing CTA |
| `/about/` | Vision, four product principles and the dependency/build philosophy |
| `/architecture/` | The system-layer diagram and implementation/future boundaries |
| `/roadmap/` | Milestone overview and links to authoritative status and ROADMAP.md |
| `/software/` | Planned package catalog, explicitly not available downloads |
| `/community/` | Contributors/recorded agent roles and optional GitHub activity |
| `/development/` | Dated update cards and the engineering-blog introduction |
| `/status/`, `/docs/`, `/blog/` | Existing pages, with shared navigation and breadcrumbs |
| `/blog/why-vibrix-exists/` | Existing article, original byline, date and content retained |
| `/legal/terms/`, `/legal/privacy/`, `/legal/license/` | Existing legal text, unchanged |
| `/404.html` | Noindex recovery page; not listed in the sitemap |

The current source produces **15 documents** (14 content pages plus a 404).

To avoid forking editorial content while PR #100 is still being integrated,
`site/index.html` remains the source of the original homepage sections. The
builder assigns each section exactly once to a page. Edit that source to change
the corresponding text; edit `ROUTES` in `tools/build_site.py` for page titles,
introductions and descriptions. Existing standalone documents stay in their
current `site/.../index.html` paths. Generated files are disposable output and
must not be edited or committed.

This is deliberately a small migration, not a new framework or an npm toolchain.
Unknown, duplicate or missing source sections fail the build rather than silently
losing content. Independently authored files at a generated route also fail until
the overlap is explicitly integrated. A future migration to separate editorial
fragments can retain the same published URLs.

## Shared layout and navigation

The builder installs a shared header and footer on **every** generated document,
including nested articles and legal pages. Desktop navigation and the native
HTML `details` mobile disclosure expose the same links in the same order.
Both work without JavaScript. Current pages/sections use `aria-current`, every
page has a skip link and focusable main landmark, and nested pages include
visible breadcrumbs plus matching `BreadcrumbList` structured data.

The original dark/lime visual identity and existing assets remain in place.
`site/multipage.css` adds page introductions, compact directory cards, shared
navigation/footer, responsive layouts and reduced-motion handling. No new fonts,
third-party scripts, package dependencies, analytics or runtime services are added.

Only the homepage illustration and optional Community activity feed need the
existing browser script. Their failure cannot disable navigation. The original
animation label and GitHub-data failure states remain intact.

Legacy `/#vision`, `/#architecture`, `/#roadmap`, `/#software` and `/#updates`
bookmarks still land on a corresponding homepage directory link. Internal links
to those old anchors are migrated directly to the new pages. Nested asset URLs,
queries and fragments are rebased; external links, JSON-LD and article text are
not rewritten. Canonicals remain absolute. The sitemap is generated from the
actual output pages and excludes the noindex 404.

## Validation and deployment

```bash
python3 -m unittest discover -s tools -p 'test_build_site.py' -v
python3 -m unittest discover -s tools -p 'test_check_site.py' -v
python3 tools/build_site.py
python3 tools/check_site.py build/site
node --test tools/test-site.cjs
```

The builder tests cover content allocation, missing/duplicate sections, relative
URLs, legacy bookmarks, canonical metadata, shared navigation, byline preservation,
repeatable builds, route collisions, symlinks and refusal to erase unowned output.
It finishes all HTML transformations before replacing its marked output directory.
Do not point it at an arbitrary folder of important data.

The Pages workflow runs these checks against the **built** website, then retains
a seven-day `website-preview-<run>-<attempt>` artifact. PRs never deploy. Only the
separate main-branch deployment job has Pages/OIDC permissions; it builds and
validates the same revision before publishing `build/site`.

This website-only change preserves the parent PR's dated engineering snapshot;
it does not mark new kernel milestones or present concurrent, unmerged work as
shipped. Merge PR #100 first, synchronize/retarget the multipage PR to main and
rerun checks before integrating. Passing tests on the stacked branch do not waive
the parent PR's unresolved integration requirements.

## Primary references checked 2026-09-28

- [GitHub Pages static sites and project URL prefixes](https://docs.github.com/en/pages/getting-started-with-github-pages/what-is-github-pages)
- [W3C WAI menu structure, consistent links and current-page indication](https://www.w3.org/WAI/tutorials/menus/structure/)

No third-party implementation code was copied. The source-preserving parser,
layout and tests are first-party Python/HTML/CSS, authored by GPT-6 Astra Pro.
