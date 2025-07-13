<script lang="ts">
	import * as Card from '$lib/components/ui/card';
	import { derived, writable } from 'svelte/store';
	import AudioMonitor from './audio-monitor.svelte';
	import AudioMonitor2 from './audio-monitor2.svelte';
	import type { AudioMetrics } from '$lib/types/metrics';
	import { Slider } from '$lib/components/ui/slider';

	const {
		audioDevice = '',
		audio,
		className = ''
	} = $props<{ audioDevice?: string; audio: AudioMetrics; className: string }>();

	let lastAudioSignal: Date | null = $state(null);
	let isDead = $state(true);

	let timer: ReturnType<typeof setInterval> | null = null;

	const heights = 70;
	let duration = $state(5);

	$effect(() => {
		if (timer) clearInterval(timer);
		timer = setInterval(() => {
			if (!lastAudioSignal) {
				isDead = true;
				return;
			}

			const dist = new Date().getTime() - lastAudioSignal.getTime();

			if (dist > 1000) {
				isDead = true;
			} else {
				isDead = false;
			}
		}, 500);

		return () => {
			if (timer) clearInterval(timer);
		};
	});

	$effect(() => {
		if (audio && audio.volume) {
			if (lastAudioSignal && new Date().getTime() - lastAudioSignal.getTime() < 300) {
				return;
			}

			lastAudioSignal = new Date();
		}
	});
</script>

<Card.Root class={`w-full ${className}`}>
	<Card.Header>
		<Card.Title>Audio</Card.Title>
	</Card.Header>
	<Card.Content>
		<Card.Root class="flex w-full flex-row items-center px-3 py-2">
			<span class="relative flex h-3 w-3">
				<span
					class="absolute inline-flex h-full w-full animate-ping rounded-full bg-sky-400 opacity-75"
				></span>
				<span
					class={`relative inline-flex h-3 w-3 rounded-full ${isDead ? 'bg-red-600' : 'bg-green-600'}`}
				></span>
			</span>

			<span>
				{#if audioDevice}
					{audioDevice}
					(
					{#if isDead}
						dead
					{:else}
						alive
					{/if}
					)
				{:else}
					NO INPUT DEVICE
				{/if}
			</span>
		</Card.Root>
	</Card.Content>

	<div>
		<Slider type="single" bind:value={duration} min={1} max={30} step={1} />
		<pre>{duration} sec</pre>
	</div>

	<!-- <AudioMonitor value={volume}></AudioMonitor> -->
	<AudioMonitor2
		viewHeight={heights}
		label="Volume"
		volume={audio?.volume}
		{duration}
		updateInterval={50}
	></AudioMonitor2>

	<AudioMonitor2
		viewHeight={heights}
		label="Bass"
		volume={audio?.bass}
		{duration}
		updateInterval={50}
	></AudioMonitor2>

	<AudioMonitor2
		viewHeight={heights}
		label="Bass Avg"
		volume={audio?.bassAvg}
		{duration}
		updateInterval={50}
	></AudioMonitor2>

	<AudioMonitor2
		viewHeight={heights}
		label="Bass Peaks"
		volume={audio?.bassAvgShort}
		{duration}
		updateInterval={50}
	></AudioMonitor2>

	<AudioMonitor2 viewHeight={heights} label="BPM" volume={audio?.bpm} {duration} updateInterval={50}
	></AudioMonitor2>

	<AudioMonitor2
		viewHeight={heights}
		label="Beat Vol"
		volume={audio?.beatVolume}
		{duration}
		updateInterval={50}
	></AudioMonitor2>
</Card.Root>
