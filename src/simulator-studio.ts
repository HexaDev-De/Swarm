import * as THREE from 'three';

/* A product-photography studio for the simulator: what the surfaces reflect
 * (a soft overhead box, tall strip lights at the sides, a dim rim behind) and a
 * floor that only shows the device's soft shadow. Strip lights are what give
 * metal edges and glass their long, crisp highlights. */

function lightformer(scene: THREE.Scene, width: number, height: number, intensity: number, position: [number, number, number], target: [number, number, number] = [0, 0, 0], tint = 0xffffff): void {
  const material = new THREE.MeshBasicMaterial({ color: new THREE.Color(tint).multiplyScalar(intensity), side: THREE.DoubleSide, toneMapped: false });
  const panel = new THREE.Mesh(new THREE.PlaneGeometry(width, height), material);
  panel.position.set(...position);
  panel.lookAt(...target);
  scene.add(panel);
}

/** A prefiltered studio environment map; dispose the returned texture with the renderer. */
export function studioEnvironment(renderer: THREE.WebGLRenderer): THREE.Texture {
  const studio = new THREE.Scene();
  studio.background = new THREE.Color(0x2a2a2e);
  // A soft, warm-neutral dome: product shots never reflect pure black.
  const dome = new THREE.Mesh(new THREE.SphereGeometry(30, 32, 16), new THREE.MeshBasicMaterial({ color: new THREE.Color(0x6c6c72), side: THREE.BackSide, toneMapped: false }));
  studio.add(dome);
  lightformer(studio, 14, 7, 3.4, [0, 9, 3]);                        // overhead softbox
  lightformer(studio, 2, 16, 8, [-8, 1, 3]);                        // left strip: the long edge highlight
  lightformer(studio, 2, 16, 6, [8, 0, 1]);                         // right strip
  lightformer(studio, 16, 2, 3, [0, -2, -9], [0, 0, 0], 0xfff1df);  // warm rim behind
  lightformer(studio, 10, 7, 4.5, [4.5, 5, -12]);                      // big softbox behind, upper left as seen from the back: the back's soft diagonal gradient
  lightformer(studio, 20, 14, 1.3, [0, 0, 12]);                     // broad fill behind the camera
  lightformer(studio, 30, 30, 0.3, [0, -10, 0], [0, 0, 0], 0xf0ebe2); // bright floor bounce
  const pmrem = new THREE.PMREMGenerator(renderer);
  const texture = pmrem.fromScene(studio, 0.015).texture;
  pmrem.dispose();
  studio.traverse(node => { if (node instanceof THREE.Mesh) { node.geometry.dispose(); (node.material as THREE.Material).dispose(); } });
  return texture;
}

/** Shadow-only floor under the device, lit by one soft key light. */
export function studioFloor(scene: THREE.Scene): { key: THREE.DirectionalLight; place(bottom: number, span: number): void; dispose(): void } {
  // Direct light on top of the reflections, so flat faces read bright too.
  const hemisphere = new THREE.HemisphereLight(0xffffff, 0xd9d4cc, 0.45);
  const fill = new THREE.DirectionalLight(0xfff8f0, 0.9); fill.position.set(-6, 3, 6);
  const back = new THREE.DirectionalLight(0xffffff, 0.45); back.position.set(3, 4, -7);
  scene.add(hemisphere, fill, back);
  const key = new THREE.DirectionalLight(0xffffff, 2.0);
  key.position.set(2.5, 9, 4.5);
  key.castShadow = true;
  key.shadow.mapSize.set(1024, 1024);
  key.shadow.radius = 9;
  key.shadow.bias = -0.0004;
  key.shadow.blurSamples = 16;
  scene.add(key, key.target);
  const floor = new THREE.Mesh(new THREE.PlaneGeometry(40, 40), new THREE.ShadowMaterial({ opacity: 0.22, transparent: true }));
  floor.rotation.x = -Math.PI / 2;
  floor.receiveShadow = true;
  scene.add(floor);
  return {
    key,
    place(bottom, span) {
      floor.position.y = bottom - span * 0.06;
      const camera = key.shadow.camera;
      camera.left = camera.bottom = -span; camera.right = camera.top = span; camera.near = 0.5; camera.far = span * 6 + 20;
      camera.updateProjectionMatrix();
    },
    dispose() { floor.geometry.dispose(); (floor.material as THREE.Material).dispose(); key.dispose(); fill.dispose(); back.dispose(); hemisphere.dispose(); },
  };
}
