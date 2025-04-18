<script lang="ts">
    import { onMount } from "svelte";
    import Page from "../../Page.svelte";
    import { loading } from "../../global";
    import {
        Button,
        Folder,
        FpsGraph,
        List,
        Monitor,
        ThemeUtils,
        type ListOptions,
    } from "svelte-tweakpane-ui";
    import { Binding, type BindingObject } from "svelte-tweakpane-ui";
    import {
        BlaulichtWebsocket,
        BlaulichtWebsocketCallbacks,
        topicAudioDevicesView,
        topicBass,
        topicBassAvg,
        topicBeatVolume,
        topicBPM,
        topicDMX,
        topicHeartbeat,
        topicLog,
        topicLoopSpeed,
        topicSelectAudioDevice,
        topicTickSpeed,
        topicVolume,
    } from "../../lib/websocket";
    import { WaveformMonitor } from "svelte-tweakpane-ui";
    import BpmLight from "../../components/BPMLight.svelte";
    import Dmx4Chan from "../../components/Dmx4Chan.svelte";
    import DmxPanel2Chan from "../../components/DmxPanel2Chan.svelte";
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

    let socket: BlaulichtWebsocket | null = null;

    audioPortListOptions["None"] = "None";

    let bass = 0;
    let bassAvg = 0;
    let beatVolume = 0;

    let dmx = new Array(513);

    onMount(async () => {
        $loading = true;
        ThemeUtils.setGlobalDefaultTheme(ThemeUtils.presets.retro);

        // Connect socket.
        const callbacks = new BlaulichtWebsocketCallbacks();
        callbacks.subscribe(topicHeartbeat(), (event) => {
            console.log(`Heartbeat: ${event.value}`);
        });

        callbacks.subscribe(topicAudioDevicesView(), (event) => {
            const devices = event.value;

            let audioPortListOptionsTemp = {};

            for (let dev of devices) {
                audioPortListOptionsTemp[dev] = dev;
            }

            if (
                JSON.stringify(audioPortListOptions) ===
                JSON.stringify(audioPortListOptionsTemp)
            ) {
                // Use old state.
                return;
            }

            audioPortListOptions = audioPortListOptionsTemp;
        });

        callbacks.subscribe(topicDMX(), (event) => {
            const msg = event.value;
            // console.log(`[DMX]: ${msg}`)
            dmx = event.value;
        });

        callbacks.subscribe(topicLog(), (event) => {
            const msg = event.value;
            console.log(`[LOG]: ${msg}`);
        });

        callbacks.subscribe(topicSelectAudioDevice(), (event) => {
            const dev = event.value;
            console.log(`Selected audio device: ${dev}`);
            selectedAudio = dev;
        });

        callbacks.subscribe(topicSelectAudioDevice(), (event) => {
            const dev = event.value;
            console.log(`Selected audio device: ${dev}`);
            selectedAudio = dev;
        });

        callbacks.subscribe(topicVolume(), (event) => {
            // console.log(`Volume: ${event.value}`)
            // waveData.splice(0, 1)
            // waveData = [...waveData, event.value]
            volume = event.value;
        });

        callbacks.subscribe(topicBass(), (event) => {
            // console.log(`Bass: ${event.value}`)
            bass = event.value;
        });

        callbacks.subscribe(topicBassAvg(), (event) => {
            // console.log(`Bass: ${event.value}`)
            bassAvg = event.value;
        });

        callbacks.subscribe(topicBeatVolume(), (event) => {
            // console.log(`Beat volume: ${event.value}`)
            beatVolume = event.value;
        });

        callbacks.subscribe(topicLoopSpeed(), (event) => {
            // console.log(`Beat volume: ${event.value}`)
            loopSpeed = event.value;
        });

        callbacks.subscribe(topicTickSpeed(), (event) => {
            // console.log(`Beat volume: ${event.value}`)
            tickSpeed = event.value;
        });

        callbacks.subscribe(topicBPM(), (event) => {
            bpm = event.value;
        });

        socket = new BlaulichtWebsocket(callbacks);

        // Serial devices.
        // for (let dev of serialDevices) {
        //     serialPortListOptions[dev] = dev
        // }

        // Audio devices.
        // for (let dev of audioDevices) {
        //     audioPortListOptions[dev] = dev
        // }

        // Waveform demo.
        setInterval(() => {
            waveData = waveData.map((v) =>
                Math.max(0, Math.min(10, v + (Math.random() * 2 - 1) * 0.5)),
            );
        }, 50);

        // setInterval(() => {
        //     numberToMonitor = Math.random() * 100;
        // }, 50);

        // midi();

        $loading = false;
    });

    let waveData = [5, 6, 7, 8, 9, 3, 9, 8, 7, 6, 5];
    let volume = 85;
    let loopSpeed = 0;
    let tickSpeed = 0;
    let bpm = 0;

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
</script>

