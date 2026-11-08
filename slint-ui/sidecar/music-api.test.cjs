'use strict'
const test = require('node:test')
const assert = require('node:assert/strict')
const { dispatch, METHODS } = require('./music-api.cjs')
const request = (operation, params = {}) => ({ version: 1, operation, params, cookie: 'test-session' })
test('every exposed operation uses its mapped endpoint and keeps the session out of params overrides', async () => {
  for (const [operation, method] of Object.entries(METHODS)) {
    let called = 0
    const result = await dispatch({ [method]: async p => {
      called++; assert.equal(p.cookie, 'test-session'); assert.equal(p.id, '9007199254740991')
      return { status: 200, body: { code: 200, id: p.id } }
    } }, async () => {}, request(operation, { id: '9007199254740991', cookie: 'override' }))
    assert.equal(called, 1); assert.equal(result.body.id, '9007199254740991')
  }
})
test('playlist mutation uses playlist_tracks with comma joined full IDs', async () => {
  for (const [op, expected] of [['playlist_track_add', 'add'], ['playlist_track_delete', 'del']]) {
    const result = await dispatch({ playlist_tracks: async p => {
      assert.deepEqual(p, { cookie: 'test-session', pid: '6500000001', tracks: '4500000001,4500000002', op: expected })
      return { status: 200, body: { code: 200 } }
    } }, null, request(op, { pid: '6500000001', tracks: ['4500000001', '4500000002'] }))
    assert.equal(result.body.code, 200)
  }
})
test('metadata strings are JSON encoded and both batch results must succeed', async () => {
  const paths = ['/api/playlist/update/name', '/api/playlist/desc/update']
  for (const nested of [undefined, { code: 500, message: 'cannot save description' }, { code: 200 }]) {
    const result = await dispatch({ batch: async p => {
      assert.deepEqual(JSON.parse(p[paths[0]]), { id: '5000000000', name: '名字"\n新行' })
      assert.deepEqual(JSON.parse(p[paths[1]]), { id: '5000000000', desc: 'line\nline' })
      assert.equal(Object.keys(p).length, 3)
      return { status: 200, body: { code: 200, [paths[0]]: { code: 200 }, [paths[1]]: nested } }
    } }, null, request('playlist_update', { id: '5000000000', name: '名字"\n新行', desc: 'line\nline' }))
    assert.equal(result.body.code === 200, nested?.code === 200)
  }
})
test('business rejections retain the message and mutations are never retried', async () => {
  let count = 0
  const result = await dispatch({ playlist_create: async () => { count++; throw { status: 200, body: { code: 401, msg: 'login expired' } } } }, null, request('playlist_create'))
  assert.equal(count, 1); assert.equal(result.body.msg, 'login expired'); assert.equal(result.body.code, 401)
})
test('missing xeapi key is refreshed once, then playback falls back to legacy with inferred quality', async () => {
  let attempts = 0, refreshes = 0
  const result = await dispatch({ song_url_v1: async () => { attempts++; throw new Error('xeapi public key missing') }, song_url: async p => {
    assert.equal(p.br, 320000); return { status: 200, body: { code: 200, data: [{ br: 320000, url: 'https://example.test/song' }] } }
  } }, async () => { refreshes++ }, request('song_url', { id: '5000000000', level: 'exhigh' }))
  assert.equal(attempts, 2); assert.equal(refreshes, 1); assert.equal(result.body.data[0].level, 'exhigh')
})
test('ordinary song errors do not retry or fall back', async () => {
  let count = 0
  const result = await dispatch({ song_url_v1: async () => { count++; throw { status: 403, body: { code: 403, message: 'not purchased' } } } }, () => assert.fail(), request('song_url'))
  assert.equal(count, 1); assert.equal(result.status, 403)
})
test('unknown operations are rejected rather than invoking arbitrary package exports', async () => {
  const result = await dispatch({ server: () => assert.fail() }, null, request('server'))
  assert.equal(result.status, 400)
})
