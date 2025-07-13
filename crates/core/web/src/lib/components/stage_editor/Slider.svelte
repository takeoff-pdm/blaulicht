<script lang="ts">
	import { updated } from '$app/state';
	import { onMount, onDestroy } from 'svelte';

	let isSliding = false;

	export let on_update: (_: number) => void = (value: number) => {
		throw 'Not bound';
	};

	function onPointerDown() {
		isSliding = true;
	}

	function onPointerUp() {
		if (isSliding) {
			isSliding = false;
		}
	}

	// Only update value on input if not sliding
	function onInput(event: any) {
		value = parseInt(event.target.value);
		// dispatch('change', value);
		on_update(value);
	}

	onMount(() => {
		window.addEventListener('pointerdown', onPointerDown);
		window.addEventListener('pointerup', onPointerUp);
	});

	onDestroy(() => {
		window.removeEventListener('pointerup', onPointerUp);
	});

	export let valueExt = 0;
	let value = 0;

	$: if (!isSliding) {
		value = valueExt;
	}

	export let min = 0;
	export let max = 255;
	export let step = 1;
</script>

<div class="slider-wrapper">
	<input type="range" bind:value {min} {max} {step} on:input={onInput} />
	<span>{value}</span>
</div>

<style>
	.slider-wrapper {
		display: flex;
		align-items: center;
		gap: 0.5em;
	}
	input[type='range'] {
		flex: 1;
	}
</style>
