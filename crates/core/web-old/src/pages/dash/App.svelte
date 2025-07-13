<script lang="ts">
  import { onMount } from "svelte";
  import Page from "../../Page.svelte";
  import { loading } from "../../global";
  import {
    Blade,
    Button,
    Folder,
    FpsGraph,
    List,
    Monitor,
    Separator,
    ThemeUtils,
    type ListOptions,
    Text,
    TabGroup,
    TabPage,
  } from "svelte-tweakpane-ui";
  import { Binding, type BindingObject } from "svelte-tweakpane-ui";
  import {
    BlaulichtWebsocket,
    BlaulichtWebsocketCallbacks,
    topicAudioDevicesView,
    topicBass,
    topicBassAvg,
    topicBassAvgShort,
    topicBeatVolume,
    topicBPM,
    topicControl,
    topicDMX,
    topicHeartbeat,
    topicLog,
    topicLoopSpeed,
    topicSelectAudioDevice,
    topicTickSpeed,
    topicVolume,
    topicWasmControlsConfig,
    topicWasmControlsLog,
    topicWasmControlsSet,
    topicWasmLog,
  } from "../../lib/websocket";
  import { WaveformMonitor } from "svelte-tweakpane-ui";
  import BpmLight from "../../components/BPMLight.svelte";
  import Dmx4Chan from "../../components/Dmx4Chan.svelte";
  import DmxPanel2Chan from "../../components/DmxPanel2Chan.svelte";
  import Terminal from "../../components/Terminal.svelte";
  import ControlMatrix from "../../components/ControlMatrix.svelte";
  import { CalculationInterpolation } from "sass";
  import StageEditor from "../../components/editor/StageEditor.svelte";
  import type { Data, SystemMetrics } from "../../lib/types";
  import MainPage from "../../components/pages/Main.svelte";
  // import { midi } from '../../lib/midi';

  async function loadAvailableAudioDevices(): Promise<String[]> {
    let res = (await fetch("/api/audio/devices")).json();
    // console.log(res)
    return res;
  }

  async function loadAvailableSerialDevices(): Promise<String[]> {
    let res = (await fetch("/api/serial/devices")).json();
    // console.log(res)
    return res;
  }

  //
  // Serial devices.
  //

  // Filled by web request.
  // let serialDevices = [""]

  let serialPortListOptions: ListOptions<string> = {};
  let selectedSerial = null;

  // Filled by web request.
  // let audioDevices = [""]

  let audioPortListOptions: ListOptions<string> = {};
  // $: console.dir(audioPortListOptions)
  let selectedAudio = null;


  let controlInput = "";

  audioPortListOptions["None"] = "None";

  let dmx = new Array(513);

  let logs: string[] = [];
  let wasmLogs: string[] = [];

  let data: Data = null;

  async function getData() {
    const res = await (await fetch("/api/state")).json();
    return res;
  }

  async function fetchDataDaemon() {
    data = await getData();
    setTimeout(fetchDataDaemon, 100);
  }

  let socket: BlaulichtWebsocket | null = null;

  onMount(async () => {
    $loading = true;
    ThemeUtils.setGlobalDefaultTheme(ThemeUtils.presets.iceberg);

    fetchDataDaemon();

    // Connect socket.

    // Serial devices.
    // for (let dev of serialDevices) {
    //     serialPortListOptions[dev] = dev
    // }

    // Audio devices.
    // for (let dev of audioDevices) {
    //     audioPortListOptions[dev] = dev
    // }

    // Waveform demo.
    // setInterval(() => {
    //     numberToMonitor = Math.random() * 100;
    // }, 50);

    // midi();

    $loading = false;
  });

  let metrics: SystemMetrics = {
    loopSpeed: 0,
    tickSpeed: 0,
    audio: {
      volume: 0,
      bpm: 0,
      bass: 0,
      bassAvgShort: 0,
      bassAvg: 0,
      beatVolume: 0,
    },
  };

  // let volume = 85;
  // let loopSpeed = 0;
  // let tickSpeed = 0;
  // let bpm = 0;
  let controlMatrixConfig: {
    rows: number;
    cols: number;
    labels: string[];
  } = {
    rows: 0,
    cols: 0,
    labels: [],
  };
  let controlMatrixStates = [];

  async function selectAudio(device: string | any) {
    socket.send({
      kind: "SelectAudioDevice",
      value: device,
    });
  }

  async function selectSerial(device: string | any) {
    socket.send({
      kind: "SelectSerialDevice",
      value: device,
    });
  }

  async function reloadEngine() {
    socket.send({
      kind: "Reload",
      value: null,
    });
  }

  async function sendControlWrapper() {
    try {
      let parsed = JSON.parse(controlInput);
      sendControl(parsed);
    } catch (e) {
      alert(e);
    }
  }

  async function sendControl(data: any) {
    socket.send({
      kind: "Control",
      value: data,
    });
  }

  async function handleEditorControl(event: CustomEvent<any>) {
    await sendControl(event.detail);
  }
</script>

<Page pageId="dash">
  <div
    class="page"
    style="height: 100vh; display: flex; flex-direction: column;"
  >
    <TabGroup>
      <TabPage title="SYS">
        <MainPage {logs} {wasmLogs} {metrics}></MainPage>
      </TabPage>
      <TabPage title="FIX">TODO: fix</TabPage>
      <TabPage title="SETTINGS">
        <Folder userExpandable={false} expanded={true} title="Devices">
          <List
            bind:value={selectedAudio}
            label="Audio Input"
            options={audioPortListOptions}
            on:change={(e) => selectAudio(e.detail.value)}
          />
          <pre>Selected Option: {selectedAudio}</pre>

          <Button on:click={reloadEngine} label={"Engine"} title="Reload"
          ></Button>
        </Folder>

        <!-- <div style="width: 100%; height: 70vh; overflow-y: scroll;">
          <div
            style="width: 100%; padding: 1rem; box-sizing: border-box; display: flex; justify-content:center;"
          >
            <ControlMatrix
              config={controlMatrixConfig}
              states={controlMatrixStates}
              on:buttonToggle={(e) => {
                const x = Math.floor(e.detail.index / controlMatrixConfig.cols);
                const y = e.detail.index % controlMatrixConfig.cols;

                console.log(`Button ${x} ${y} toggled to ${e.detail.state}`);

                socket.send({
                  kind: "MatrixControl",
                  value: {
                    device: 1,
                    x,
                    y,
                    value: e.detail.state,
                  },
                });
              }}
            ></ControlMatrix> -->

            <!-- <div style="width: 100%;"> -->
              <!-- List fixtures & groups -->

              <!-- <Folder userExpandable={false} expanded={true} title="Control DBG">
          <Text bind:value={controlInput} label="Input (JSON)"></Text>
          <Button on:click={sendControlWrapper} label={"Control"} title="Send"
          ></Button>
        </Folder>

        {#if data}
          <StageEditor state={data.dmx_engine} on:control={handleEditorControl}
          ></StageEditor>
        {/if} -->
            <!-- </div> -->
          <!-- </div>
        </div> -->
      </TabPage>
    </TabGroup>
  </div>
</Page>

<style lang="scss">
  @use "../../mixins" as *;
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    // overflow: hidden;
    height: 100vh;
  }

  :global(h6) {
    margin: 0;
  }
</style>
