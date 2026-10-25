/* Product details for the 3D simulator: which hardware each simulated device has
 * and its real finishes. Geometry is in model units (one unit is 100 points);
 * lengths in millimetres are converted with the body's own width. No logos. */

export type Finish = { id: string; name: string; color: string };
type Camera = 'pro-plateau' | 'air-plateau' | 'capsule' | 'pro-square' | 'dual-diagonal' | 'dual-vertical' | 'single-square' | 'single' | 'pad-pro' | 'pad-single' | 'pixel-pill' | 'pixel-bar' | 'android-square' | 'duo-dual';
export type ModelSpec = {
  key: string;
  camera: Camera;
  /** Dynamic Island, notch, home button (front from the profile), or an iPad bezel camera. */
  front: 'island' | 'notch' | 'none' | 'pad' | 'punch';
  /** Body depth as a share of body width, from Apple's dimensions. */
  depth: number;
  /** The back: anodized metal, glass, or metal with a glass panel (17 Pro). */
  back: 'metal' | 'glass' | 'metal-glass';
  cameraControl: boolean;
  smartConnector: boolean;
  finishes: Finish[];
  /** Android hardware: power and volume on the right, nothing on the left. */
  android?: boolean;
  /** Bezel and display corner radius in model units, where they differ from iPhone's. */
  bezel?: number;
  corner?: number;
};

