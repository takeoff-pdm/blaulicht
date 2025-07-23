<script lang="ts">
	import { onMount } from 'svelte';
	import type { ChangeEvent, GroupSelection, SelectionEvent } from '$lib/types/selection';
	import type { FixtureGroup } from '$lib/types/state';

	import Fixture from './Fixture.svelte';
	import { Button } from '../ui/button';
	import type { Select } from 'bits-ui';

	export let groupKey = 0;
	export let group: FixtureGroup | null = null;

	export let selectionMap: Map<number, GroupSelection> = new Map();
	let selected = false;
	$: selected = selectionMap.get(groupKey)!.entireGroup;

	export let on_change: (_: ChangeEvent) => void = (ev: ChangeEvent) => {
		throw 'Not bound';
	};
	export let on_select: (_: SelectionEvent) => void = (ev: SelectionEvent) => {
		throw 'Not bound';
	};

	let component: HTMLDivElement | null = null;

	function toggleSelection() {
		if (!component) {
			return;
		}

		// component.dispatchEvent(
		// 	new CustomEvent('select', {
		// 		bubbles: true,
		// 		detail: {
		// 			group: groupKey as any as number,
		// 			fixture: null,
		// 			selected: !selected
		// 		}
		// 	})
		// );

		on_select({
			group: groupKey as any as number,
			fixture: undefined,
			selected: !selected
		});

		// dispatch('select', {
		// 	group: groupKey as any as number,
		// 	fixture: null,
		// 	selected: !selected
		// });
	}

	// function propagateSelect(event: CustomEvent<any>) {
	// 	dispatch('select', event.detail);
	// }

	// function propagateChange(event: CustomEvent<any>) {
	// 	event.detail.groupID = groupKey;
	// 	dispatch('change', event.detail);
	// }
</script>

<div bind:this={component} class="fix-group" class:selected>
	<span class="fix-group__title"><code>GROUP {groupKey}</code></span>

	<div class="fix-group__fixtures">
		{#each Object.entries(group!.fixtures) as fixture_arr}
			<Fixture
				selected={selectionMap.get(groupKey)!.onlyFixtures.get(parseInt(fixture_arr[0]))}
				fixtureKey={parseInt(fixture_arr[0])}
				fixture={fixture_arr[1]}
				on_change={(change: ChangeEvent) => {
					change.groupID = groupKey;
					on_change(change);
				}}
				on_select={(select: SelectionEvent) => {
					select.group = groupKey;
					on_select(select);
				}}
			></Fixture>
		{/each}
	</div>
</div>

<style lang="scss">
	.fix-group {
		background-color: #424242;
		display: flex;
		flex-direction: column;
		padding: 1rem 2rem;
		gap: 1rem;
		border: solid black 1px;
		width: 100%;
		box-sizing: border-box;

		width: 50%;

		&__title {
			background-color: black;
			width: fit-content;
			padding: 0.1rem 0.5rem;
			color: #909090;
			border: solid 1px transparent;
		}

		&__fixtures {
			display: flex;
			gap: 1rem;
		}

		&.selected {
			.fix-group__title {
				color: white;
				border: solid 1px #00ff88;
			}
		}

		&:hover {
			background-color: #606060;
			cursor: pointer;
		}
	}
</style>
