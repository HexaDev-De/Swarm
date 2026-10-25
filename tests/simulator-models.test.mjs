import test from 'node:test';
import assert from 'node:assert/strict';
import { isDuo, modelSpec } from '../src/simulator-models.ts';

const id = name => `com.apple.CoreSimulator.SimDeviceType.${name}`;

test('current devices get their own hardware and launch finishes', () => {
  const pro = modelSpec(id('iPhone-17-Pro'), 'phone');
  assert.equal(pro.camera, 'pro-plateau');
  assert.equal(pro.back, 'metal-glass');
  assert.equal(pro.finishes[0].name, 'Cosmic Orange');
  assert.equal(modelSpec(id('iPhone-17-Pro-Max'), 'phone').camera, 'pro-plateau');
  assert.equal(modelSpec(id('iPhone-Air'), 'phone').camera, 'air-plateau');
  assert.equal(modelSpec(id('iPhone-17'), 'phone').camera, 'capsule');
  assert.equal(modelSpec(id('iPhone-16-Pro-Max'), 'phone').camera, 'pro-square');
  assert.equal(modelSpec(id('iPhone-16e'), 'phone').front, 'notch');
  assert.equal(modelSpec(id('iPhone-13-mini'), 'phone').camera, 'dual-diagonal');
  assert.equal(modelSpec(id('iPhone-12-Pro'), 'phone').front, 'notch');
  assert.equal(modelSpec(id('iPhone-SE-3rd-generation'), 'phone').camera, 'single');
  assert.equal(modelSpec(id('iPad-Pro-13-inch-M5-16GB'), 'pad').camera, 'pad-pro');
  assert.equal(modelSpec(id('iPad-Air-11-inch-M3'), 'pad').finishes.length, 4);
  assert.equal(modelSpec(id('iPad-mini-A17-Pro'), 'pad').smartConnector, false);
});

test('Camera Control appears only on models that have it', () => {
  for (const name of ['iPhone-17-Pro', 'iPhone-Air', 'iPhone-17', 'iPhone-16', 'iPhone-16-Pro']) assert.equal(modelSpec(id(name), 'phone').cameraControl, true, name);
  for (const name of ['iPhone-16e', 'iPhone-15-Pro', 'iPhone-SE-3rd-generation']) assert.equal(modelSpec(id(name), 'phone').cameraControl, false, name);
});

test('unknown device types fall back by family and every finish is a color', () => {
  assert.equal(modelSpec('com.example.Unknown', 'pad').front, 'pad');
  assert.equal(modelSpec('', 'phone').camera, 'single');
  for (const name of ['iPhone-17-Pro', 'iPhone-Air', 'iPhone-17', 'iPad-Air-11-inch-M3', 'iPad-Pro-11-inch-M4-8GB', 'iPad-A16'])
    for (const finish of modelSpec(id(name), name.startsWith('iPad') ? 'pad' : 'phone').finishes) assert.match(finish.color, /^#[0-9a-f]{6}$/);
});

test('Android emulator profiles get Pixel and generic Android hardware', () => {
  const pixel9 = modelSpec('android:pixel_9', 'phone');
  assert.equal(pixel9.camera, 'pixel-pill');
  assert.equal(pixel9.front, 'punch');
  assert.equal(pixel9.android, true);
  assert.equal(pixel9.finishes[0].name, 'Obsidian');
  assert.equal(modelSpec('android:pixel_7_pro', 'phone').camera, 'pixel-bar');
  assert.equal(modelSpec('android:medium_phone', 'phone').camera, 'android-square');
  assert.equal(modelSpec('android:pixel_tablet', 'pad').front, 'pad');
  assert.equal(modelSpec('android:generic_tablet_10', 'pad').front, 'pad');
  assert.equal(modelSpec('com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro', 'phone').android, undefined);
});

test('iPhone Duo is the foldable with its own finishes and camera', () => {
  for (const type of [id('iPhone-Duo'), id('V68'), 'iPhone-Duo']) {
    const duo = modelSpec(type, 'phone');
    assert.equal(duo.key, 'iPhone-Duo', type);
    assert.equal(duo.camera, 'duo-dual');
    assert.equal(duo.cameraControl, true);
    assert.deepEqual(duo.finishes.map(f => [f.name, f.color]), [['Night Sky', '#1f2430'], ['Star White', '#ecebe6']]);
  }
  assert.equal(isDuo(id('iPhone-Duo')), true);
  assert.equal(isDuo(id('V68')), true);
  for (const type of [id('iPhone-17-Pro'), id('iPad-A16'), 'android:pixel_9', '']) assert.equal(isDuo(type), false, type);
  assert.equal(modelSpec(id('iPhone-17-Pro'), 'phone').key, 'iPhone-17-Pro');
});
