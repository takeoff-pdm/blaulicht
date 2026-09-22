/**
 * The 3D stage.
 *
 * Everything the rig needs is drawn with four instanced meshes — housings,
 * lenses, beams and floor pools — so a 500-fixture showfile costs a handful of
 * draw calls and the frame budget goes to the post-processing chain instead.
 */

import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { OutputPass } from 'three/addons/postprocessing/OutputPass.js';
import { SMAAPass } from 'three/addons/postprocessing/SMAAPass.js';

import type { Layout, PlacedFixture } from '../core/layout.js';
import type { Show } from '../core/showfile.js';
import type { FixtureState } from '../core/profiles.js';
import {
  BEAM_FRAGMENT,
  BEAM_VERTEX,
  POOL_FRAGMENT,
  POOL_VERTEX,
} from './shaders.js';

export interface StageSettings {
  bloom: boolean;
  bloomStrength: number;
  beams: boolean;
  pools: boolean;
  room: boolean;
  haze: number;
  exposure: number;
  /** Beam brightness multiplier. */
  gain: number;
}

export const DEFAULT_SETTINGS: StageSettings = {
  bloom: true,
  bloomStrength: 0.42,
  beams: true,
  pools: true,
  room: true,
  haze: 0.65,
  exposure: 1.0,
  gain: 0.6,
};

/** Cone half-angles in radians, by how the fixture projects light. */
const HEAD_ANGLE = THREE.MathUtils.degToRad(7.5);
// A typical par is a 25-30 deg full beam; a wide cone here turns a dense rig
// into one undifferentiated white field.
const WASH_ANGLE = THREE.MathUtils.degToRad(14);

const MAX_BEAM_LENGTH = 40;
const UP = new THREE.Vector3(0, -1, 0);
const AXIS_X = new THREE.Vector3(1, 0, 0);
const AXIS_Y = new THREE.Vector3(0, 1, 0);

function srgbToLinear(c: number): number {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
}

export class Stage {
  readonly settings: StageSettings = { ...DEFAULT_SETTINGS };

  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera: THREE.PerspectiveCamera;
  private readonly controls: OrbitControls;
  private readonly composer: EffectComposer;
  private readonly bloomPass: UnrealBloomPass;
  private readonly renderPass: RenderPass;

  private rigGroup = new THREE.Group();
  private roomGroup = new THREE.Group();

  private bodies: THREE.InstancedMesh | null = null;
  private lenses: THREE.InstancedMesh | null = null;
  private beams: THREE.InstancedMesh | null = null;
  private pools: THREE.InstancedMesh | null = null;

  private beamColors: THREE.InstancedBufferAttribute | null = null;
  private beamIntensity: THREE.InstancedBufferAttribute | null = null;
  private poolColors: THREE.InstancedBufferAttribute | null = null;
  private poolIntensity: THREE.InstancedBufferAttribute | null = null;

  private beamMaterial: THREE.ShaderMaterial;
  private poolMaterial: THREE.ShaderMaterial;

  private fixtures: PlacedFixture[] = [];
  private selection = -1;
  private selectionMarker: THREE.Mesh;

  // Scratch objects, reused every frame to keep the render loop allocation-free.
  private readonly scratch = {
    matrix: new THREE.Matrix4(),
    quat: new THREE.Quaternion(),
    dir: new THREE.Vector3(),
    pos: new THREE.Vector3(),
    scale: new THREE.Vector3(),
    color: new THREE.Color(),
    euler: new THREE.Euler(),
    raycaster: new THREE.Raycaster(),
    pointer: new THREE.Vector2(),
  };

