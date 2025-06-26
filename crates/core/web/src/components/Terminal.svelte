<script>
  import { afterUpdate } from "svelte";

  export let lines = [];

  let terminalRef;

  // Automatically scroll to bottom when new lines are added
  afterUpdate(() => {
    terminalRef.scrollTop = terminalRef.scrollHeight;
  });
</script>

<div class="terminal" bind:this={terminalRef}>
  {#each lines as line}
    <div class="line">{line}</div>
  {/each}
</div>

<style>
  .terminal {
    background: hsla(230, 20%, 8%, 1.00);
    color: hsla(230, 10%, 80%, 1.00);
    font-family: "Courier New", monospace;
    padding: 1rem;
    border-radius: 8px;
    height: 100%;
    overflow-y: auto;
    white-space: pre-wrap;
    box-shadow: 0 0 10px rgba(0, 0, 0, 0.5);
    box-sizing: border-box;
  }

  .line {
    line-height: 1.4;
  }

  .btn {
    margin-top: 1rem;
    padding: 0.5rem 1rem;
    background: #333;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }

  .btn:hover {
    background: #555;
  }
</style>
