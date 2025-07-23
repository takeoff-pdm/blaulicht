<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import * as Card from '$lib/components/ui/card';
	import '@fontsource/press-start-2p';

	export let duration = 10; // seconds of data to display
	export let updateInterval = 100; // ms between volume updates
	export let volume = 0; // volume value (0–255)

	export let label = '';

	export let dynamicRange = false

	let canvas: HTMLCanvasElement;
	let ctx: CanvasRenderingContext2D;
	let width = 0;
	let height = 0;

	export let viewHeight = 100;

	let maxVolume = 255;
	const maxPoints = Math.ceil((duration * 10000) / updateInterval);
	let data: { time: number; value: number }[] = [];

	let animationFrame: number;

	function updateData() {
		const now = Date.now();
		data.push({ time: now, value: volume });

		const cutoff = now - duration * 1000;
		while (data.length > maxPoints || (data.length && data[0].time < cutoff)) {
			data.shift();
		}

		// TODO: maybe limit to 1s
		//
		// Update max value.
		//
		if (dynamicRange) {
			let max = 0;
			for (const dataPoint of data) {
				if (dataPoint.value > max) {
					max = dataPoint.value;
				}
			}

			maxVolume = max;
		}
	}

	function drawSmoothedLine(points: { x: number; y: number }[]) {
		if (points.length < 2) return;

		ctx.beginPath();
		ctx.moveTo(points[0].x, points[0].y);

		for (let i = 1; i < points.length - 1; i++) {
			const midX = (points[i].x + points[i + 1].x) / 2;
			const midY = (points[i].y + points[i + 1].y) / 2;
			ctx.quadraticCurveTo(points[i].x, points[i].y, midX, midY);
		}

		// Last segment
		ctx.lineTo(points[points.length - 1].x, points[points.length - 1].y);

		ctx.strokeStyle = '#00ff88';
		ctx.lineWidth = 2;
		ctx.stroke();
	}

	function draw() {
		ctx.clearRect(0, 0, width, height);
		// ctx.fillStyle = '#000000';
		// ctx.fillRect(0, 0, width, height);
		// ctx.fillStyle = '#00ff88';

		if (data.length < 2) return;

		const now = Date.now();
		const startTime = now - duration * 1000;

		const points = data.map(({ time, value }) => ({
			x: ((time - startTime) / (duration * 1000)) * width,
			y: height - (value / maxVolume) * height
		}));

		drawSmoothedLine(points);

		ctx.font = '12px "Press Start 2P"';
		ctx.font = 'bold 12px "Courier New"';
		ctx.fillStyle = '#00ff88';

		const textMetrics = ctx.measureText(label);
		const padding = 6;
		const bgHeight = 20;
		ctx.fillStyle = 'rgba(0,0,0,0.95)';
		ctx.fillRect(5, 11, textMetrics.width + padding * 2, bgHeight);

		const volText = `${volume}`;
		const volMetrics = ctx.measureText(volText);
		ctx.fillRect(width - 50 - padding, 11, volMetrics.width + padding * 2, bgHeight);

		ctx.fillStyle = '#EE82EE';
		ctx.fillText(label, 12, 24);
		ctx.fillText(`${volume}`, width - 50, 24);
	}

	function renderLoop() {
		updateData();
		draw();
		animationFrame = requestAnimationFrame(renderLoop);
	}

	function resizeCanvas() {
		const rect = canvas.getBoundingClientRect();
		width = canvas.width = rect.width * devicePixelRatio;
		height = canvas.height = rect.height * devicePixelRatio;

		canvas.style.width = `${rect.width}px`;
		canvas.style.height = `${rect.height}px`;

		ctx.setTransform(1, 0, 0, 1, 0, 0);
		ctx.scale(devicePixelRatio, devicePixelRatio);
	}

	onMount(() => {
		ctx = canvas.getContext('2d');
		resizeCanvas();

		window.addEventListener('resize', resizeCanvas);
		animationFrame = requestAnimationFrame(renderLoop);

		return () => {
			cancelAnimationFrame(animationFrame);
			window.removeEventListener('resize', resizeCanvas);
		};
	});
</script>

<div style="{`height: ${viewHeight}px`}}">
	<canvas style={`height: ${viewHeight}px`} bind:this={canvas} />
</div>

