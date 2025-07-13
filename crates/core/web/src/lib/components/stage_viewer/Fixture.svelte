<script lang="ts">
	import type { ChangeEvent, SelectionEvent } from '$lib/types/selection';
	import type { Fixture } from '$lib/types/state';
	import Button from '../ui/button/button.svelte';

	import Slider from './Slider.svelte';
	export let fixtureKey = 0;
	export let fixture: Fixture | null = null;

	export let selected = false;
	$: console.log(selected);

	export let on_change: (_: ChangeEvent) => void = (ev: ChangeEvent) => {
		throw 'Not bound';
	};
	export let on_select: (_: SelectionEvent) => void = (ev: SelectionEvent) => {
		throw 'Not bound';
	};

	let component: HTMLElement | null = null;

	function toggleSelection(e: any) {
		console.log('TOG');
		on_select({
			group: undefined,
			fixture: fixtureKey as any as number,
			selected: !selected
		});
	}

	function onChange(key: string, newValue: any) {
		on_change({
			key,
			newValue,
			fixID: fixtureKey,
			groupID: 0 // Will be later injected by the group.
		});

		// component!.dispatchEvent(
		// 	new CustomEvent('change', {
		// 		bubbles: true,
		// 		detail: {
		// 			key,
		// 			newValue,
		// 			fixID: fixtureKey,
		// 			groupID: null // Will be later injected by the group.
		// 		}
		// 	})
		// );
	}
</script>

<div bind:this={component} class="fixture" class:selected>
	<div class="fixture__header">
		<span>ID {fixtureKey}</span>
		{fixture!.name}
	</div>
	<br />
	{JSON.stringify(fixture!.type_)}
</div>

<style lang="scss">
	.fixture {
		background-color: #525252;
		display: flex;
		flex-direction: column;
		padding: 1rem 2rem;
		border: solid black 1px;
		width: 100%;
		box-sizing: border-box;

		&:hover {
			background-color: #707070;
			cursor: pointer;
		}

		&.selected {
			border-color: greenyellow;
		}

		&__header {
			display: flex;	
			align-items: center;
			justify-content: space-between;
		}
	}
</style>
