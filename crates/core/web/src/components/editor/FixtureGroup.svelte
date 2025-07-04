<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import type { FixtureGroup, GroupSelection } from "../../lib/types";
  import Fixture from "./Fixture.svelte";
  import { Button } from "svelte-tweakpane-ui";

  export let groupKey = 0;
  export let group: FixtureGroup = null;

  export let selectionMap: Map<number, GroupSelection> = null;
  let selected = false;
  $: selected = selectionMap.get(groupKey).entireGroup;

  let dispatch = createEventDispatcher();

  $: console.log(groupKey);

  // onMount(() => {

  // })

  function toggleSelection() {
    dispatch("select", {
      group: groupKey as any as number,
      fixture: null,
      selected: !selected,
    });
  }

  function propagateSelect(event: CustomEvent<any>) {
    dispatch("select", event.detail);
  }
</script>

<div class="fix-group" class:selected>
  <h4>GROUP {groupKey}</h4>
  <Button on:click={toggleSelection} label="(de) select"></Button>

  {#each Object.entries(group.fixtures) as fixture_arr}
    <Fixture
      selected={selectionMap
        .get(groupKey)
        .onlyFixtures.get(parseInt(fixture_arr[0]))}
      fixtureKey={parseInt(fixture_arr[0])}
      fixture={fixture_arr[1]}
      on:select={propagateSelect}
    ></Fixture>
  {/each}
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

    &.selected {
      border-color: greenyellow;
    }

    &:hover {
      background-color: #606060;
      cursor: pointer;
    }
  }
</style>