<!-- <script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import * as Card from '$lib/components/ui/card';
	import '@fontsource/press-start-2p';

	export let duration = 10; // seconds of data to display
	export let updateInterval = 100; // ms between volume updates
	export let volume = 0; // volume value (0–255)
	export let label = '';
	export let viewHeight = 100;

	let canvas: HTMLCanvasElement;
	let ctx: CanvasRenderingContext2D;
	let width = 0;
	let height = 0;

	const maxVolume = 255;
	const maxPoints = Math.ceil((duration * 10000) / updateInterval);
	let data: { time: number; value: number }[] = [];

	let animationFrame: number;

	function updateData() {
		const now = Date.now();
		data.push({ time: now, value: volume });

		const cutoff = now - duration * 1000;
		while (data.length > maxPoints || (data.length && data[0].time < cutoff)) {
			data.shift();
		}
	}

	function drawSmoothedLine(points: { x: number; y: number }[]) {
		if (points.length < 2) return;

		ctx.beginPath();
		ctx.moveTo(points[0].x, points[0].y);

		for (let i = 0; i < points.length - 1; i++) {
			const p0 = points[i - 1] || points[i];
			const p1 = points[i];
			const p2 = points[i + 1];
			const p3 = points[i + 2] || p2;

			const cp1x = p1.x + (p2.x - p0.x) / 6;
			const cp1y = p1.y + (p2.y - p0.y) / 6;

			const cp2x = p2.x - (p3.x - p1.x) / 6;
			const cp2y = p2.y - (p3.y - p1.y) / 6;

			ctx.bezierCurveTo(cp1x, cp1y, cp2x, cp2y, p2.x, p2.y);
		}

		ctx.strokeStyle = '#00ff88';
		ctx.lineWidth = 2;
		ctx.stroke();
	}

	function draw() {
		ctx.clearRect(0, 0, width, height);

		// Optional: Background gradient
		const gradient = ctx.createLinearGradient(0, 0, 0, height);
		gradient.addColorStop(0, '#002');
		gradient.addColorStop(1, '#111');
		ctx.fillStyle = gradient;
		ctx.fillRect(0, 0, width, height);

		ctx.font = '12px "Press Start 2P"';
		ctx.fillStyle = '#00ff88';
		// ctx.fillText(label, 12, 24);
		// ctx.fillText(`${volume}`, width - 50, 24);

        // Draw black background behind label
        const textMetrics = ctx.measureText(label);
        const padding = 6;
        const bgHeight = 20;
        ctx.fillStyle = 'rgba(0,0,0,0.85)';
        ctx.fillRect(8, 8, textMetrics.width + padding * 2, bgHeight);

        // Draw black background behind volume
        const volText = `${volume}`;
        const volMetrics = ctx.measureText(volText);
        ctx.fillRect(width - 50 - padding, 8, volMetrics.width + padding * 2, bgHeight);

        // Restore text color for drawing text
        ctx.fillStyle = '#00ff88';
        ctx.fillText(label, 12, 24);
        ctx.fillText(volText, width - 50, 24);

		if (data.length < 2) return;

		const now = Date.now();
		const startTime = now - duration * 1000;

		const points = data.map(({ time, value }) => ({
			x: ((time - startTime) / (duration * 1000)) * width,
			y: height - (value / maxVolume) * height
		}));

		drawSmoothedLine(points);
	}

	function renderLoop() {
		updateData();
		draw();
		animationFrame = requestAnimationFrame(renderLoop);
	}

	function resizeCanvas() {
		const rect = canvas.getBoundingClientRect();
		width = canvas.width = rect.width * devicePixelRatio;
		height = canvas.height = rect.height * devicePixelRatio;

		canvas.style.width = `${rect.width}px`;
		canvas.style.height = `${rect.height}px`;

		ctx.setTransform(1, 0, 0, 1, 0, 0);
		ctx.scale(devicePixelRatio, devicePixelRatio);
	}

	onMount(() => {
		ctx = canvas.getContext('2d');
		resizeCanvas();

		window.addEventListener('resize', resizeCanvas);
		animationFrame = requestAnimationFrame(renderLoop);

		return () => {
			cancelAnimationFrame(animationFrame);
			window.removeEventListener('resize', resizeCanvas);
		};
	});
</script>

<div style={`height: ${viewHeight}px`}>
	<canvas style={`height: ${viewHeight}px`} bind:this={canvas} />
</div>

<style lang="scss">
	canvas {
		width: 100%;
		display: block;
		background-color: #111;
		border-radius: 8px;
	}
</style> -->

<style lang="scss">
	canvas {
		width: 100%;
		display: block;
		background-color: #111;
		border-radius: 8px;
	}
</style>
