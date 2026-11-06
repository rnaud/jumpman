import * as THREE from 'three';

export interface SkyColors { top: number; mid: number; bottom: number }

/**
 * A camera-following gradient sky with soft procedural clouds. It ignores fog
 * and depth, so it works at any scale (a diorama seen from 30k units away, or
 * an endless runner that never stops moving).
 */
export class SkyDome {
  readonly mesh: THREE.Mesh;
  private readonly uniforms: {
    topColor: { value: THREE.Color };
    midColor: { value: THREE.Color };
    bottomColor: { value: THREE.Color };
    cloudColor: { value: THREE.Color };
    cloudAmount: { value: number };
    time: { value: number };
  };

  constructor(colors: SkyColors, private readonly radius = 150000, cloudAmount = 0.55) {
    this.uniforms = {
      topColor: { value: new THREE.Color(colors.top) },
      midColor: { value: new THREE.Color(colors.mid) },
      bottomColor: { value: new THREE.Color(colors.bottom) },
      cloudColor: { value: new THREE.Color(0xffffff) },
      cloudAmount: { value: cloudAmount },
      time: { value: 0 },
    };
    this.mesh = new THREE.Mesh(new THREE.SphereGeometry(radius, 48, 24), new THREE.ShaderMaterial({
      side: THREE.BackSide,
      depthWrite: false,
      depthTest: false,
      fog: false,
      uniforms: this.uniforms,
      vertexShader: /* glsl */ `
        varying vec3 vDir;
        void main() {
          vDir = normalize(position);
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }
      `,
      fragmentShader: /* glsl */ `
        uniform vec3 topColor, midColor, bottomColor, cloudColor;
        uniform float cloudAmount, time;
        varying vec3 vDir;
        float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
        float noise(vec2 p) {
          vec2 i = floor(p), f = fract(p);
          vec2 u = f * f * (3.0 - 2.0 * f);
          return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
        }
        float fbm(vec2 p) {
          float v = 0.0, a = 0.5;
          for (int i = 0; i < 5; i++) { v += a * noise(p); p *= 2.03; a *= 0.5; }
          return v;
        }
        void main() {
          float h = vDir.y;
          vec3 color = mix(bottomColor, midColor, smoothstep(-0.25, 0.08, h));
          color = mix(color, topColor, smoothstep(0.08, 0.75, h));
          // Clouds on a virtual plane above, fading toward the horizon.
          if (h > 0.02) {
            vec2 uv = vDir.xz / (h + 0.18) * 1.6 + vec2(time * 0.004, time * 0.0015);
            float c = smoothstep(0.52, 0.78, fbm(uv)) * cloudAmount * smoothstep(0.02, 0.25, h);
            color = mix(color, cloudColor, c);
          }
          gl_FragColor = vec4(color, 1.0);
          #include <colorspace_fragment>
        }
      `,
    }));
    this.mesh.frustumCulled = false;
    this.mesh.renderOrder = -100;
    this.mesh.name = 'experiment-sky';
  }

  setColors(colors: SkyColors, blend = 1): void {
    this.uniforms.topColor.value.lerp(new THREE.Color(colors.top), blend);
    this.uniforms.midColor.value.lerp(new THREE.Color(colors.mid), blend);
    this.uniforms.bottomColor.value.lerp(new THREE.Color(colors.bottom), blend);
  }

  /** Keep the dome centred on the camera; call once per frame. */
  update(camera: THREE.Camera, time: number): void {
    this.mesh.position.copy(camera.position);
    // Always just inside the far plane, whatever scale the scene runs at.
    if (camera instanceof THREE.PerspectiveCamera) this.mesh.scale.setScalar((camera.far * 0.85) / this.radius);
    this.uniforms.time.value = time;
  }

  dispose(): void {
    this.mesh.removeFromParent();
    this.mesh.geometry.dispose();
    (this.mesh.material as THREE.Material).dispose();
  }
}
