<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import type { EngineState } from "../../lib/types";
  import FixtureGroup from "./FixtureGroup.svelte";

  export let state: EngineState = null;
  $: if (state) buildSelectionMap(state);

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
        entireGroup,
      } as GroupSelection);
    }

    // TODO: does this work
    selectionMap = selectionMapTemp;
    // console.log(selectionMap)
  }

  interface SelectionEvent {
    group?: number;
    fixture?: number;
    selected: boolean;
  }

  let dispatch = createEventDispatcher();

  function handleSelect(event: CustomEvent<SelectionEvent>) {
    let detail = event.detail as SelectionEvent;

    if (detail.fixture !== null) {
      if (detail.selected) {
        dispatch("control", {
          LimitSelectionToFixtureInCurrentGroup: detail.fixture,
        });
      } else {
        dispatch("control", {
          UnLimitSelectionToFixtureInCurrentGroup: detail.fixture,
        });
      }
    } else if (detail.group !== null) {
      if (detail.selected) {
        dispatch("control", {
          SelectGroup: detail.group,
        });
      } else {
        dispatch("control", {
          DeSelectGroup: detail.group,
        });
      }
    }
  }
</script>

<div class="editor">
  {#if state}
    {#each Object.entries(state.groups) as group_arr}
      <FixtureGroup
        {selectionMap}
        groupKey={parseInt(group_arr[0])}
        group={group_arr[1]}
        on:select={handleSelect}
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