  constructor(private readonly canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: false, // SMAA in the composer instead; cheaper with bloom.
      powerPreference: 'high-performance',
      stencil: false,
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = this.settings.exposure;

    this.scene.background = new THREE.Color(0x05060a);
    this.scene.fog = new THREE.FogExp2(0x05060a, 0.012);

    this.camera = new THREE.PerspectiveCamera(50, 1, 0.1, 500);
    this.camera.position.set(0, 6, 18);

    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    this.controls.maxPolarAngle = Math.PI * 0.495; // never go under the floor
    this.controls.target.set(0, 3, 0);

    this.scene.add(this.rigGroup, this.roomGroup);

    // The rig is mostly self-lit; ambient exists only so housings are not
    // silhouettes when everything is blacked out.
    this.scene.add(new THREE.AmbientLight(0x2a3344, 0.5));
    const key = new THREE.DirectionalLight(0x5a6a88, 0.35);
    key.position.set(6, 12, 8);
    this.scene.add(key);

    this.beamMaterial = new THREE.ShaderMaterial({
      vertexShader: BEAM_VERTEX,
      fragmentShader: BEAM_FRAGMENT,
      uniforms: {
        uTime: { value: 0 },
        uHaze: { value: this.settings.haze },
        uGain: { value: this.settings.gain },
      },
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      side: THREE.DoubleSide,
    });

    this.poolMaterial = new THREE.ShaderMaterial({
      vertexShader: POOL_VERTEX,
      fragmentShader: POOL_FRAGMENT,
      uniforms: { uGain: { value: this.settings.gain } },
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      side: THREE.DoubleSide,
    });

    this.selectionMarker = new THREE.Mesh(
      new THREE.RingGeometry(0.35, 0.42, 32),
      new THREE.MeshBasicMaterial({ color: 0x66ccff, side: THREE.DoubleSide }),
    );
    this.selectionMarker.visible = false;
    this.scene.add(this.selectionMarker);

    this.composer = new EffectComposer(this.renderer);
    this.renderPass = new RenderPass(this.scene, this.camera);
    this.composer.addPass(this.renderPass);

    this.bloomPass = new UnrealBloomPass(
      new THREE.Vector2(1, 1),
      this.settings.bloomStrength,
      0.4, // radius
      // Threshold high enough that only lens hotspots and beam cores bloom.
      // Low values here smear the whole rig into one white field.
      0.6,
    );
    this.composer.addPass(this.bloomPass);
    this.composer.addPass(new SMAAPass());
    this.composer.addPass(new OutputPass());

    this.resize();
  }

  //
  // Scene construction.
  //

  setShow(show: Show, layout: Layout): void {
    this.disposeGroup(this.rigGroup);
    this.disposeGroup(this.roomGroup);
    this.fixtures = layout.fixtures;
    this.selection = -1;
    this.selectionMarker.visible = false;

    this.buildRoom(show, layout);
    this.buildRig(layout);
    this.frameCamera(layout);
  }

  private disposeGroup(group: THREE.Group): void {
    group.traverse((obj) => {
      const mesh = obj as THREE.Mesh;
      if (mesh.geometry) mesh.geometry.dispose();
      const material = mesh.material;
      if (Array.isArray(material)) material.forEach((m) => m.dispose());
      // Shared shader materials are owned by the Stage, not by the mesh.
      else if (material && material !== this.beamMaterial && material !== this.poolMaterial) {
        material.dispose();
      }
    });
    group.clear();
    if (group === this.rigGroup) {
      this.bodies = this.lenses = this.beams = this.pools = null;
    }
  }

  private buildRoom(show: Show, layout: Layout): void {
    const { room } = show;
    // Grow the room if the generated rig is wider than the configured stage.
    const width = Math.max(room.width, (layout.bounds.max.x - layout.bounds.min.x) * 1.2, 4);
    const depth = Math.max(room.depth, (layout.bounds.max.z - layout.bounds.min.z) * 1.6, 4);
    const height = Math.max(room.height, layout.bounds.max.y * 1.15, 3);

    const floorColor = new THREE.Color(
      srgbToLinear(room.floorColor[0]),
      srgbToLinear(room.floorColor[1]),
      srgbToLinear(room.floorColor[2]),
    );
    const wallColor = new THREE.Color(
      srgbToLinear(room.wallColor[0]),
      srgbToLinear(room.wallColor[1]),
      srgbToLinear(room.wallColor[2]),
    );

    const floor = new THREE.Mesh(
      new THREE.PlaneGeometry(width, depth),
      new THREE.MeshStandardMaterial({
        color: floorColor,
        roughness: 0.55,
        metalness: 0.15,
      }),
    );
    floor.rotation.x = -Math.PI / 2;
    this.roomGroup.add(floor);

    // Walls: a box seen from the inside, open at the front so the camera can
    // orbit without clipping through geometry.
    const walls = new THREE.Mesh(
      new THREE.BoxGeometry(width, height, depth),
      new THREE.MeshStandardMaterial({
        color: wallColor,
        roughness: 0.9,
        metalness: 0.0,
        side: THREE.BackSide,
        transparent: true,
        opacity: 0.55,
      }),
    );
    walls.position.y = height / 2;
    this.roomGroup.add(walls);

    const grid = new THREE.GridHelper(Math.max(width, depth), Math.round(Math.max(width, depth)), 0x2a3550, 0x161d2e);
    grid.position.y = 0.01;
    this.roomGroup.add(grid);

    // A truss bar per hung row makes the rig read as a real hang. Fixtures in
    // one group can sit at different trims (heads above washes, hazers on the
    // deck), so rows are keyed by group *and* height, and floor-level fixtures
    // get no truss at all.
    if (layout.generated) {
      interface Row { minX: number; maxX: number; y: number; z: number }
      const rows = new Map<string, Row>();
      for (const f of layout.fixtures) {
        if (f.world.y < height * 0.25) continue;
        const key = `${f.groupKey}:${f.world.y.toFixed(3)}`;
        const row = rows.get(key);
        if (!row) {
          rows.set(key, { minX: f.world.x, maxX: f.world.x, y: f.world.y, z: f.world.z });
        } else {
          row.minX = Math.min(row.minX, f.world.x);
          row.maxX = Math.max(row.maxX, f.world.x);
        }
      }
      const trussMaterial = new THREE.MeshStandardMaterial({
        color: 0x3a4152,
        roughness: 0.4,
        metalness: 0.8,
      });
      for (const row of rows.values()) {
        const span = Math.max(row.maxX - row.minX + 0.8, 1);
        const truss = new THREE.Mesh(new THREE.CylinderGeometry(0.06, 0.06, span, 8), trussMaterial);
        truss.rotation.z = Math.PI / 2;
        truss.position.set((row.minX + row.maxX) / 2, row.y + 0.28, row.z);
        this.roomGroup.add(truss);
      }
    }

    this.roomGroup.visible = this.settings.room;
  }

