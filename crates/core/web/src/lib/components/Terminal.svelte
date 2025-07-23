<script lang="ts">
	import { tick } from 'svelte';
	import * as Card from '$lib/components/ui/card/index.js';

	let terminalRef: HTMLDivElement | null = null;

	$effect.pre(() => {
		tick().then(() => {
			if (!terminalRef) {
				return;
			}

			terminalRef.scrollTop = terminalRef.scrollHeight;
		});
	});

	const {
		lines = [],
		title = '',
		className = ''
	} = $props<{ lines?: string[]; className?: string; title?: string }>();
</script>

<Card.Root class={`root w-full ${className}`}>
	<Card.Header>
		<Card.Title>{title}</Card.Title>
	</Card.Header>
	<Card.Content>
		<div bind:this={terminalRef} class="root__terminal">
			{#each lines as line}
				<div class="root__terminal__line">{line}</div>
			{/each}
		</div>
	</Card.Content>
</Card.Root>

<style lang="scss">
	// :global(.root) {
	// 	width: 20rem;
	// }

	.root {
		&__terminal {
			/* background: hsla(230, 20%, 8%, 1); */
			/* color: hsla(230, 10%, 80%, 1); */
			/* padding: 1rem; */
			/* border-radius: 8px; */
			/* box-sizing: border-box; */

			font-family: 'Courier New', monospace;
			height: 100%;
			width: 100%;
			overflow-y: auto;
			white-space: pre-wrap;
			box-shadow: 0 0 10px rgba(0, 0, 0, 0.5);

			&__line {
				line-height: 1.4;
			}
		}
	}
</style>