const f = (id: string, name: string, color: string): Finish => ({ id, name, color });
const NEUTRAL = [f('black', 'Black', '#2b2b2e'), f('white', 'White', '#e9e7e2')];
const ANDROID = { front: 'punch' as const, back: 'glass' as const, cameraControl: false, smartConnector: false, android: true, bezel: 0.2, corner: 0.42 };
const SPECS: { match: RegExp; spec: Omit<ModelSpec, 'key'> }[] = [
  // Android emulator profiles (`android:<hw.device.name>`).
  { match: /^android:.*tablet/, spec: { ...ANDROID, camera: 'pad-single', front: 'pad', depth: 0.032, back: 'metal', bezel: 0.3, corner: 0.2,
    finishes: [f('hazel', 'Hazel', '#8e9488'), f('porcelain', 'Porcelain', '#ece6dc')] } },
  { match: /^android:pixel_(9|10)/, spec: { ...ANDROID, camera: 'pixel-pill', depth: 0.118,
    finishes: [f('obsidian', 'Obsidian', '#2c2d30'), f('porcelain', 'Porcelain', '#ece6dc'), f('wintergreen', 'Wintergreen', '#c4d9c6'), f('peony', 'Peony', '#f1bfd0')] } },
  { match: /^android:pixel_[678]/, spec: { ...ANDROID, camera: 'pixel-bar', depth: 0.12,
    finishes: [f('obsidian', 'Obsidian', '#2c2d30'), f('hazel', 'Hazel', '#8e9488'), f('rose', 'Rose', '#f2d2cf')] } },
  { match: /^android:/, spec: { ...ANDROID, camera: 'android-square', depth: 0.11, finishes: [f('black', 'Black', '#28292c'), f('silver', 'Silver', '#d9dadb')] } },
  // iPhone Duo (V68): titanium, 48MP Dual Fusion, depth of the closed body (11.3 of 84.1 mm).
  { match: /iPhone-Duo|V68/, spec: { camera: 'duo-dual', front: 'island', depth: 0.134, back: 'glass', cameraControl: true, smartConnector: false,
    finishes: [f('night-sky', 'Night Sky', '#1f2430'), f('star-white', 'Star White', '#ecebe6')] } },
  { match: /iPhone-17-Pro/, spec: { camera: 'pro-plateau', front: 'island', depth: 0.122, back: 'metal-glass', cameraControl: true, smartConnector: false,
    finishes: [f('cosmic-orange', 'Cosmic Orange', '#d9692f'), f('deep-blue', 'Deep Blue', '#33405c'), f('silver', 'Silver', '#dcdcda')] } },
  { match: /iPhone-Air/, spec: { camera: 'air-plateau', front: 'island', depth: 0.075, back: 'glass', cameraControl: true, smartConnector: false,
    finishes: [f('sky-blue', 'Sky Blue', '#c4d6e6'), f('light-gold', 'Light Gold', '#e6d9bf'), f('cloud-white', 'Cloud White', '#f1f0ec'), f('space-black', 'Space Black', '#29292c')] } },
  { match: /iPhone-17$/, spec: { camera: 'capsule', front: 'island', depth: 0.111, back: 'glass', cameraControl: true, smartConnector: false,
    finishes: [f('lavender', 'Lavender', '#c9bde0'), f('sage', 'Sage', '#b5c3a9'), f('mist-blue', 'Mist Blue', '#a6bad2'), ...NEUTRAL] } },
  { match: /iPhone-16-Pro/, spec: { camera: 'pro-square', front: 'island', depth: 0.114, back: 'glass', cameraControl: true, smartConnector: false,
    finishes: [f('desert', 'Desert Titanium', '#bfa58c'), f('natural', 'Natural Titanium', '#b8b2a7'), f('white', 'White Titanium', '#e4e2dd'), f('black', 'Black Titanium', '#3a393b')] } },
  { match: /iPhone-16e/, spec: { camera: 'single-square', front: 'notch', depth: 0.11, back: 'glass', cameraControl: false, smartConnector: false, finishes: NEUTRAL } },
  { match: /iPhone-16/, spec: { camera: 'capsule', front: 'island', depth: 0.11, back: 'glass', cameraControl: true, smartConnector: false,
    finishes: [f('ultramarine', 'Ultramarine', '#6f82e2'), f('teal', 'Teal', '#8dbfb4'), f('pink', 'Pink', '#f0a7c8'), ...NEUTRAL] } },
  { match: /iPhone-1[345]-Pro/, spec: { camera: 'pro-square', front: 'island', depth: 0.116, back: 'glass', cameraControl: false, smartConnector: false,
    finishes: [f('natural', 'Natural Titanium', '#b8b2a7'), f('blue', 'Blue Titanium', '#3f4a5a'), f('white', 'White Titanium', '#e4e2dd'), f('black', 'Black Titanium', '#3a393b')] } },
  { match: /iPhone-1[345]/, spec: { camera: 'dual-diagonal', front: 'island', depth: 0.11, back: 'glass', cameraControl: false, smartConnector: false,
    finishes: [f('blue', 'Blue', '#c5d6e3'), f('pink', 'Pink', '#f2cbd3'), f('green', 'Green', '#cfdccb'), ...NEUTRAL] } },
  { match: /iPhone-1[12]-Pro/, spec: { camera: 'pro-square', front: 'notch', depth: 0.11, back: 'glass', cameraControl: false, smartConnector: false,
    finishes: [f('pacific-blue', 'Pacific Blue', '#2f4b5e'), f('gold', 'Gold', '#e9d8bd'), f('graphite', 'Graphite', '#4a4a4c'), f('silver', 'Silver', '#e2e1dd')] } },
  { match: /iPhone-1[12]|iPhone-XR/, spec: { camera: 'dual-vertical', front: 'notch', depth: 0.11, back: 'glass', cameraControl: false, smartConnector: false,
    finishes: [f('blue', 'Blue', '#2e5c9a'), f('green', 'Green', '#d3e5d3'), f('purple', 'Purple', '#cfc7e8'), f('red', 'Red', '#b41f2b'), ...NEUTRAL] } },
  { match: /iPhone-X/, spec: { camera: 'dual-vertical', front: 'notch', depth: 0.11, back: 'glass', cameraControl: false, smartConnector: false,
    finishes: [f('space-gray', 'Space Gray', '#3b3b3d'), f('silver', 'Silver', '#e4e3df'), f('gold', 'Gold', '#f1dcc6')] } },
  { match: /iPhone/, spec: { camera: 'single', front: 'none', depth: 0.108, back: 'glass', cameraControl: false, smartConnector: false,
    finishes: [f('midnight', 'Midnight', '#26282e'), f('starlight', 'Starlight', '#efe9e0'), f('red', 'Red', '#b41f2b')] } },
  { match: /iPad-Pro.*M[45]/, spec: { camera: 'pad-pro', front: 'pad', depth: 0.028, back: 'metal', cameraControl: false, smartConnector: true,
    finishes: [f('space-black', 'Space Black', '#2c2c2e'), f('silver', 'Silver', '#e2e3e4')] } },
  { match: /iPad-Pro/, spec: { camera: 'pad-pro', front: 'pad', depth: 0.033, back: 'metal', cameraControl: false, smartConnector: true,
    finishes: [f('space-gray', 'Space Gray', '#5b5c60'), f('silver', 'Silver', '#e2e3e4')] } },
  { match: /iPad-Air/, spec: { camera: 'pad-single', front: 'pad', depth: 0.034, back: 'metal', cameraControl: false, smartConnector: true,
    finishes: [f('space-gray', 'Space Gray', '#5d5e62'), f('blue', 'Blue', '#a8b9cf'), f('purple', 'Purple', '#c5bcd8'), f('starlight', 'Starlight', '#e3ddd1')] } },
  { match: /iPad-mini/, spec: { camera: 'pad-single', front: 'pad', depth: 0.047, back: 'metal', cameraControl: false, smartConnector: false,
    finishes: [f('space-gray', 'Space Gray', '#5d5e62'), f('blue', 'Blue', '#a8b9cf'), f('purple', 'Purple', '#c5bcd8'), f('starlight', 'Starlight', '#e3ddd1')] } },
  { match: /iPad/, spec: { camera: 'pad-single', front: 'pad', depth: 0.039, back: 'metal', cameraControl: false, smartConnector: false,
    finishes: [f('blue', 'Blue', '#9ebfdf'), f('pink', 'Pink', '#efb8c2'), f('yellow', 'Yellow', '#f2db78'), f('silver', 'Silver', '#e2e3e4')] } },
];

