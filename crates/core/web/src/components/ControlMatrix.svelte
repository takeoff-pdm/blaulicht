<script lang="ts">
  import { createEventDispatcher } from "svelte";

  export let config: { rows: number; cols: number; labels: string[] };
  export let states: boolean[];

  const dispatch = createEventDispatcher();

  function toggleButton(index: number) {
    //   states[index] = !states[index];
    dispatch("buttonToggle", { index, state: states[index] });
  }

  let totalButtons = 0;

  // Grid size validation
  $: if (config) {
    totalButtons = config.rows * config.cols;
  }

  $: if (
    (config && config.labels.length !== totalButtons) ||
    states.length !== totalButtons
  ) {
    console.warn(
      "Launchpad: Labels and states must match total buttons (rows x cols)"
    );
  }
</script>

{#if config}
  <div
    class="launchpad"
    style="grid-template-columns: repeat({config.cols}, 1fr); max-width: {config.cols *
      120}px;"
  >
    {#each Array(totalButtons) as _, index}
      <button
        class="button {states[index] ? 'on' : ''}"
        on:click={() => toggleButton(index)}
      >
        {@html config.labels[index]}
      </button>
    {/each}
  </div>
{/if}

<style>
  .launchpad {
    display: grid;
    gap: 8px;
    width: 100%;
    /* grid-template-columns: repeat(auto-fit, minmax(0, 100px)); */
  }
  .button {
    width: 100%;
    max-width: 120px;
    aspect-ratio: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: hsla(230, 20%, 8%, 1.00);
    border: solid 1px #2b2f46;
    border-radius: 6px;
    color: hsla(230, 10%, 80%, 1.00);
    font-weight: bold;
    cursor: pointer;
    user-select: none;
    font-family: monospace;
    font-size: 0.65rem;
  }
  .button.on {
    border: solid 1px cornflowerblue;
  }
</style>