  private buildRig(layout: Layout): void {
    const count = layout.fixtures.length;
    if (count === 0) return;

    const bodyGeometry = new THREE.BoxGeometry(1, 0.9, 1);
    const bodyMaterial = new THREE.MeshStandardMaterial({
      color: 0x14171f,
      roughness: 0.6,
      metalness: 0.55,
    });
    this.bodies = new THREE.InstancedMesh(bodyGeometry, bodyMaterial, count);
    this.bodies.instanceMatrix.setUsage(THREE.StaticDrawUsage);

    // The lens is what actually glows, so it is what bloom keys off.
    const lensGeometry = new THREE.SphereGeometry(0.42, 12, 8);
    const lensMaterial = new THREE.MeshBasicMaterial({ toneMapped: false });
    this.lenses = new THREE.InstancedMesh(lensGeometry, lensMaterial, count);
    this.lenses.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.lenses.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(count * 3), 3);
    this.lenses.instanceColor.setUsage(THREE.DynamicDrawUsage);

    // Cone authored apex-at-origin, axis along -Y, height 1, base radius 1 —
    // the beam shader relies on exactly this.
    const beamGeometry = new THREE.ConeGeometry(1, 1, 28, 1, true);
    beamGeometry.translate(0, -0.5, 0);
    this.beamColors = new THREE.InstancedBufferAttribute(new Float32Array(count * 3), 3);
    this.beamIntensity = new THREE.InstancedBufferAttribute(new Float32Array(count), 1);
    this.beamColors.setUsage(THREE.DynamicDrawUsage);
    this.beamIntensity.setUsage(THREE.DynamicDrawUsage);
    beamGeometry.setAttribute('beamColor', this.beamColors);
    beamGeometry.setAttribute('beamIntensity', this.beamIntensity);
    this.beams = new THREE.InstancedMesh(beamGeometry, this.beamMaterial, count);
    this.beams.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.beams.frustumCulled = false;
    this.beams.renderOrder = 2;

    const poolGeometry = new THREE.PlaneGeometry(1, 1);
    poolGeometry.rotateX(-Math.PI / 2);
    this.poolColors = new THREE.InstancedBufferAttribute(new Float32Array(count * 3), 3);
    this.poolIntensity = new THREE.InstancedBufferAttribute(new Float32Array(count), 1);
    this.poolColors.setUsage(THREE.DynamicDrawUsage);
    this.poolIntensity.setUsage(THREE.DynamicDrawUsage);
    poolGeometry.setAttribute('poolColor', this.poolColors);
    poolGeometry.setAttribute('poolIntensity', this.poolIntensity);
    this.pools = new THREE.InstancedMesh(poolGeometry, this.poolMaterial, count);
    this.pools.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.pools.frustumCulled = false;
    this.pools.renderOrder = 1;

