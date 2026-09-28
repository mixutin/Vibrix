'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {safeActivityUrl, counterText, dateText, activityItems} = require('../site/script.js');
const fallback = 'https://github.com/mixutin/Vibrix';

test('repository PR links are retained', () => {
  assert.equal(safeActivityUrl(fallback + '/pull/100'), fallback + '/pull/100');
});
test('untrusted schemes, hosts, credentials and lookalike paths fall back', () => {
  for (const value of ['javascript:alert(1)', 'https://evil.example/', 'https://github.com.evil.example/mixutin/Vibrix',
    'https://evil@github.com/mixutin/Vibrix', 'https://github.com/mixutin/Vibrix-evil', undefined]) {
    assert.equal(safeActivityUrl(value), fallback);
  }
});
test('counters never invent values for malformed data', () => {
  assert.equal(counterText(0), '0');
  assert.equal(counterText(123), '123');
  for (const value of [-1, NaN, Infinity, '4', null]) assert.equal(counterText(value), '—');
});
test('dates fail explicitly for invalid data', () => {
  assert.equal(dateText(null), 'Unavailable');
  assert.equal(dateText('not-a-date'), 'Unavailable');
});
test('unexpected events do not break the activity mapper', () => {
  assert.deepEqual(activityItems(null), []);
  assert.deepEqual(activityItems([null, false, 5]), []);
  assert.equal(activityItems([{}])[0].text, 'Activity by unknown contributor');
});
test('API strings remain plain text and links are validated', () => {
  const item = activityItems([{type: 'PushEvent', actor: {login: '<img onerror=bad>'},
    payload: {issue: {html_url: 'javascript:bad'}}}])[0];
  assert.equal(item.text, 'Push by <img onerror=bad>');
  assert.equal(item.url, fallback);
});
test('the feed remains bounded', () => {
  assert.equal(activityItems(Array.from({length: 100}, () => ({}))).length, 6);
});
