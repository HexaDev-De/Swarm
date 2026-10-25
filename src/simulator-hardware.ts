import * as THREE from 'three';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';
import type { Finish, ModelSpec } from './simulator-models';

/* Meshes for one device's hardware: back camera layout, finish materials, front
 * cutouts and side buttons. Sizes are shares of the body width (`w`), taken from
 * product photography; the back faces −Z, so "top left from behind" is +X. */

export type Body = { width: number; height: number; radius: number; depth: number };
export type Screen = { width: number; height: number; side: number; cap: number };
export type Hardware = { buttons: Map<THREE.Object3D, 'home' | 'lock'>; materials: THREE.Material[] };

/** A rounded rectangle with separate top and bottom corner radii, centered. */
export function roundedRect(width: number, height: number, top: number, bottom = top): THREE.Shape {
  const x = -width / 2, y = -height / 2, t = Math.min(top, width / 2, height / 2), b = Math.min(bottom, width / 2, height / 2), shape = new THREE.Shape();
  shape.moveTo(x + b, y);
  shape.lineTo(x + width - b, y); shape.quadraticCurveTo(x + width, y, x + width, y + b);
  shape.lineTo(x + width, y + height - t); shape.quadraticCurveTo(x + width, y + height, x + width - t, y + height);
  shape.lineTo(x + t, y + height); shape.quadraticCurveTo(x, y + height, x, y + height - t);
  shape.lineTo(x, y + b); shape.quadraticCurveTo(x, y, x + b, y);
  return shape;
}
/** A flat rounded rectangle whose UVs span 0–1 across its bounds. */
export function plate(width: number, height: number, radius: number, bottom = radius): THREE.ShapeGeometry {
  const geometry = new THREE.ShapeGeometry(roundedRect(width, height, radius, bottom), 24);
  const position = geometry.attributes.position, uv = geometry.attributes.uv;
  for (let i = 0; i < position.count; i++) uv.setXY(i, position.getX(i) / width + 0.5, position.getY(i) / height + 0.5);
  return geometry;
}
/** A raised slab on the back: extruded toward −Z from the back face. */
function slab(width: number, height: number, top: number, bottom: number, thickness: number): THREE.ExtrudeGeometry {
  const bevel = Math.min(thickness * 0.45, 0.06);
  const geometry = new THREE.ExtrudeGeometry(roundedRect(width - bevel * 2, height - bevel * 2, Math.max(top - bevel, 0.001), Math.max(bottom - bevel, 0.001)), { depth: Math.max(thickness - bevel * 2, 0.001), bevelEnabled: true, bevelThickness: bevel, bevelSize: bevel, bevelSegments: 5, curveSegments: 32 });
  geometry.rotateY(Math.PI);
  return geometry;
}

