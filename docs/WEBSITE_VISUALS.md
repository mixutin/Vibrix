# Website 3D presentation

Authoring model: **GPT-5.6 Sol**. Research checked 29 September 2026.

The homepage adds an interactive crystalline core, wireframe globe, faceted shield,
orbit lines and particles. These are decorative illustrations, not live kernel
output, security certification or roadmap evidence.

## Implementation

- `site/visuals.js` uses first-party procedural WebGL 1 geometry with no runtime
  framework, CDN model, texture, telemetry or storage dependency.
- `site/visuals.css` provides the dimensional presentation and a CSS fallback.
- The renderer caps its back buffer, pauses while hidden or offscreen, supports a
  manual pause control, and respects `prefers-reduced-motion`.
- WebGL context loss switches back to static artwork and restoration rebuilds GPU
  resources.
- Generated multipage routes import the same visual accents through
  `site/multipage.css`.

## Validation

```text
python3 tools/build_site.py
python3 tools/check_site.py build/site
node --check site/visuals.js
node --test tools/test-site.cjs tools/test-visuals.cjs
```

The unit tests cover finite procedural geometry, rotation data, render-buffer
limits and animation lifecycle gates. They do not claim physical-GPU, Safari,
Firefox or mobile-device validation.

## Research

Primary references checked for the implementation:

- MDN, WebGL API and context-loss handling:
  https://developer.mozilla.org/en-US/docs/Web/API/HTMLCanvasElement/webglcontextlost_event
- MDN, reduced-motion media feature:
  https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-reduced-motion
- MDN, WEBGL_lose_context extension:
  https://developer.mozilla.org/en-US/docs/Web/API/WEBGL_lose_context

A larger rendering framework was considered unnecessary for this bounded,
untextured scene. If the website grows into model loading, post-processing or a
reusable renderer, that tradeoff should be revisited.

No Rust, kernel, storage, authentication, backend, dependency graph or roadmap
completion state changes are included.
