<script lang="ts">
	import Terminal from '$lib/components/Terminal.svelte';
	import StageEditor from '$lib/components/stage_editor/StageEditor.svelte';
	import * as Card from '$lib/components/ui/card';

	// import { page } from '$app/state';
	import type { SystemMetrics } from '$lib/types/metrics';
	import {
		BlaulichtWebsocket,
		BlaulichtWebsocketCallbacks,
		topicBass,
		topicBassAvg,
		topicBassAvgShort,
		topicBeatVolume,
		topicBPM,
		topicControl,
		topicHeartbeat,
		topicLog,
		topicLoopSpeed,
		topicSelectAudioDevice,
		topicTickSpeed,
		topicVolume,
		topicWasmLog
	} from '$lib/websocket';
	import { onMount } from 'svelte';
	import AudioStatus from './audio-status.svelte';
	import { getData } from '$lib/fetch';
	import type { Data } from '$lib/types/state';
	import SystemStatus from './system-status.svelte';
	import StageViewer from '$lib/components/stage_viewer/StageViewer.svelte';
	import PluginStatus from './plugin-status.svelte';

	export let data: Data | null = null;

	export async function fetchDataDaemon() {
		data = await getData();
		logs = data.logs;
		setTimeout(fetchDataDaemon, 100);
	}

	let logs: string[] = [];

	let wasmLogs: string[] = [];
	let metrics: SystemMetrics = {
		loopSpeed: 0,
		tickSpeed: 0,
		audio: {
			volume: 0,
			beatVolume: 0,
			bass: 0,
			bassAvg: 0,
			bassAvgShort: 0,
			bpm: 0
		}
	};

	let heartbeat = false;

	let socket: BlaulichtWebsocket | null = null;

	async function sendControl(data: any) {
		console.log('Send control', data);
		socket!.send({
			kind: 'Control',
			value: data
		});
	}

	onMount(() => {
		const callbacks = new BlaulichtWebsocketCallbacks();
		callbacks.subscribe(topicHeartbeat(), (event) => {
			// console.log(`Heartbeat: ${event.value}`);
			// TODO: !!!
			heartbeat = !heartbeat;
		});

		// TODO: what
		// data = await getData();

		// callbacks.subscribe(topicAudioDevicesView(), (event) => {
		// 	const devices = event.value;

		// 	let audioPortListOptionsTemp = {};

		// 	for (let dev of devices) {
		// 		audioPortListOptionsTemp[dev] = dev;
		// 	}

		// 	if (JSON.stringify(audioPortListOptions) === JSON.stringify(audioPortListOptionsTemp)) {
		// 		// Use old state.
		// 		return;
		// 	}

		// 	audioPortListOptions = audioPortListOptionsTemp;
		// });

		// callbacks.subscribe(topicDMX(), (event) => {
		// 	const msg = event.value;
		// 	// console.log(`[DMX]: ${msg}`)
		// 	dmx = event.value;
		// });

		callbacks.subscribe(topicLog(), (event) => {
			const msg = event.value;
			//   console.log(`[LOG]: ${msg}`);
			if (logs.length > 500) {
				logs.splice(0, 1);
			}

			logs = [...logs, msg];
		});

		callbacks.subscribe(topicWasmLog(), (event) => {
			const msg = event.value;
			//   console.log(`[LOG]: ${msg}`);
			// if (logs.length > 500) {
			// 	wasmLogs.splice(0, 1);
			// }

			wasmLogs = [...wasmLogs, `[${msg.plugin_id}]: ${msg.msg}`];
		});

		// callbacks.subscribe(topicWasmControlsLog(), (event) => {
		// 	const x = event.value.x;
		// 	const y = event.value.y;
		// 	const msg = event.value.value;

		// 	controlMatrixConfig.labels[y * controlMatrixConfig.cols + x] = msg;
		// 	console.log(
		// 		`Updated control matrix label: ${x}, ${y} = ${msg} (${controlMatrixConfig.labels})`
		// 	);
		// });

		// callbacks.subscribe(topicWasmControlsSet(), (event) => {
		// 	const x = event.value.x;
		// 	const y = event.value.y;
		// 	const value = event.value.value;
		// 	controlMatrixStates[y * controlMatrixConfig.cols + x] = value;

		// 	console.log(`Updated control matrix state: ${x}, ${y} = ${value}`);
		// });

		// callbacks.subscribe(topicWasmControlsConfig(), (event) => {
		// 	const x = event.value.x;
		// 	const y = event.value.y;

		// 	controlMatrixConfig = {
		// 		rows: x,
		// 		cols: y,
		// 		labels: new Array(y * x).fill('/')
		// 	};

		// 	controlMatrixStates = new Array(controlMatrixConfig.rows * controlMatrixConfig.cols).fill(
		// 		false
		// 	);

		// 	console.log(`Control matrix config: ${controlMatrixConfig}`);
		// });

		callbacks.subscribe(topicSelectAudioDevice(), (event) => {
			const dev = event.value;
			console.log(`Selected audio device: ${dev}`);

			if (data) {
				// data.audio.device_name = dev
			}
		});

		callbacks.subscribe(topicVolume(), (event) => {
			// console.log(`Volume: ${event.value}`)
			// waveData.splice(0, 1)
			// waveData = [...waveData, event.value]
			metrics.audio.volume = event.value;
		});

		callbacks.subscribe(topicBass(), (event) => {
			// console.log(`Bass: ${event.value}`)
			metrics.audio.bass = event.value;
		});

		callbacks.subscribe(topicBassAvgShort(), (event) => {
			// console.log(`Bass: ${event.value}`)
			metrics.audio.bassAvgShort = event.value;
		});

		callbacks.subscribe(topicBassAvg(), (event) => {
			// console.log(`Bass: ${event.value}`)
			metrics.audio.bassAvg = event.value;
		});

		callbacks.subscribe(topicBeatVolume(), (event) => {
			// console.log(`Beat volume: ${event.value}`)
			metrics.audio.beatVolume = event.value;
		});

		callbacks.subscribe(topicLoopSpeed(), (event) => {
			// console.log(`Beat volume: ${event.value}`)
			metrics.loopSpeed = event.value;
		});

		callbacks.subscribe(topicTickSpeed(), (event) => {
			// console.log(`Beat volume: ${event.value}`)
			metrics.tickSpeed = event.value;
		});

		callbacks.subscribe(topicBPM(), (event) => {
			metrics.audio.bpm = event.value;
		});

		callbacks.subscribe(topicControl(), (event) => {
			console.log('control', event);
		});

		const socketTemp = new BlaulichtWebsocket(callbacks);
		socket = socketTemp;

		fetchDataDaemon();
	});

	interface SelectEvent {
		group: number;
		fixture: number;
		selected: boolean;
	}

	const selectHandler = (ev: SelectEvent) => {};

	function on_reload() {
		socket!.send({
			kind: 'Reload',
			value: null
		});
	}