export function buildHardware(device: THREE.Group, spec: ModelSpec, finish: Finish, body: Body, screen: Screen, front: THREE.Material): Hardware {
  const w = body.width, H = body.height, back = -body.depth / 2, pad = spec.front === 'pad';
  const color = new THREE.Color(finish.color);
  const light = color.getHSL({ h: 0, s: 0, l: 0 }).l > 0.6;
  // Anodized aluminum is satin; colored glass is frosted with a clear top coat.
  // The frame is brushed along its length (anisotropic), so the strip lights stretch into long edge highlights.
  const metal = new THREE.MeshPhysicalMaterial({ color, metalness: 0.88, roughness: 0.3, anisotropy: 0.5, anisotropyRotation: Math.PI / 2, clearcoat: 0.3, clearcoatRoughness: 0.2, envMapIntensity: 1.25 });
  // Bead-blasted anodizing: a fine satin with a faint lacquer sheen.
  const satin = new THREE.MeshPhysicalMaterial({ color, metalness: 0.78, roughness: 0.38, clearcoat: 0.35, clearcoatRoughness: 0.4, sheen: 0.2, sheenRoughness: 0.6, sheenColor: color.clone().offsetHSL(0, -0.1, 0.15), envMapIntensity: 1.25 });
  // Glass in the finish reads a touch softer than the metal around it, never washed out.
  const frosted = new THREE.MeshPhysicalMaterial({ color: color.clone().offsetHSL(0, -0.04, light ? -0.01 : 0.004), metalness: 0.3, roughness: 0.55, clearcoat: 0.3, clearcoatRoughness: 0.65, envMapIntensity: 0.85 });
  // Sapphire over the optics: neutral black with gray elements and only a hint of coating color.
  const lensGlass = new THREE.MeshPhysicalMaterial({ color: 0x060607, metalness: 0, roughness: 0.06, clearcoat: 0.6, clearcoatRoughness: 0.05, envMapIntensity: 0.3 });
  // The lens face reads as deep coated sapphire from any angle; the rings around it carry the reflections.
  const lensFace = new THREE.MeshBasicMaterial({ color: 0x08080a });
  const lensInner = new THREE.MeshPhysicalMaterial({ color: 0x141518, metalness: 0.4, roughness: 0.25, envMapIntensity: 0.2 });
  const lensElement = new THREE.MeshPhysicalMaterial({ color: 0x1e1f23, metalness: 0.5, roughness: 0.3, envMapIntensity: 0.22 });
  // Polished rings around each lens, in the finish.
  const ringMetal = new THREE.MeshPhysicalMaterial({ color, metalness: 0.8, roughness: 0.22, clearcoat: 0.6, clearcoatRoughness: 0.15, envMapIntensity: 1.2 });
  const ringDark = new THREE.MeshStandardMaterial({ color: 0x1b1c20, metalness: 0.7, roughness: 0.35 });
  const flash = new THREE.MeshPhysicalMaterial({ color: 0xf4f1ea, metalness: 0, roughness: 0.2, clearcoat: 1 });
  const flashCore = new THREE.MeshPhysicalMaterial({ color: 0xc9c5bd, metalness: 0.2, roughness: 0.3, clearcoat: 1 });
  const subtle = new THREE.MeshBasicMaterial({ color: color.clone().offsetHSL(0, 0, light ? -0.08 : 0.07), transparent: true, opacity: 0.35, depthWrite: false });
  const antenna = new THREE.MeshPhysicalMaterial({ color: color.clone().offsetHSL(0, -0.15, light ? -0.18 : 0.06), metalness: 0, roughness: 0.55, clearcoat: 0.3 });
  const hole = new THREE.MeshBasicMaterial({ color: 0x050505 });
  const materials: THREE.Material[] = [metal, satin, frosted, lensGlass, lensInner, lensElement, lensFace, ringMetal, ringDark, flash, flashCore, subtle, antenna, hole];
  const buttons = new Map<THREE.Object3D, 'home' | 'lock'>();
  const add = (mesh: THREE.Mesh, x: number, y: number, z: number) => { mesh.position.set(x, y, z); device.add(mesh); return mesh; };
  const faceBack = (geometry: THREE.BufferGeometry) => { geometry.rotateY(Math.PI); return geometry; };
  const disc = (radius: number, material: THREE.Material, x: number, y: number, z: number) => add(new THREE.Mesh(faceBack(new THREE.CircleGeometry(radius, 48)), material), x, y, z);

  /** One camera: a raised ring with a rounded lip in the finish, sapphire glass, and the optics behind it. */
  const lens = (x: number, y: number, z: number, r: number, housing: THREE.Material = ringMetal) => {
    const height = w * 0.034, outer = r * 1.2, lip = r * 0.07;
    const wall = new THREE.CylinderGeometry(outer, outer * 1.03, height, 64, 1, true); wall.rotateX(Math.PI / 2);
    add(new THREE.Mesh(wall, housing), x, y, z - height / 2);
    const rim = new THREE.TorusGeometry(outer - lip, lip, 16, 64); rim.rotateY(Math.PI);
    add(new THREE.Mesh(rim, housing), x, y, z - height);
    add(new THREE.Mesh(faceBack(new THREE.RingGeometry(r, outer - lip, 64)), housing), x, y, z - height + 0.001);
    disc(r, lensFace, x, y, z - height + 0.004);
    add(new THREE.Mesh(faceBack(new THREE.RingGeometry(r * 0.5, r * 0.62, 64)), lensElement), x, y, z - height + 0.0035);
    disc(r * 0.3, lensInner, x, y, z - height + 0.003);
  };
  const smallDot = (x: number, y: number, z: number, r: number, material: THREE.Material) => disc(r, material, x, y, z);

  // The body's back: metal or glass in the finish, as one plate over the slab's back cap.
  const backPlate = new THREE.Mesh(faceBack(plate(w - 0.05, H - 0.05, body.radius - 0.025)), spec.back === 'glass' ? frosted : satin);
  add(backPlate, 0, 0, back - 0.002);

  const top = H / 2;
  switch (spec.camera) {
    case 'pro-plateau': {
      // Measured from Apple's back view, in shares of the body width from the top-left (seen from behind).
      const h = w * 0.64, t = w * 0.034, y = top - h / 2 - w * 0.01, z = back - t;
      add(new THREE.Mesh(slab(w - w * 0.016, h, body.radius - w * 0.008, w * 0.1, t), satin), 0, y, back - 0.002);
      const at = (fromLeft: number, fromTop: number): [number, number] => [w / 2 - w * fromLeft, top - w * fromTop];
      for (const [left, down] of [[0.205, 0.215], [0.205, 0.47], [0.455, 0.345]]) { const [lx, ly] = at(left, down); lens(lx, ly, z, w * 0.1); }
      const [fx, fy] = at(0.83, 0.2), [dx, dy] = at(0.83, 0.48), [mx, my] = at(0.815, 0.335);
      smallDot(fx, fy, z - 0.002, w * 0.042, flash);
      smallDot(fx, fy, z - 0.0025, w * 0.024, flashCore);
      smallDot(dx, dy, z - 0.002, w * 0.046, lensGlass);
      smallDot(mx, my, z - 0.002, w * 0.008, hole);
      // The glass panel on the lower back: set into the aluminum, a shade softer than it.
      const panelTop = y - h / 2 - w * 0.035, panelBottom = -top + w * 0.1, panelHeight = panelTop - panelBottom;
      add(new THREE.Mesh(faceBack(plate(w - w * 0.12, panelHeight, w * 0.1, body.radius - w * 0.1)), frosted), w * 0.008, (panelTop + panelBottom) / 2, back - 0.004);
      break;
    }
    case 'air-plateau': {
      const h = w * 0.3, t = w * 0.03, y = top - h / 2 - w * 0.02;
      add(new THREE.Mesh(slab(w - w * 0.04, h, body.radius - w * 0.02, w * 0.12, t), satin), 0, y, back - 0.002);
      lens(w / 2 - w * 0.2, y, back - t, w * 0.1);
      smallDot(w / 2 - w * 0.4, y + w * 0.04, back - t - 0.002, w * 0.035, flash);
      smallDot(-w / 2 + w * 0.25, y, back - t - 0.002, w * 0.007, ringDark);
      break;
    }
    case 'capsule': {
      const pw = w * 0.27, ph = w * 0.55, t = w * 0.02, x = w / 2 - w * 0.2, y = top - ph / 2 - w * 0.08;
      add(new THREE.Mesh(slab(pw, ph, pw / 2, pw / 2, t), frosted), x, y, back - 0.002);
      lens(x, y + ph * 0.23, back - t, w * 0.095); lens(x, y - ph * 0.23, back - t, w * 0.095);
      smallDot(x - pw / 2 - w * 0.08, y + ph * 0.2, back - 0.004, w * 0.03, flash);
      smallDot(x - pw / 2 - w * 0.08, y - ph * 0.02, back - 0.004, w * 0.007, ringDark);
      break;
    }
    case 'pro-square': case 'dual-diagonal': case 'dual-vertical': case 'single-square': {
      const size = w * (spec.camera === 'pro-square' ? 0.52 : spec.camera === 'single-square' ? 0.3 : 0.44), t = w * 0.022;
      const x = w / 2 - size / 2 - w * 0.06, y = top - size / 2 - w * 0.06, z = back - t;
      add(new THREE.Mesh(slab(size, size, w * 0.12, w * 0.12, t), frosted), x, y, back - 0.002);
      const r = w * (spec.camera === 'pro-square' ? 0.1 : 0.085), q = size * 0.24;
      if (spec.camera === 'pro-square') {
        lens(x + q, y + q, z, r); lens(x + q, y - q, z, r); lens(x - q, y, z, r);
        smallDot(x - q, y + q * 1.05, z - 0.002, w * 0.032, flash); smallDot(x - q, y - q * 1.05, z - 0.002, w * 0.04, lensGlass);
      } else if (spec.camera === 'dual-diagonal') {
        lens(x + q, y + q, z, r); lens(x - q, y - q, z, r); smallDot(x - q, y + q, z - 0.002, w * 0.035, flash);
      } else if (spec.camera === 'dual-vertical') {
        lens(x + q, y + q, z, r); lens(x + q, y - q, z, r); smallDot(x - q, y + q, z - 0.002, w * 0.035, flash);
      } else {
        lens(x + size * 0.18, y + size * 0.18, z, r); smallDot(x - size * 0.22, y - size * 0.2, z - 0.002, w * 0.03, flash);
      }
      break;
    }
    case 'single': {
      const x = w / 2 - w * 0.17, y = top - w * 0.17;
      lens(x, y, back + w * 0.012, w * 0.075, ringDark);
      smallDot(x - w * 0.17, y, back - 0.004, w * 0.035, flash);
      break;
    }
    case 'pad-pro': {
      const size = w * 0.16, t = w * 0.012, x = w / 2 - size / 2 - w * 0.04, y = top - size / 2 - w * 0.04, z = back - t;
      add(new THREE.Mesh(slab(size, size, size * 0.3, size * 0.3, t), satin), x, y, back - 0.002);
      lens(x + size * 0.2, y + size * 0.2, z, w * 0.035); smallDot(x + size * 0.2, y - size * 0.22, z - 0.002, w * 0.022, lensGlass);
      smallDot(x - size * 0.22, y + size * 0.22, z - 0.002, w * 0.016, flash);
      break;
    }
    case 'pixel-pill': {
      // Pixel 9: a raised, fully rounded camera island floating across the upper back.
      const pw = w * 0.8, ph = w * 0.24, t = w * 0.045, y = top - w * 0.2;
      add(new THREE.Mesh(slab(pw, ph, ph / 2, ph / 2, t), satin), 0, y, back - 0.002);
      const z = back - t;
      add(new THREE.Mesh(faceBack(plate(pw * 0.62, ph * 0.74, ph * 0.37)), lensFace), w * 0.08, y, z - 0.002);
      lens(w * 0.27, y, z, w * 0.075, ringDark); lens(w * 0.06, y, z, w * 0.075, ringDark);
      smallDot(-w * 0.16, y + ph * 0.18, z - 0.003, w * 0.03, flash);
      smallDot(-w * 0.16, y - ph * 0.2, z - 0.003, w * 0.018, lensGlass);
      break;
    }
    case 'pixel-bar': {
      // Pixel 6–8: an edge-to-edge camera visor with a black glass window.
      const ph = w * 0.22, t = w * 0.04, y = top - w * 0.3;
      add(new THREE.Mesh(slab(w + 0.01, ph, ph * 0.45, ph * 0.45, t), metal), 0, y, back - 0.002);
      const z = back - t;
      add(new THREE.Mesh(faceBack(plate(w * 0.62, ph * 0.72, ph * 0.36)), lensFace), w * 0.1, y, z - 0.002);
      lens(w * 0.3, y, z, w * 0.072, ringDark); lens(w * 0.1, y, z, w * 0.072, ringDark);
      smallDot(-w * 0.12, y, z - 0.003, w * 0.028, flash);
      break;
    }
    case 'android-square': {
      const size = w * 0.42, t = w * 0.03, x = w / 2 - size / 2 - w * 0.07, y = top - size / 2 - w * 0.07, z = back - t;
      add(new THREE.Mesh(slab(size, size, w * 0.09, w * 0.09, t), frosted), x, y, back - 0.002);
      const q = size * 0.24;
      lens(x + q, y + q, z, w * 0.08, ringDark); lens(x + q, y - q, z, w * 0.08, ringDark); lens(x - q, y + q, z, w * 0.06, ringDark);
      smallDot(x - q, y - q, z - 0.002, w * 0.03, flash);
      break;
    }
    case 'pad-single': {
      lens(w / 2 - w * 0.07, top - w * 0.07, back + w * 0.01, w * 0.035, satin);
      break;
    }
  }
  if (spec.smartConnector) for (const dx of [-1, 0, 1]) smallDot(dx * w * 0.025, -top + w * 0.035, back - 0.004, w * 0.0065, ringDark);

  // Front: the cutout in the display, or the iPad's bezel camera.
  const face = body.depth / 2;
  // The notch hangs from the display's top edge: square where it meets the frame, rounded below.
  if (spec.front === 'notch') add(new THREE.Mesh(plate(1.62, 0.32, 0, 0.17), front), 0, screen.height / 2 - 0.16, face + 0.0065);
  // Earpiece: a slit in the glass along the top edge.
  if (!pad) add(new THREE.Mesh(plate(0.62, 0.035, 0.0175), hole), 0, screen.height / 2 + Math.min(screen.cap * 0.45, 0.08), face + 0.0045);
  if (pad) disc(0.045, lensGlass, 0, screen.height / 2 + screen.cap / 2, face + 0.005).rotation.y = Math.PI;

  // Side buttons in the finish; Camera Control is flush sapphire in a metal ring.
  const edge = body.width / 2, d = body.depth * 0.42;
  const key = (width: number, height: number, depth: number, x: number, y: number, z = 0, material: THREE.Material = metal) => {
    const mesh = new THREE.Mesh(new RoundedBoxGeometry(width, height, depth, 3, Math.min(width, height, depth) / 2.2), material);
    return add(mesh, x, y, z);
  };
  if (pad) {
    buttons.set(key(0.9, 0.08, d, edge - 1.3, H / 2 + 0.02), 'lock');
    key(0.65, 0.08, d, edge - 2.4, H / 2 + 0.02); key(0.65, 0.08, d, edge - 3.2, H / 2 + 0.02);
  } else if (spec.android) {
    // Android phones: power above the volume rocker, both on the right edge.
    const sh = screen.height;
    buttons.set(key(0.08, 0.62, d, edge + 0.02, sh * 0.22), 'lock');
    key(0.08, 1.15, d, edge + 0.02, sh * 0.02);
  } else {
    const sh = screen.height;
    buttons.set(key(0.08, 1.0, d, edge + 0.02, sh * 0.17), 'lock');
    key(0.08, 0.34, d, -edge - 0.02, sh * 0.31); key(0.08, 0.62, d, -edge - 0.02, sh * 0.19); key(0.08, 0.62, d, -edge - 0.02, sh * 0.07);
    if (spec.cameraControl) {
      key(0.05, 1.1, d * 0.9, edge + 0.005, -sh * 0.13, 0, ringDark);
      key(0.04, 0.98, d * 0.7, edge + 0.025, -sh * 0.13, 0, lensGlass);
    }
  }
  // Antenna bands: thin non-metal breaks in the frame, near each corner.
  if (spec.back !== 'glass' || spec.camera === 'pro-square') {
    const band = (x: number, y: number, horizontal: boolean) => add(new THREE.Mesh(new THREE.BoxGeometry(horizontal ? 0.035 : 0.028, horizontal ? 0.028 : 0.035, body.depth * 0.62), antenna), x, y, 0);
    const inset = Math.min(0.8, H * 0.09);
    for (const sx of [-1, 1]) for (const sy of [-1, 1]) band(sx * (body.width / 2 + 0.002), sy * (H / 2 - inset), false);
    for (const sy of [-1, 1]) band(-(body.width / 2 - inset), sy * (H / 2 + 0.002), true);
  }
  // Bottom edge: the USB-C port between speaker holes (iPads also have speakers along the top).
  const bottom = -H / 2 - 0.001, port = new THREE.Mesh(plate(pad ? 0.5 : 0.42, Math.min(body.depth * 0.38, 0.13), 0.06), hole);
  port.rotation.x = Math.PI / 2; add(port, 0, bottom, 0);
  const holes = (y: number, from: number, count: number, gap: number) => {
    for (let i = 0; i < count; i++) for (const side of [-1, 1]) {
      const dot = new THREE.Mesh(new THREE.CircleGeometry(Math.min(body.depth * 0.09, 0.026), 16), hole);
      dot.rotation.x = y < 0 ? Math.PI / 2 : -Math.PI / 2; add(dot, side * (from + i * gap), y, 0);
    }
  };
  holes(bottom, pad ? 0.9 : 0.45, pad ? 7 : 5, pad ? 0.09 : 0.075);
  if (pad) holes(H / 2 + 0.001, 0.9, 7, 0.09);
  return { buttons, materials };
}
