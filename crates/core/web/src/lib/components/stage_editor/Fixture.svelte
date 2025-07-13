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
	<h4>Fixture {fixtureKey}</h4>
	<!-- {groupKey} -->

	{fixture!.name}

	<div class="controls">
		{#each Object.entries(fixture!.state) as [key, value]}
			{#if typeof value === 'number'}
				<div class="control">
					<!-- <label for={key}>{key}</label> -->
					<Slider valueExt={fixture!.state[key]} on_update={(value: number) => onChange(key, value)}
					></Slider>
					<!-- <input
            id={key}
            type="range"
            min="0"
            max="255"
            step="1"
            bind:value={fixture.state[key]}
            on:input={() => dispatch("update", { key, value: fixture.state[key] })}
          />
          <span>{fixture.state[key]}</span> -->
				</div>
			{/if}
		{/each}
	</div>

	<table>
		<tbody>
			<tr>
				<th>Key</th>
				<th>Value</th>
			</tr>
			{#each Object.entries(fixture!.state) as [key, value]}
				<tr>
					<td><code>{key}</code></td>
					<td><code>{JSON.stringify(value)}</code></td>
				</tr>
			{/each}

			<tr>
				<td><code>MODEL</code></td>
				<td><code>{JSON.stringify(fixture!.type_)}</code></td>
			</tr>
		</tbody>
	</table>

	<Button onclick={toggleSelection}>Select</Button>
</div>

<style lang="scss">
	table {
		width: 100%;
		border-collapse: collapse;
		margin-top: 1rem;
	}

	th,
	td {
		border: 1px solid #888;
		padding: 0.5rem 1rem;
		text-align: left;
	}

	th {
		background: #333;
		color: #fff;
	}

	tr:nth-child(even) {
		background: #444;
	}

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

		textarea {
			width: 100%;
			height: min-content;
		}
	}
</style>
