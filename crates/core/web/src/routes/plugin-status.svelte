<script lang="ts">
	import * as Card from '$lib/components/ui/card';
	import { derived, writable } from 'svelte/store';
	import { Slider } from '$lib/components/ui/slider';
	import type { AudioMetrics, SystemMetrics } from '$lib/types/metrics';
	import type { Data, PluginState } from '$lib/types/state';
	import AudioMonitor2 from './audio-monitor2.svelte';
	import HeartPulse from '@lucide/svelte/icons/heart-pulse';

	const { sysState } = $props<{ sysState: { number: PluginState } }>();
	let s = sysState as { number: PluginState };
</script>

<Card.Root class={`w-full`}>
	<Card.Header>
		<Card.Title>Plugins</Card.Title>
	</Card.Header>
	<Card.Content>
		<Card.Root class="flex w-full flex-col items-center px-3 py-2">
			{#each Object.entries(s) as [key, plugin]}
				<div>
					<p>
						{key}:
						{plugin.path}
					</p>

					<p>
						enabled: {plugin.flags.enabled}
					</p>
					<p>
						has err: {plugin.flags.has_error}
					</p>
				</div>
			{/each}
		</Card.Root>
	</Card.Content>
</Card.Root>