</script>

<div class="col-container">
	<Card.Root class="h-full w-1/3">
		<Card.Header>
			<Card.Title>System</Card.Title>
		</Card.Header>
		<Card.Content class="flex h-full flex-col gap-2">
			<AudioStatus
				className="h-2/3 w-full"
				audio={metrics.audio}
				audioDevice={data?.audio?.device_name}
			></AudioStatus>

			<!-- <Card.Root class="h-1/3 w-full">
				<Card.Header>
					<Card.Title>System</Card.Title>
				</Card.Header>
				<Card.Content></Card.Content>
			</Card.Root> -->

			<Card.Root class="h-1/3 w-full">
				<Card.Header>
					<Card.Title>System</Card.Title>
				</Card.Header>
				<Card.Content>
					{#if data}
						<SystemStatus {on_reload} sysState={data} {metrics} {heartbeat}></SystemStatus>
					{/if}
				</Card.Content>
			</Card.Root>
		</Card.Content>
	</Card.Root>

	<Card.Root class="h-full w-2/3">
		<Card.Header>
			<Card.Title>Foo</Card.Title>
		</Card.Header>
		<Card.Content class="h-full">
			<Card.Root class="h-1/2">
				<Card.Header>
					<Card.Title>Bar</Card.Title>
				</Card.Header>
				<Card.Content>
					<StageViewer state={data?.dmx_engine} on_control={sendControl}></StageViewer>
				</Card.Content>
			</Card.Root>
			<Card.Root class="h-1/2">
				<Card.Header>
					<Card.Title>Bar</Card.Title>
				</Card.Header>
				<Card.Content>
					<div class="flex h-full">
						<div class="w-2/8">
							{#if data && data.plugins}
								<PluginStatus sysState={data!.plugins}></PluginStatus>
							{/if}
						</div>
						<div class="w-6/8 h-full flex">
							<Terminal className="h-full" lines={logs}></Terminal>
							<Terminal className="h-full" lines={wasmLogs}></Terminal>
						</div>
					</div>
				</Card.Content>
			</Card.Root>
		</Card.Content>
	</Card.Root>

	<!-- <div class="terminals">
		<Terminal title="Terminal" class='w-1/2' lines={logs}></Terminal>
		<Terminal class='w-1/2' lines={wasmLogs}></Terminal>
	</div> -->
</div>

<style lang="scss">
	.col-container {
		width: 100%;
		height: 100%;
		display: flex;

		gap: 1rem;
		padding: 1rem;

		box-sizing: border-box;
		flex-shrink: 0;
	}
</style>