<Page pageId="dash">
    <div class="page">
        <div style="width: 100%; display: flex;">
            <div style="width: 60%;">
                <div style="display: flex;">
                    <div style="width: 90%">
                        <Monitor
                            value={loopSpeed}
                            graph={true}
                            max={100}
                            theme={ThemeUtils.presets.retro}
                            format={(v) => `${v} micro s`}
                        />
                    </div>
                    <div style="width: 10%">
                        <span>LOOP SPEED</span>
                        <Monitor value={loopSpeed} graph={false} />
                    </div>
                </div>

                <div style="display: flex;">
                    <div style="width: 90%">
                        <Monitor
                            value={tickSpeed}
                            graph={true}
                            max={50}
                            theme={ThemeUtils.presets.retro}
                            format={(v) => `${v} micro s`}
                        />
                    </div>
                    <div style="width: 10%">
                        <span>WASM SPEED</span>
                        <Monitor value={tickSpeed} graph={false} />
                    </div>
                </div>

                <div style="display: flex;">
                    <div style="width: 90%">
                        <Monitor
                            value={volume}
                            graph={true}
                            max={300}
                            theme={ThemeUtils.presets.retro}
                        />
                    </div>
                    <div style="width: 10%">
                        <span>VOLUME</span>
                        <Monitor value={volume} graph={false} />
                    </div>
                </div>

                <div style="display: flex;">
                    <div style="width: 90%">
                        <Monitor
                            value={bass}
                            graph={true}
                            max={300}
                            theme={ThemeUtils.presets.retro}
                        />
                    </div>
                    <div style="width: 10%">
                        <span>BASS</span>
                        <Monitor value={bass} graph={false} />
                    </div>
                </div>

                <div style="display: flex;">
                    <div style="width: 90%">
                        <Monitor
                            value={bassAvg}
                            graph={true}
                            max={300}
                            theme={ThemeUtils.presets.retro}
                        />
                    </div>
                    <div style="width: 10%">
                        <span>BASS AVG.</span>
                        <Monitor value={bassAvg} graph={false} />
                    </div>
                </div>

                <div style="display: flex;">
                    <div style="width: 90%">
                        <Monitor
                            value={beatVolume}
                            graph={true}
                            max={300}
                            theme={ThemeUtils.presets.retro}
                        />
                    </div>
                    <div style="width: 10%">
                        <span>BEAT VOL.</span>
                        <Monitor value={beatVolume} graph={false} />
                    </div>
                </div>

                <div style="display: flex;">
                    <div style="width: 90%">
                        <Monitor
                            value={bpm}
                            graph={true}
                            max={160}
                            min={90}
                            theme={ThemeUtils.presets.retro}
                        />
                    </div>
                    <div style="width: 10%">
                        <span>BPM</span>
                        <Monitor value={bpm} graph={false} />
                    </div>

                    <BpmLight {bpm} dimensions={80}></BpmLight>
                </div>

                <!-- <WaveformMonitor value={waveData} min={-1} max={11} lineStyle={'bezier'} /> -->

                <!-- <Folder expanded={true} title="Reticulation Management Folder"> -->
                <!--     <Button on:click={() => console.log("incr")} title="Increment" /> -->
                <!--     <Monitor value={0} label="Count" /> -->
                <!-- </Folder> -->
            </div>

            <div style="width: 40%;">
                <Folder userExpandable={false} expanded={true} title="Devices">
                    <List
                        bind:value={selectedSerial}
                        label="Serial Port"
                        options={serialPortListOptions}
                        on:change={(e) => selectSerial(e.detail.value)}
                    />
                    <pre>Selected Option: {selectedSerial}</pre>

                    <List
                        bind:value={selectedAudio}
                        label="Audio Input"
                        options={audioPortListOptions}
                        on:change={(e) => selectAudio(e.detail.value)}
                    />
                    <pre>Selected Option: {selectedAudio}</pre>
                </Folder>

                <Button on:click={reloadEngine} title="Reload"></Button>

                <div>
                    <Dmx4Chan
                        brightness={dmx[21]}
                        red={dmx[22]}
                        green={dmx[23]}
                        blue={dmx[24]}
                    ></Dmx4Chan>

                    <br>

                    <Dmx4Chan
                        brightness={255}
                        red={dmx[250]}
                        green={dmx[251]}
                        blue={dmx[252]}
                    ></Dmx4Chan>

                    <br />

                    <div style="display: flex; flex-wrap: wrap;">
                        <DmxPanel2Chan
                            brightness={dmx[1]}
                            colorTemp={dmx[2]}
                        ></DmxPanel2Chan>

                        <DmxPanel2Chan
                            brightness={dmx[17]}
                            colorTemp={dmx[18]}
                        ></DmxPanel2Chan>

                        <DmxPanel2Chan
                            brightness={dmx[33]}
                            colorTemp={dmx[34]}
                        ></DmxPanel2Chan>

                        <DmxPanel2Chan
                            brightness={dmx[49]}
                            colorTemp={dmx[50]}
                        ></DmxPanel2Chan>

                        <DmxPanel2Chan
                            brightness={dmx[81]}
                            colorTemp={dmx[82]}
                        ></DmxPanel2Chan>

                        <DmxPanel2Chan
                            brightness={dmx[129]}
                            colorTemp={dmx[130]}
                        ></DmxPanel2Chan>

                        <DmxPanel2Chan
                            brightness={dmx[65]}
                            colorTemp={dmx[66]}
                        ></DmxPanel2Chan>

                        <DmxPanel2Chan
                            brightness={dmx[161]}
                            colorTemp={dmx[162]}
                        ></DmxPanel2Chan>
                    </div>
                </div>
            </div>
        </div>
    </div>
</Page>

<style lang="scss">
    @use "../../mixins" as *;
</style>
