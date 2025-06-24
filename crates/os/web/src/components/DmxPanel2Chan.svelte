<script lang="ts">
  import { onMount } from 'svelte';

  export let brightness: number = 255;     // DMX Channel 1
  export let colorTemp: number = 127;      // DMX Channel 2 (0 = warm, 255 = cool)

  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;

  function lerpColor(c1: { r: number; g: number; b: number }, c2: { r: number; g: number; b: number }, t: number) {
    return {
      r: Math.round(c1.r + (c2.r - c1.r) * t),
      g: Math.round(c1.g + (c2.g - c1.g) * t),
      b: Math.round(c1.b + (c2.b - c1.b) * t)
    };
  }

  function renderLight(bri: number, temp: number) {
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;

    const warmWhite = { r: 255, g: 214, b: 170 }; // ~2700K
    const coolWhite = { r: 170, g: 204, b: 255 }; // ~6500K

    const t = temp / 255;
    const baseColor = lerpColor(warmWhite, coolWhite, t);

    const r = Math.round((baseColor.r * bri) / 255);
    const g = Math.round((baseColor.g * bri) / 255);
    const b = Math.round((baseColor.b * bri) / 255);
    const color = `rgb(${r}, ${g}, ${b})`;

    ctx.clearRect(0, 0, width, height);

    const gradient = ctx.createRadialGradient(
      width / 2, height / 2, 10,
      width / 2, height / 2, width / 2
    );
    gradient.addColorStop(0, color);
    gradient.addColorStop(1, 'black');

    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);
  }

  // Automatically re-render when props change
  $: renderLight(brightness, colorTemp);

  onMount(() => {
    ctx = canvas.getContext('2d')!;
    renderLight(brightness, colorTemp);
  });
</script>

<style>
  .controller {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 320px;
    margin: auto;
  }

  canvas {
    width: 50px;
    height: 50px;
    border-radius: 8px;
    box-shadow: 0 0 15px rgba(0,0,0,0.3);
    border: white 1px solid;
  }
</style>

<div class="controller">
  <canvas bind:this={canvas} width="100" height="100"></canvas>
</div>
