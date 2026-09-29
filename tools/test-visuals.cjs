'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const {
  crystal, shield, orbit, globe, particles, rotation, bufferSize, canAnimate
} = require('../site/visuals.js');

function finiteGeometry(name, data) {
  test(`${name}: finite interleaved geometry`, () => {
    assert.ok(data instanceof Float32Array);
    assert.ok(data.length > 0);
    assert.equal(data.length % 6, 0);
    for (const value of data) assert.ok(Number.isFinite(value));
  });
}

finiteGeometry('crystal', crystal());
finiteGeometry('shield', shield());
finiteGeometry('orbit', orbit(1.7, 1.0, 0.2));
finiteGeometry('globe', globe());
finiteGeometry('particles', particles());

test('rotation matrix remains finite', () => {
  const matrix = rotation(0.2, 0.4, 0.1);
  assert.equal(matrix.length, 9);
  for (const value of matrix) assert.ok(Number.isFinite(value));
});

test('back buffer caps DPR and longest edge', () => {
  const [w, h] = bufferSize(1600, 900, 3);
  assert.ok(w > 0 && h > 0);
  assert.ok(Math.max(w, h) <= 1100);
});

test('every lifecycle gate can stop animation', () => {
  const base = {visible:true, hidden:false, reduced:false, paused:false, lost:false};
  assert.equal(canAnimate(base), true);
  for (const key of ['visible','hidden','reduced','paused','lost']) {
    const state = {...base};
    state[key] = key === 'visible' ? false : true;
    assert.equal(canAnimate(state), false);
  }
});
