<script lang="ts">
  import { Blade, Button, Folder, List, Monitor } from "svelte-tweakpane-ui";
  import BpmLight from "../BPMLight.svelte";
  import ControlMatrix from "../ControlMatrix.svelte";
  import Terminal from "../Terminal.svelte";
  import type { SystemMetrics } from "../../lib/types";

  export let metrics: SystemMetrics = {
    loopSpeed: 0,
    tickSpeed: 0,
    audio: {
      volume: 0,
      bass: 0,
      bassAvg: 0,
      bassAvgShort: 0,
      bpm: 0,
      beatVolume: 0,
    },
  };

  export let logs = [];
  export let wasmLogs = [];
</script>

<div style="height: 100%; width: 100%;">
  <div style="width: 100%; height: 70%; display: flex; flex: 1; background-color: red;">
    <div
      style="width: 100%; display: flex; flex-direction: column; justify-content: space-between; padding-bottom: 1rem;"
    >
      <Folder userExpandable={false} expanded={true} title="System">
        <Monitor value={metrics.loopSpeed} graph={false} label={"Loop Speed"} />
        <Monitor
          value={metrics.loopSpeed}
          graph={true}
          max={200}
          format={(v) => `${v} micro s`}
        />

        <Blade
          options={{
            view: "separator",
          }}
        />

        <Monitor value={metrics.tickSpeed} graph={false} label={"Tick Speed"} />
        <Monitor
          value={metrics.tickSpeed}
          graph={true}
          max={100}
          format={(v) => `${v} micro s`}
        />
      </Folder>

      <Folder userExpandable={false} expanded={true} title="Audio">
        <Monitor value={metrics.audio.volume} graph={false} label={"Volume"} />
        <Monitor value={metrics.audio.volume} graph={true} max={300} />

        <Blade
          options={{
            view: "separator",
          }}
        />

        <Monitor value={metrics.audio.bass} graph={false} label={"Bass"} />
        <Monitor value={metrics.audio.bass} graph={true} max={300} />

        <Blade
          options={{
            view: "separator",
          }}
        />

        <Monitor
          value={metrics.audio.bassAvg}
          graph={false}
          label={"Bass AVG."}
        />
        <Monitor value={metrics.audio.bassAvg} graph={true} max={300} />
      </Folder>

      <Folder userExpandable={false} expanded={true} title="Beat Detection">
        <Monitor
          value={metrics.audio.bassAvgShort}
          graph={false}
          label={"Bass Peak"}
        />
        <Monitor value={metrics.audio.bassAvgShort} graph={true} max={300} />

        <Blade
          options={{
            view: "separator",
          }}
        />

        <Monitor value={metrics.audio.bpm} graph={false} label={"BPM"} />
        <Monitor value={metrics.audio.bpm} graph={true} max={160} min={90} />

        <BpmLight bpm={metrics.audio.bpm} dimensions={80}></BpmLight>
      </Folder>
    </div>
  </div>

</div>