    const { matrix, pos, quat, scale, euler } = this.scratch;
    layout.fixtures.forEach((fixture, i) => {
      pos.set(fixture.world.x, fixture.world.y, fixture.world.z);
      euler.set(
        THREE.MathUtils.degToRad(fixture.rotation.x),
        THREE.MathUtils.degToRad(fixture.rotation.y),
        THREE.MathUtils.degToRad(fixture.rotation.z),
      );
      quat.setFromEuler(euler);
      const s = fixture.scale;
      scale.set(s, s, s);
      matrix.compose(pos, quat, scale);
      this.bodies!.setMatrixAt(i, matrix);

      // Lens sits on the face the beam leaves from.
      pos.set(
        fixture.world.x + fixture.aim.x * s * 0.5,
        fixture.world.y + fixture.aim.y * s * 0.5,
        fixture.world.z + fixture.aim.z * s * 0.5,
      );
      scale.set(s * 0.72, s * 0.72, s * 0.72);
      matrix.compose(pos, quat, scale);
      this.lenses!.setMatrixAt(i, matrix);
    });
    this.bodies.instanceMatrix.needsUpdate = true;
    this.lenses.instanceMatrix.needsUpdate = true;

    this.rigGroup.add(this.bodies, this.lenses, this.pools, this.beams);
  }

  private frameCamera(layout: Layout): void {
    const { center, radius } = layout.bounds;
    const distance = Math.max(10, radius * 2.2);
    this.controls.target.set(center.x, Math.max(center.y * 0.6, 1.2), center.z);
    // Slightly above the trim, looking down the rig.
    this.camera.position.set(
      center.x,
      Math.max(center.y * 1.1, 3),
      center.z + distance,
    );
    this.camera.far = Math.max(200, distance * 6);
    this.camera.updateProjectionMatrix();
    this.controls.update();
  }

  //
  // Per-frame update.
  //

  /**
   * Pushes decoded fixture states into the instanced attributes.
   *
   * @param states one entry per layout fixture, in layout order.
   * @param time   seconds since start, used for strobe and haze.
   */
  update(states: FixtureState[], time: number): void {
    const { bodies, lenses, beams, pools } = this;
    if (!bodies || !lenses || !beams || !pools) return;

    const { matrix, pos, quat, dir, scale, color } = this.scratch;
    const beamColors = this.beamColors!.array as Float32Array;
    const beamIntensity = this.beamIntensity!.array as Float32Array;
    const poolColors = this.poolColors!.array as Float32Array;
    const poolIntensity = this.poolIntensity!.array as Float32Array;
    const lensColors = lenses.instanceColor!.array as Float32Array;

    for (let i = 0; i < this.fixtures.length; i++) {
      const fixture = this.fixtures[i];
      const state = states[i];

      let level = state.alpha / 255;
      // Strobe: a square wave whose rate rises with the strobe channel.
      if (state.strobe > 0) {
        const hz = 1 + (state.strobe / 255) * 24;
        level *= Math.sin(time * hz * Math.PI * 2) > 0 ? 1 : 0.05;
      }

      const r = srgbToLinear(state.r) * level;
      const g = srgbToLinear(state.g) * level;
      const b = srgbToLinear(state.b) * level;
      // Lens colour is linear, because the tone mapper expects linear light.
      // Beam and pool density, though, follow *perceptual* brightness: linear
      // luminance crushes a fixture at 15% dimmer down to ~2% of full, and the
      // beam disappears even though the eye would still read it clearly on
      // stage.
      const perceptual =
        ((0.2126 * state.r + 0.7152 * state.g + 0.0722 * state.b) / 255) * level;

      lensColors[i * 3] = r;
      lensColors[i * 3 + 1] = g;
      lensColors[i * 3 + 2] = b;

      if (fixture.profile.beam === 'none' || perceptual <= 0.004) {
        beamIntensity[i] = 0;
        poolIntensity[i] = 0;
        continue;
      }

      // Aim.
      if (fixture.profile.beam === 'head') {
        // Pan sweeps +-270 deg, tilt +-135 deg, both centred on 128.
        const pan = ((state.pan - 128) / 128) * Math.PI * 1.5;
        const tilt = ((state.tilt - 128) / 128) * Math.PI * 0.75;
        dir.copy(UP).applyAxisAngle(AXIS_X, tilt);
        dir.applyAxisAngle(AXIS_Y, pan);
      } else {
        dir.set(fixture.aim.x, fixture.aim.y, fixture.aim.z);
      }
      dir.normalize();

      // Length: stop at the floor when pointing down, otherwise run to the cap.
      let length = MAX_BEAM_LENGTH;
      if (dir.y < -0.05) {
        length = Math.min(MAX_BEAM_LENGTH, Math.max(0.5, fixture.world.y / -dir.y));
      }

      // Heads narrow as focus rises; washes keep a fixed spread.
      const halfAngle =
        fixture.profile.beam === 'head'
          ? HEAD_ANGLE * (1.6 - (state.focus / 255) * 0.8)
          : WASH_ANGLE;
      const endRadius = Math.max(0.05, Math.tan(halfAngle) * length);

      pos.set(fixture.world.x, fixture.world.y, fixture.world.z);
      quat.setFromUnitVectors(UP, dir);
      scale.set(endRadius, length, endRadius);
      matrix.compose(pos, quat, scale);
      beams.setMatrixAt(i, matrix);

      color.setRGB(r, g, b);
      // Normalise hue so a dim fixture still shows its colour in the beam; the
      // level is carried by the intensity attribute instead.
      const peak = Math.max(color.r, color.g, color.b, 1e-4);
      beamColors[i * 3] = color.r / peak;
      beamColors[i * 3 + 1] = color.g / peak;
      beamColors[i * 3 + 2] = color.b / peak;
      // Long throws spread the same flux over more volume.
      beamIntensity[i] = Math.min(
        1.0,
        perceptual * (fixture.profile.beam === 'head' ? 1.15 : 0.55),
      );

      // Floor pool where the beam lands.
      if (dir.y < -0.05 && length < MAX_BEAM_LENGTH) {
        const poolRadius = endRadius * 1.7;
        pos.set(
          fixture.world.x + dir.x * length,
          0.02,
          fixture.world.z + dir.z * length,
        );
        matrix.makeScale(poolRadius * 2, 1, poolRadius * 2);
        matrix.setPosition(pos);
        pools.setMatrixAt(i, matrix);
        poolColors[i * 3] = color.r / peak;
        poolColors[i * 3 + 1] = color.g / peak;
        poolColors[i * 3 + 2] = color.b / peak;
        poolIntensity[i] = Math.min(0.85, perceptual * 0.6);
      } else {
        poolIntensity[i] = 0;
      }
    }

    lenses.instanceColor!.needsUpdate = true;
    beams.instanceMatrix.needsUpdate = true;
    this.beamColors!.needsUpdate = true;
    this.beamIntensity!.needsUpdate = true;
    pools.instanceMatrix.needsUpdate = true;
    this.poolColors!.needsUpdate = true;
    this.poolIntensity!.needsUpdate = true;

    this.beamMaterial.uniforms.uTime.value = time;

    if (this.selection >= 0) {
      const f = this.fixtures[this.selection];
      this.selectionMarker.position.set(f.world.x, f.world.y, f.world.z);
      this.selectionMarker.quaternion.copy(this.camera.quaternion);
    }
  }

  applySettings(): void {
    const s = this.settings;
    this.renderer.toneMappingExposure = s.exposure;
    this.bloomPass.enabled = s.bloom;
    this.bloomPass.strength = s.bloomStrength;
    this.beamMaterial.uniforms.uHaze.value = s.haze;
    this.beamMaterial.uniforms.uGain.value = s.gain;
    this.poolMaterial.uniforms.uGain.value = s.gain;
    this.roomGroup.visible = s.room;
    if (this.beams) this.beams.visible = s.beams;
    if (this.pools) this.pools.visible = s.pools;
  }

  render(): void {
    this.controls.update();
    this.composer.render();
  }

  resize(): void {
    const width = this.canvas.clientWidth || window.innerWidth;
    const height = this.canvas.clientHeight || window.innerHeight;
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(width, height, false);
    this.composer.setSize(width, height);
  }

  /** Picks a fixture from a pointer event, in CSS pixels relative to the canvas. */
  pick(clientX: number, clientY: number): PlacedFixture | null {
    if (!this.bodies) return null;
    const rect = this.canvas.getBoundingClientRect();
    const { raycaster, pointer } = this.scratch;
    pointer.set(
      ((clientX - rect.left) / rect.width) * 2 - 1,
      -((clientY - rect.top) / rect.height) * 2 + 1,
    );
    raycaster.setFromCamera(pointer, this.camera);
    const hits = raycaster.intersectObject(this.bodies, false);
    const instanceId = hits[0]?.instanceId;
    if (instanceId === undefined) {
      this.selection = -1;
      this.selectionMarker.visible = false;
      return null;
    }
    this.selection = instanceId;
    this.selectionMarker.visible = true;
    return this.fixtures[instanceId] ?? null;
  }

  dispose(): void {
    this.disposeGroup(this.rigGroup);
    this.disposeGroup(this.roomGroup);
    this.beamMaterial.dispose();
    this.poolMaterial.dispose();
    this.composer.dispose();
    this.renderer.dispose();
  }
}
