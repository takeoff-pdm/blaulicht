<script lang="ts">
    import { onMount } from 'svelte';

    export let brightness = 255;
    export let red = 255;
    export let green = 255;
    export let blue = 255;

    let canvas;
    let ctx;
  
    // Function to update the canvas color based on DMX values
    function renderLight(bri: number, r: number, g: number, b: number) {
      if (!ctx) return;
  
      const width = canvas.width;
      const height = canvas.height;
  
      // Calculate color with brightness
      r = Math.round((r * bri) / 255);
      g = Math.round((g * bri) / 255);
      b = Math.round((b * bri) / 255);
      const color = `rgb(${r}, ${g}, ${b})`;
  
      ctx.clearRect(0, 0, width, height);
  
      // Draw a glowing circle to simulate light
      const gradient = ctx.createRadialGradient(
        width / 2, height / 2, 10,
        width / 2, height / 2, width / 2
      );
      gradient.addColorStop(0, color);
      gradient.addColorStop(1, 'black');
  
      ctx.fillStyle = gradient;
      ctx.fillRect(0, 0, width, height);
    }
  
    // Update canvas when values change
    $: renderLight(brightness, red, green, blue);
  
    onMount(() => {
      ctx = canvas.getContext('2d');
      renderLight(brightness, red, green, blue);
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
  
    .slider-group {
      display: flex;
      flex-direction: column;
    }
  
    canvas {
      width: 60px;
      height: 60px;
      border-radius: 8px;
      box-shadow: 0 0 15px rgba(0,0,0,0.3);
      border: white 1px solid;
    }
  </style>
  
  <div class="controller">
    <canvas bind:this={canvas} width="100" height="100"></canvas>
  </div>
  