/** The foldable iPhone Duo, by device type (`…SimDeviceType.iPhone-Duo`, product code V68). */
export const isDuo = (deviceType: string): boolean => /iPhone-Duo|V68/.test(deviceType);

/** The hardware of a simulated device type, e.g. `com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro`. */
export function modelSpec(deviceType: string, family: 'phone' | 'pad'): ModelSpec {
  const key = deviceType.split('.').pop() ?? '';
  const found = SPECS.find(entry => entry.match.test(key)) ?? SPECS.find(entry => entry.match.test(family === 'pad' ? 'iPad' : 'iPhone'))!;
  // An Android tablet without a recognized profile still gets tablet hardware.
  if (key.startsWith('android:') && family === 'pad' && !found.match.source.includes('tablet')) return { key, ...SPECS[0].spec };
  // Every Duo device type shares one finish memory.
  return { key: isDuo(key) ? 'iPhone-Duo' : key, ...found.spec };
}

const FINISH_STORE = 'pandacode.simulatorFinish';
/** The remembered finish for a model, else its launch color. */
export function chosenFinish(spec: ModelSpec): Finish {
  let saved: Record<string, string> = {};
  try { saved = JSON.parse(localStorage.getItem(FINISH_STORE) ?? '{}') ?? {}; } catch { /* First finish. */ }
  return spec.finishes.find(finish => finish.id === saved[spec.key]) ?? spec.finishes[0];
}
export function rememberFinish(spec: ModelSpec, finish: Finish): void {
  try {
    const saved = JSON.parse(localStorage.getItem(FINISH_STORE) ?? '{}') ?? {};
    localStorage.setItem(FINISH_STORE, JSON.stringify({ ...saved, [spec.key]: finish.id }));
  } catch { /* This session keeps it. */ }
}
