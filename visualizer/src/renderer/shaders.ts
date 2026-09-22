/**
 * GLSL for the two effects that carry the look: volumetric beams and the light
 * pools they cast on the floor.
 *
 * Both are drawn as additively-blended instanced geometry. Real volumetric
 * scattering would mean ray-marching a froxel grid, which is overkill for a
 * lighting previsualiser — a cone whose opacity is driven by its own parametric
 * coordinates reads the same way on stage and costs one draw call for the whole
 * rig.
 */

export const BEAM_VERTEX = /* glsl */ `
  attribute vec3 beamColor;
  attribute float beamIntensity;

  varying float vT;
  varying float vR;
  varying vec3 vColor;
  varying float vIntensity;
  varying vec3 vWorld;

  void main() {
    // Cone geometry is authored with its apex at the origin and its axis along
    // -Y, height 1, base radius 1. So -y is the distance along the beam and the
    // cone's radius at that distance is exactly -y.
    float t = clamp(-position.y, 0.0, 1.0);
    float radiusAtT = max(t, 1e-4);

    vT = t;
    vR = clamp(length(position.xz) / radiusAtT, 0.0, 1.0);
    vColor = beamColor;
    vIntensity = beamIntensity;

    vec4 world = modelMatrix * instanceMatrix * vec4(position, 1.0);
    vWorld = world.xyz;
    gl_Position = projectionMatrix * viewMatrix * world;
  }
`;

export const BEAM_FRAGMENT = /* glsl */ `
  precision highp float;

  uniform float uTime;
  uniform float uHaze;
  uniform float uGain;

  varying float vT;
  varying float vR;
  varying vec3 vColor;
  varying float vIntensity;
  varying vec3 vWorld;

  // Cheap value noise; enough to break up the cone into drifting haze.
  float hash(vec3 p) {
    return fract(sin(dot(p, vec3(127.1, 311.7, 74.7))) * 43758.5453123);
  }

  float noise(vec3 p) {
    vec3 i = floor(p);
    vec3 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
      mix(mix(hash(i + vec3(0, 0, 0)), hash(i + vec3(1, 0, 0)), f.x),
          mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), f.x), f.y),
      mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), f.x),
          mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), f.x), f.y),
      f.z);
  }

  void main() {
    if (vIntensity <= 0.001) discard;

    // Soft outer edge, hot core.
    float edge = smoothstep(1.0, 0.45, vR);
    float core = mix(0.15, 1.0, pow(1.0 - vR, 3.0));
    // Light thins out with distance from the lens.
    float axial = pow(1.0 - vT, 1.5);
    // Very close to the lens the cone is degenerate; fade it in.
    float lensFade = smoothstep(0.0, 0.04, vT);

    float haze = mix(1.0, 0.55 + 0.9 * noise(vWorld * 1.6 + vec3(0.0, uTime * 0.35, 0.0)), uHaze);

    float density = edge * core * axial * lensFade * haze * vIntensity;
    vec3 rgb = vColor * density * uGain;

    gl_FragColor = vec4(rgb, density);
  }
`;

export const POOL_VERTEX = /* glsl */ `
  attribute vec3 poolColor;
  attribute float poolIntensity;

  varying vec2 vUv;
  varying vec3 vColor;
  varying float vIntensity;

  void main() {
    vUv = uv;
    vColor = poolColor;
    vIntensity = poolIntensity;
    gl_Position = projectionMatrix * viewMatrix * modelMatrix * instanceMatrix * vec4(position, 1.0);
  }
`;

export const POOL_FRAGMENT = /* glsl */ `
  precision highp float;

  uniform float uGain;

  varying vec2 vUv;
  varying vec3 vColor;
  varying float vIntensity;

  void main() {
    if (vIntensity <= 0.001) discard;

    float d = length(vUv - 0.5) * 2.0;
    if (d > 1.0) discard;

    // Penumbra: bright centre falling to nothing at the rim.
    float falloff = pow(1.0 - d, 2.2);
    float density = falloff * vIntensity;

    gl_FragColor = vec4(vColor * density * uGain, density);
  }
`;
