'use strict';

// First-party decorative geometry. No assets, telemetry or runtime dependencies.
(function () {
  const TAU = Math.PI * 2;
  const clamp = (value, min, max) => Math.max(min, Math.min(max, value));

  function triangle(out, a, b, c) {
    const u = b.map((v, i) => v - a[i]);
    const v = c.map((n, i) => n - a[i]);
    const n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    const length = Math.hypot(...n) || 1;
    for (const point of [a, b, c]) out.push(...point, ...n.map(x => x / length));
  }

  function crystal() {
    const out = [];
    const rim = [[0.92, 0, 0], [0, 0, 0.92], [-0.92, 0, 0], [0, 0, -0.92]];
    for (let i = 0; i < 4; i++) {
      triangle(out, [0, 1.32, 0], rim[(i + 1) % 4], rim[i]);
      triangle(out, [0, -1.32, 0], rim[i], rim[(i + 1) % 4]);
    }
    return new Float32Array(out);
  }

  function shield() {
    const out = [];
    const rim = [[0, 1.22], [-0.92, 0.84], [-0.85, -0.25], [-0.52, -0.9], [0, -1.3], [0.52, -0.9], [0.85, -0.25], [0.92, 0.84]];
    for (let i = 0; i < rim.length; i++) {
      const a = rim[i], b = rim[(i + 1) % rim.length];
      triangle(out, [0, 0.12, 0.52], [...a, 0.1], [...b, 0.1]);
      triangle(out, [0, 0.12, -0.25], [...b, -0.16], [...a, -0.16]);
      triangle(out, [...a, 0.1], [...a, -0.16], [...b, -0.16]);
      triangle(out, [...a, 0.1], [...b, -0.16], [...b, 0.1]);
    }
    return new Float32Array(out);
  }

  function orbit(radius, tilt, turn, segments = 160) {
    const out = [];
    const point = t => {
      const x = radius * Math.cos(t), y = radius * Math.sin(t) * Math.cos(tilt), z = radius * Math.sin(t) * Math.sin(tilt);
      return [x * Math.cos(turn) - y * Math.sin(turn), x * Math.sin(turn) + y * Math.cos(turn), z];
    };
    for (let i = 0; i < segments; i++) {
      out.push(...point(i / segments * TAU), 0, 0, 1, ...point((i + 1) / segments * TAU), 0, 0, 1);
    }
    return new Float32Array(out);
  }

  function globe() {
    const out = [], segments = 80, radius = 1.28;
    const point = (lat, lon) => [radius * Math.cos(lat) * Math.cos(lon), radius * Math.sin(lat), radius * Math.cos(lat) * Math.sin(lon)];
    const line = (a, b) => out.push(...a, 0, 0, 1, ...b, 0, 0, 1);
    for (let lat = -4; lat <= 4; lat++) {
      for (let i = 0; i < segments; i++) line(point(lat * Math.PI / 10, i * TAU / segments), point(lat * Math.PI / 10, (i + 1) * TAU / segments));
    }
    for (let lon = 0; lon < 20; lon++) {
      for (let i = 0; i < segments / 2; i++) line(point(i * Math.PI / (segments / 2) - Math.PI / 2, lon * TAU / 20), point((i + 1) * Math.PI / (segments / 2) - Math.PI / 2, lon * TAU / 20));
    }
    return new Float32Array(out);
  }

  function particles(count = 150) {
    const out = [];
    for (let i = 0; i < count; i++) {
      const y = 1 - (i + 0.5) * 2 / count, angle = i * 2.399963229728653;
      const radius = 1.5 + (i % 7) / 12;
      const ring = Math.sqrt(1 - y * y);
      out.push(Math.cos(angle) * ring * radius, y * radius, Math.sin(angle) * ring * radius, 0, 0, 1);
    }
    return new Float32Array(out);
  }

  function rotation(x, y, z) {
    const cx = Math.cos(x), sx = Math.sin(x), cy = Math.cos(y), sy = Math.sin(y), cz = Math.cos(z), sz = Math.sin(z);
    return new Float32Array([cy * cz, cy * sz, -sy, sx * sy * cz - cx * sz, sx * sy * sz + cx * cz, sx * cy, cx * sy * cz + sx * sz, cx * sy * sz - sx * cz, cx * cy]);
  }

  function bufferSize(width, height, ratio) {
    const dpr = clamp(Number.isFinite(ratio) ? ratio : 1, 1, 1.75);
    const scale = Math.min(dpr, 1100 / Math.max(1, width, height));
    return [Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale))];
  }

  function canAnimate(state) {
    return state.visible && !state.hidden && !state.reduced && !state.paused && !state.lost;
  }

  function createRenderer(canvas) {
    const gl = canvas.getContext('webgl', {alpha: true, antialias: true, depth: true, powerPreference: 'low-power'});
    if (!gl) throw new Error('WebGL unavailable');
    const shaders = [], buffers = [];
    let program;
    const dispose = () => {
      buffers.forEach(buffer => gl.deleteBuffer(buffer));
      shaders.forEach(shader => gl.deleteShader(shader));
      if (program) gl.deleteProgram(program);
    };
    try {
      const sources = [
        `attribute vec3 aPosition; attribute vec3 aNormal;
         uniform mat3 uRotation; uniform float uAspect; uniform float uScale; uniform float uPoint;
         varying vec3 vNormal; varying vec3 vPosition;
         void main() {
           vec3 p = uRotation * aPosition * uScale;
           float depth = 5.2 - p.z;
           gl_Position = vec4(p.x * 2.35 / uAspect, p.y * 2.35, 1.004 * depth - 0.2004, depth);
           gl_PointSize = uPoint;
           vNormal = uRotation * aNormal; vPosition = p;
         }`,
        `precision mediump float;
         uniform vec3 uColor; uniform float uSolid; uniform float uAlpha; uniform float uDots;
         varying vec3 vNormal; varying vec3 vPosition;
         void main() {
           if (uDots > 0.5 && distance(gl_PointCoord, vec2(0.5)) > 0.5) discard;
           vec3 n = normalize(vNormal);
           float light = max(dot(n, normalize(vec3(-0.5, 0.8, 1.0))), 0.0);
           float rim = pow(1.0 - abs(n.z), 2.5);
           vec3 shaded = uColor * (0.16 + light * 0.75) + vec3(0.45, 0.95, 0.85) * rim * 0.65;
           vec3 color = mix(uColor, shaded, uSolid);
           float depthFade = mix(0.36, 1.0, clamp((vPosition.z + 2.0) / 4.0, 0.0, 1.0));
           gl_FragColor = vec4(color, uAlpha * mix(depthFade, 1.0, uSolid));
         }`
      ];
      program = gl.createProgram();
      if (!program) throw new Error('Program allocation failed');
      for (let i = 0; i < 2; i++) {
        const shader = gl.createShader(i === 0 ? gl.VERTEX_SHADER : gl.FRAGMENT_SHADER);
        if (!shader) throw new Error('Shader allocation failed');
        shaders.push(shader);
        gl.shaderSource(shader, sources[i]); gl.compileShader(shader); gl.attachShader(program, shader);
      }
      gl.bindAttribLocation(program, 0, 'aPosition');
      gl.linkProgram(program);
      if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error('Shader linking failed');
      gl.useProgram(program);
      const uniforms = {};
      for (const name of ['Rotation', 'Aspect', 'Scale', 'Point', 'Color', 'Solid', 'Alpha', 'Dots']) uniforms[name] = gl.getUniformLocation(program, `u${name}`);
      const normal = gl.getAttribLocation(program, 'aNormal');
      const upload = (data, mode) => {
        const buffer = gl.createBuffer();
        if (!buffer) throw new Error('Buffer allocation failed');
        buffers.push(buffer); gl.bindBuffer(gl.ARRAY_BUFFER, buffer); gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
        return {buffer, count: data.length / 6, mode};
      };
      const core = upload(crystal(), gl.TRIANGLES), protection = upload(shield(), gl.TRIANGLES), sphere = upload(globe(), gl.LINES);
      const rings = [upload(orbit(1.72, 1.03, 0.25), gl.LINES), upload(orbit(1.96, 0.91, 2.2), gl.LINES), upload(orbit(1.56, 0.48, -0.65), gl.LINES)];
      const stars = upload(particles(), gl.POINTS);
      gl.enableVertexAttribArray(0); gl.enableVertexAttribArray(normal);
      gl.enable(gl.DEPTH_TEST); gl.enable(gl.BLEND); gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
      gl.clearColor(0, 0, 0, 0);
      const draw = (mesh, angles, color, solid = false, scale = 1, alpha = 1) => {
        gl.bindBuffer(gl.ARRAY_BUFFER, mesh.buffer);
        gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 24, 0); gl.vertexAttribPointer(normal, 3, gl.FLOAT, false, 24, 12);
        gl.uniformMatrix3fv(uniforms.Rotation, false, angles); gl.uniform3fv(uniforms.Color, color);
        gl.uniform1f(uniforms.Scale, scale); gl.uniform1f(uniforms.Solid, solid ? 1 : 0);
        gl.uniform1f(uniforms.Alpha, alpha); gl.uniform1f(uniforms.Dots, mesh.mode === gl.POINTS ? 1 : 0);
        gl.depthMask(solid); gl.drawArrays(mesh.mode, 0, mesh.count);
      };
      return {
        dispose,
        render(time, pointer, mode) {
          gl.viewport(0, 0, canvas.width, canvas.height); gl.depthMask(true); gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
          gl.uniform1f(uniforms.Aspect, canvas.width / canvas.height);
          gl.uniform1f(uniforms.Point, Math.min(4, 2 * canvas.width / Math.max(1, canvas.clientWidth)));
          const base = rotation(-0.12 + pointer[1] * 0.22, time * 0.13 + pointer[0] * 0.38 + 0.45, 0.12);
          if (mode === 'core') draw(core, base, [0.66, 1.0, 0.38], true);
          if (mode === 'shield') draw(protection, rotation(-0.12 + pointer[1] * 0.2, Math.sin(time * 0.3) * 0.28 + pointer[0] * 0.4, -0.08), [0.34, 0.87, 1], true);
          if (mode === 'globe') draw(sphere, base, [0.51, 0.98, 0.78], false, 1, 0.92);
          rings.forEach((ring, i) => draw(ring, rotation(pointer[1] * 0.12, time * (i % 2 ? -0.06 : 0.08) + pointer[0] * 0.18, time * 0.018), i === 1 ? [0.37, 0.75, 1] : [0.72, 1, 0.48], false, 1, i === 2 ? 0.5 : 0.8));
          draw(stars, rotation(time * 0.015, time * 0.035, 0.1), [0.76, 1, 0.82], false, 1, 0.95);
        }
      };
    } catch (error) { dispose(); throw error; }
  }

  function initializeScene(root) {
    const canvas = root.querySelector('canvas');
    root.querySelector('.scene-controls').hidden = false;
    const pause = root.querySelector('[data-scene-pause]');
    const status = root.querySelector('[data-scene-status]');
    const reduced = matchMedia('(prefers-reduced-motion: reduce)');
    const fine = matchMedia('(hover: hover) and (pointer: fine)');
    const state = {visible: true, hidden: document.hidden, reduced: reduced.matches, paused: false, lost: false};
    let renderer, frame = 0, last = 0, time = 0, mode = 'core', pointer = [0, 0];
    const stop = () => { cancelAnimationFrame(frame); frame = 0; last = 0; };
    const paint = () => {
      if (!renderer || state.lost || !state.visible || state.hidden) return;
      renderer.render(time, pointer, mode);
    };
    const tick = now => {
      frame = 0;
      if (!canAnimate(state) || !renderer) return;
      if (!last || now - last >= 30) {
        time += last ? Math.min((now - last) / 1000, 0.08) : 0;
        last = now; paint();
      }
      frame = requestAnimationFrame(tick);
    };
    const sync = () => {
      stop();
      root.dataset.motion = renderer && canAnimate(state) ? 'on' : 'still';
      pause.disabled = !renderer || state.lost || state.reduced;
      pause.textContent = !renderer || state.lost ? 'Static artwork' : state.reduced ? 'Reduced motion' : state.paused ? 'Resume 3D' : 'Pause 3D';
      pause.setAttribute('aria-pressed', String(state.paused || state.reduced));
      paint();
      if (renderer && canAnimate(state)) frame = requestAnimationFrame(tick);
    };
    const resize = () => {
      const [width, height] = bufferSize(canvas.clientWidth, canvas.clientHeight, window.devicePixelRatio);
      if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
      sync();
    };
    const initialize = () => {
      if (renderer && !state.lost) renderer.dispose();
      renderer = undefined;
      try {
        renderer = createRenderer(canvas); state.lost = false; resize();
        root.dataset.renderer = 'webgl'; status.textContent = 'Interactive 3D · illustrative';
      } catch (_) {
        root.dataset.renderer = 'fallback'; status.textContent = 'Static artwork · illustrative'; stop(); pause.disabled = true;
        pause.textContent = 'Static artwork'; root.dataset.motion = 'still';
      }
    };
    root.querySelectorAll('[data-scene-mode]').forEach(button => button.addEventListener('click', () => {
      mode = button.dataset.sceneMode;
      root.dataset.mode = mode;
      root.querySelectorAll('[data-scene-mode]').forEach(item => item.setAttribute('aria-pressed', String(item === button)));
      paint();
    }));
    pause.addEventListener('click', () => { state.paused = !state.paused; pointer = [0, 0]; sync(); });
    canvas.addEventListener('pointermove', event => {
      if (!fine.matches || state.reduced || state.paused || event.pointerType === 'touch') return;
      const rect = canvas.getBoundingClientRect();
      pointer = [clamp((event.clientX - rect.left) / rect.width * 2 - 1, -1, 1), clamp(1 - (event.clientY - rect.top) / rect.height * 2, -1, 1)];
    }, {passive: true});
    canvas.addEventListener('pointerleave', () => { pointer = [0, 0]; });
    reduced.addEventListener('change', () => { state.reduced = reduced.matches; pointer = [0, 0]; sync(); });
    document.addEventListener('visibilitychange', () => { state.hidden = document.hidden; sync(); });
    window.addEventListener('pagehide', () => { state.hidden = true; stop(); });
    window.addEventListener('pageshow', () => { state.hidden = document.hidden; sync(); });
    canvas.addEventListener('webglcontextlost', event => {
      event.preventDefault(); state.lost = true; root.dataset.renderer = 'fallback';
      status.textContent = 'Static artwork · graphics paused'; sync();
    });
    canvas.addEventListener('webglcontextrestored', initialize);
    if (typeof ResizeObserver === 'function') new ResizeObserver(resize).observe(canvas);
    else window.addEventListener('resize', resize, {passive: true});
    if (typeof IntersectionObserver === 'function') new IntersectionObserver(entries => {
      state.visible = entries[0].isIntersecting; sync();
    }, {threshold: 0}).observe(root);
    initialize();
  }

  if (typeof module !== 'undefined' && module.exports) module.exports = {crystal, shield, orbit, globe, particles, rotation, bufferSize, canAnimate};
  if (typeof document !== 'undefined') document.querySelectorAll('[data-vibrix-scene]').forEach(initializeScene);
}());
