<script>
    import { onMount, onDestroy } from "svelte";
  
    export let bpm = 120; // Default BPM

    export let dimensions=200

    let canvas;
    let ctx;
    let isBlinking = false;
    let lastBlinkTime = 0;
    let blinkInterval;
    let animationFrame;
  
    function updateBlinkInterval(pBpm) {
      blinkInterval = (60 / pBpm) * 1000; // Convert BPM to milliseconds
    }
  
    function draw(time) {
      if (!ctx) return;
      
      // Toggle blinking based on time and blinkInterval
      if (time - lastBlinkTime >= blinkInterval) {
        isBlinking = !isBlinking;
        lastBlinkTime = time;
      }
  
      // Clear the canvas for the next frame
      ctx.clearRect(0, 0, canvas.width, canvas.height);
  
      const centerX = canvas.width / 2;
      const centerY = canvas.height / 2;
      const radius = Math.min(canvas.width, canvas.height) / 3;
  
      if (isBlinking) {
        // Create a glowing effect with a radial gradient
        let gradient = ctx.createRadialGradient(centerX, centerY, radius * 0.2, centerX, centerY, radius);
        gradient.addColorStop(0, 'rgba(0, 255, 0, 1)');
        gradient.addColorStop(1, 'rgba(0, 255, 0, 0)');
        ctx.fillStyle = gradient;
      } else {
        // Dimmed state of the bulb
        ctx.fillStyle = "rgba(0, 50, 0, 1)";
      }
  
      // Draw the circle (the light bulb)
      ctx.beginPath();
      ctx.arc(centerX, centerY, radius, 0, Math.PI * 2);
      ctx.fill();
      ctx.closePath();
  
      animationFrame = requestAnimationFrame(draw);
    }
  
    onMount(() => {
      ctx = canvas.getContext("2d");
      updateBlinkInterval();
      lastBlinkTime = performance.now();
      animationFrame = requestAnimationFrame(draw);
    });
  
    // When BPM changes, update the blink interval and reset timer
    $: {
      updateBlinkInterval(bpm);
      lastBlinkTime = performance.now();
    }
  
    onDestroy(() => {
      if (animationFrame) cancelAnimationFrame(animationFrame);
    });
  </script>
  
  <canvas bind:this={canvas} width={dimensions} height={dimensions} />
  