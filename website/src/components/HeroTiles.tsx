import React, { useEffect, useRef } from 'react';
import { prefersReducedMotion } from '../lib/motion';

/**
 * A field of tessera tiles behind the hero. It is one shader on one plane, not
 * a scene graph: cheap enough to run on an integrated GPU, and it draws the
 * same mark the app uses. Nothing here carries meaning, so it fades out when
 * the reader scrolls, and it renders a single still frame when the reader asks
 * for reduced motion.
 */
export const HeroTiles: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // The field is a desktop enhancement. On a phone it costs more than it
    // gives, so the hero stays flat and three.js is never fetched.
    const hasDesktopPointer = window.matchMedia(
      '(min-width: 1024px) and (hover: hover) and (pointer: fine)',
    ).matches;
    if (!hasDesktopPointer) return;

    let disposed = false;
    let cleanup = () => {};
    let idleHandle = 0;
    let idle = false;

    // The field is decoration. Let the hero image and fonts land first, then
    // spend the main thread on three.js.
    const start = () => {
      void (async () => {
        if (disposed) return;
        try {
          const THREE = await import('three');
          if (disposed) return;

          const renderer = new THREE.WebGLRenderer({
            canvas,
            alpha: true,
            antialias: false,
            powerPreference: 'low-power',
          });
          renderer.setClearColor(0x000000, 0);

          const scene = new THREE.Scene();
          const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);

          const uniforms = {
            uRes: { value: new THREE.Vector2(1, 1) },
            uTime: { value: 0 },
            uPointer: { value: new THREE.Vector2(0, 0) },
            uScroll: { value: 0 },
          };

          const material = new THREE.ShaderMaterial({
            uniforms,
            transparent: true,
            depthWrite: false,
            vertexShader: /* glsl */ `
              varying vec2 vUv;
              void main() {
                vUv = uv;
                gl_Position = vec4(position.xy, 0.0, 1.0);
              }
            `,
            fragmentShader: /* glsl */ `
              precision highp float;
              varying vec2 vUv;
              uniform vec2 uRes;
              uniform float uTime;
              uniform vec2 uPointer;
              uniform float uScroll;

              float hash21(vec2 p) {
                p = fract(p * vec2(123.34, 456.21));
                p += dot(p, p + 45.32);
                return fract(p.x * p.y);
              }

              float sdRoundBox(vec2 p, vec2 b, float r) {
                vec2 q = abs(p) - b + r;
                return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
              }

              void main() {
                vec2 px = vUv * uRes;
                px.y += uScroll * 0.05;

                vec2 cellSize = vec2(62.0, 46.0);
                vec2 id = floor(px / cellSize);
                vec2 local = mod(px, cellSize) - cellSize * 0.5;
                vec2 centered = (id + 0.5) * cellSize - uRes * 0.5;

                float t = uTime * 0.22;
                float wave = sin(id.x * 0.33 + t) * cos(id.y * 0.42 - t * 0.8);
                local += vec2(wave * 1.5, wave * 1.0);
                local /= 1.0 + wave * 0.03;

                float d = sdRoundBox(local, cellSize * 0.5 - vec2(3.0), 4.0);
                float tile = 1.0 - smoothstep(-1.4, 1.4, d);

                float h = hash21(id);
                vec3 col = vec3(0.965, 0.949, 0.918);
                if (h > 0.62) {
                  col = vec3(0.984, 0.973, 0.953);
                } else if (h > 0.34) {
                  col = vec3(0.929, 0.906, 0.859);
                } else if (h > 0.10) {
                  col = vec3(0.898, 0.868, 0.804);
                }

                if (h > 0.9905) {
                  col = vec3(0.760, 0.255, 0.047);
                } else if (h > 0.9795) {
                  col = vec3(0.369, 0.451, 0.345);
                } else if (h > 0.9700) {
                  col = vec3(0.298, 0.353, 0.400);
                }

                float glow = smoothstep(300.0, 0.0, length(centered - uPointer));
                col += glow * 0.05 * vec3(0.76, 0.25, 0.05);

                float fadeX = smoothstep(0.0, 0.12, vUv.x) * smoothstep(1.0, 0.88, vUv.x);
                float fadeY = smoothstep(0.0, 0.34, vUv.y) * smoothstep(1.0, 0.94, vUv.y);
                float alpha = tile * fadeX * fadeY * 0.92;

                gl_FragColor = vec4(col, alpha);
              }
            `,
          });

          const geometry = new THREE.PlaneGeometry(2, 2);
          scene.add(new THREE.Mesh(geometry, material));

          const pointerTarget = new THREE.Vector2(0, 0);
          const resize = () => {
            const width = canvas.clientWidth;
            const height = canvas.clientHeight;
            if (!width || !height) return;
            renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.75));
            renderer.setSize(width, height, false);
            uniforms.uRes.value.set(width, height);
          };

          const onPointerMove = (event: PointerEvent) => {
            const rect = canvas.getBoundingClientRect();
            pointerTarget.set(
              event.clientX - rect.left - rect.width / 2,
              rect.bottom - event.clientY - rect.height / 2,
            );
          };

          let frame = 0;
          let started = performance.now();
          const draw = (now: number) => {
            uniforms.uTime.value = (now - started) / 1000;
            uniforms.uPointer.value.lerp(pointerTarget, 0.06);
            uniforms.uScroll.value = window.scrollY;
            renderer.render(scene, camera);
            frame = window.requestAnimationFrame(draw);
          };

          resize();
          uniforms.uPointer.value.copy(pointerTarget);
          handoff();
          window.addEventListener('resize', resize);

          if (prefersReducedMotion()) {
            renderer.render(scene, camera);
          } else {
            window.addEventListener('pointermove', onPointerMove, { passive: true });
            frame = window.requestAnimationFrame(draw);
          }

          const observer = new IntersectionObserver(([entry]) => {
            if (prefersReducedMotion()) return;
            if (entry.isIntersecting) {
              if (!frame) {
                started = performance.now() - uniforms.uTime.value * 1000;
                frame = window.requestAnimationFrame(draw);
              }
            } else if (frame) {
              window.cancelAnimationFrame(frame);
              frame = 0;
            }
          });
          observer.observe(canvas);

          cleanup = () => {
            observer.disconnect();
            window.removeEventListener('resize', resize);
            window.removeEventListener('pointermove', onPointerMove);
            if (frame) window.cancelAnimationFrame(frame);
            geometry.dispose();
            material.dispose();
            renderer.dispose();
          };
        } catch {
          // No WebGL: the hero is still complete without the texture.
        }
      })();
    };

    idle = typeof window.requestIdleCallback === 'function';
    idleHandle = idle
      ? window.requestIdleCallback(start, { timeout: 2500 })
      : window.setTimeout(start, 1200);

    function handoff() {
      if (canvas) canvas.dataset.ready = 'true';
    }

    return () => {
      disposed = true;
      if (idle) window.cancelIdleCallback(idleHandle);
      else window.clearTimeout(idleHandle);
      cleanup();
    };
  }, []);

  return (
    <div aria-hidden="true" className="pointer-events-none absolute inset-0 overflow-hidden">
      <canvas
        ref={canvasRef}
        className="h-full w-full opacity-0 transition-opacity duration-1000 data-[ready=true]:opacity-100"
      />
    </div>
  );
};
