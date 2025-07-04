<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { Fixture, FixtureGroup } from "../../lib/types";
  import { Button } from "svelte-tweakpane-ui";
  export let fixtureKey = 0;
  export let fixture: Fixture = null;

  export let selected = false;
  $: console.log(selected);

  let dispatch = createEventDispatcher();

  function toggleSelection(e: any) {
    dispatch("select", {
      group: null,
      fixture: fixtureKey as any as number,
      selected: !selected,
    });
  }
</script>

<div class="fixture" class:selected>
  <h4>Fixture {fixtureKey}</h4>
  <!-- {groupKey} -->

  {fixture.name}

  <table>
    <tr>
      <th>Key</th>
      <th>Value</th>
    </tr>
    {#each Object.entries(fixture.state) as [key, value]}
      <tr>
        <td><code>{key}</code></td>
        <td><code>{JSON.stringify(value)}</code></td>
      </tr>
    {/each}

      <tr>
        <td><code>MODEL</code></td>
        <td><code>{JSON.stringify(fixture.type_)}</code></td>
      </tr>
  </table>

  <Button on:click={toggleSelection} label="(de) select"></Button>
</div>

<style lang="scss">
  table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 1rem;
  }

  th, td {
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
