<script>
    import { afterUpdate } from 'svelte';
  
    export let lines = []
  
    let terminalRef;
  
    // Automatically scroll to bottom when new lines are added
    afterUpdate(() => {
      terminalRef.scrollTop = terminalRef.scrollHeight;
    });
  </script>
  
  <style>
    .terminal {
      background-color: #1e1e1e;
      color: #d4d4d4;
      font-family: 'Courier New', monospace;
      padding: 1rem;
      border-radius: 8px;
      height: 300px;
      overflow-y: auto;
      white-space: pre-wrap;
      box-shadow: 0 0 10px rgba(0, 0, 0, 0.5);
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
  
  <div class="terminal" bind:this={terminalRef}>
    {#each lines as line}
      <div class="line">{line}</div>
    {/each}
  </div>
  