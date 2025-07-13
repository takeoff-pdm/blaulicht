<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import type { ChangeEvent, SelectionEvent } from '$lib/types/selection';
	import type { EngineState } from '$lib/types/state';
	import FixtureGroup from './FixtureGroup.svelte';

	export let state: EngineState | null = null;
	$: if (state) buildSelectionMap(state);

	export let on_control: (ev: any) => {} = (ev: any) => {
		throw 'Not bound';
	};

	interface GroupSelection {
		onlyFixtures: Map<number, boolean>;
		entireGroup: boolean;
	}

	let selectionMap: Map<number, GroupSelection> = new Map();

	function buildSelectionMap(state: EngineState) {
		let selectionMapTemp = new Map();

		for (let [g_id_raw, group] of Object.entries(state.groups)) {
			const g_id = parseInt(g_id_raw);

			let onlyFixtures = new Map();
			let entireGroup = false;

			for (let [f_id_raw, fixture] of Object.entries(group.fixtures)) {
				const f_id = parseInt(f_id_raw);

				// Group-specific selection.
				if (
					state.selection.fixtures_in_group.length !== 0 &&
					state.selection.group_ids[0] == g_id
				) {
					if (state.selection.fixtures_in_group.includes(f_id)) {
						onlyFixtures.set(f_id, true);
					} else {
						onlyFixtures.set(f_id, false);
					}

					continue;
				}

				// Normal selection.
				if (state.selection.group_ids.includes(g_id as any as number)) {
					entireGroup = true;
				}
			}

			selectionMapTemp.set(g_id, {
				onlyFixtures,
				entireGroup
			} as GroupSelection);
		}

		// TODO: does this work
		selectionMap = selectionMapTemp;
		// console.log(selectionMap)
	}

	let component: HTMLElement | null = null;

	// function dispatch(body: any) {
	// 	// component!.dispatchEvent(
	// 	// 	new CustomEvent('select', {
	// 	// 		bubbles: true,
	// 	// 		detail: body
	// 	// 	})
	// 	// );
	//   on_control()
	// }

	function handleSelect(detail: SelectionEvent) {
    console.log("handle select")

		if (detail.fixture !== undefined) {
			if (detail.selected) {
				on_control({
					LimitSelectionToFixtureInCurrentGroup: detail.fixture
				});
			} else {
				on_control({
					UnLimitSelectionToFixtureInCurrentGroup: detail.fixture
				});
			}
		} else if (detail.group !== undefined) {
			if (detail.selected) {
				on_control({
					SelectGroup: detail.group
				});
			} else {
				on_control({
					DeSelectGroup: detail.group
				});
			}
		}
	}

	function handleChange(ev: ChangeEvent) {
		on_control({
			PushSelection: null
		});

		on_control({
			SelectGroup: ev.groupID
		});

		on_control({
			LimitSelectionToFixtureInCurrentGroup: ev.fixID
		});

		// TODO: convert key to action
		on_control(keyToAction(ev.key, ev.newValue));

		on_control({
			PopSelection: null
		});
	}

	function keyToAction(key: string, value: any): any {
		switch (key) {
			case 'brightness':
				return { SetBrightness: value };
		}
	}
</script>

<div class="editor" bind:this={component}>
	{#if state}
		{#each Object.entries(state.groups) as group_arr}
			<FixtureGroup
				{selectionMap}
				groupKey={parseInt(group_arr[0])}
				group={group_arr[1]}
				on_select={handleSelect}
        on_change={handleChange}
			></FixtureGroup>
		{/each}
	{:else}
		Loading...
	{/if}
</div>

<style lang="scss">
	.editor {
		width: 100%;
		box-sizing: border-box;
		overflow-y: auto;
		// height: 10rem;
	}
</style>
