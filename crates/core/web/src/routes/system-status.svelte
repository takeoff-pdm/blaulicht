<script lang="ts">
	import * as Card from '$lib/components/ui/card';
	import { derived, writable } from 'svelte/store';
	import { Slider } from '$lib/components/ui/slider';
	import type { AudioMetrics, SystemMetrics } from '$lib/types/metrics';
	import type { Data } from '$lib/types/state';
	import AudioMonitor2 from './audio-monitor2.svelte';
	import HeartPulse from '@lucide/svelte/icons/heart-pulse';
	import RefreshCcw from '@lucide/svelte/icons/refresh-ccw';
	import { Button } from '$lib/components/ui/button';

	const {
		heartbeat = false,
		className = '',
		sysState,
		metrics,
		on_reload
	} = $props<{
		sysState: Data;
		className?: string;
		metrics: SystemMetrics;
		heartbeat: boolean;
		on_reload: Function;
	}>();

	let heights = 70;
	let duration = 20;

	let lastBeat = $state(new Date());
	let elapsed = $state(0);

	let timer: ReturnType<typeof setInterval> | null = null;
	$effect(() => {
		if (timer) clearInterval(timer);
		timer = setInterval(() => {
			elapsed = new Date().getTime() - lastBeat.getTime();
		}, 500);

		return () => {
			if (timer) clearInterval(timer);
		};
	});

	$effect(() => {
		if (heartbeat) {
			lastBeat = new Date();
		}
	});
</script>

<Card.Root class={`w-full ${className}`}>
	<Card.Header>
		<Card.Title>Health</Card.Title>
	</Card.Header>
	<Card.Content>
		<Card.Root class="">
			<div class="flex w-full flex-row items-center px-3 py-2 gap-3">
				<div class="flex w-full flex-row items-center gap-2">
					<HeartPulse
						class={elapsed > 2000 ? 'animate-bounce' : ''}
						color={elapsed < 2000 ? 'lime' : 'red'}
					/>
					Heartbeat
				</div>
				<code>
					{elapsed.toString().padStart(4, '0')}
				</code>
			</div>

			<div class="w-10">
				<Button onclick={on_reload}>
					<RefreshCcw />
					Reload
				</Button>
			</div>
		</Card.Root>
	</Card.Content>

	<!-- <AudioMonitor value={volume}></AudioMonitor> -->
	<AudioMonitor2
		viewHeight={heights}
		label="Loop Speed"
		volume={(metrics as SystemMetrics).loopSpeed}
		{duration}
		updateInterval={50}
		dynamicRange={true}
	></AudioMonitor2>

	<AudioMonitor2
		viewHeight={heights}
		label="Tick Speed"
		volume={(metrics as SystemMetrics).tickSpeed}
		{duration}
		updateInterval={50}
		dynamicRange={true}
	></AudioMonitor2>
</Card.Root>
