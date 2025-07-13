//
// Topics.
//

const WS_PATH = 'api/ws';

export enum TopicKind {
	BPM = 'Bpm',
	DMX = 'Dmx',
	Heartbeat = 'Heartbeat',
	AudioDevicesView = 'AudioDevicesView',
	AudioDeviceSelected = 'SelectAudioDevice',
	Log = 'Log',
	WasmLog = 'WasmLog',
	WasmControlsLog = 'WasmControlsLog',
	WasmControlsSet = 'WasmControlsSet',
	WasmControlsConfig = 'WasmControlsConfig',
	Volume = 'Volume',
	Bass = 'Bass',
	BassAvgShort = 'BassAvgShort',
	BassAvg = 'BassAvg',
	BeatVolume = 'BeatVolume',
	LoopSpeed = 'LoopSpeed',
	TickSpeed = 'TickSpeed',
	Control = 'Control'
}

//
// Send events
//

export interface SendEvent {
	kind: 'SelectAudioDevice' | 'SelectSerialDevice' | 'Reload' | 'MatrixControl' | 'Control';
	value: unknown;
}

//
// End send events.
//

export interface Topic<T extends TopicKind = TopicKind> {
	kind: T;
}

export function topicHeartbeat(): Topic<TopicKind.Heartbeat> {
	return { kind: TopicKind.Heartbeat };
}

export function topicBPM(): Topic<TopicKind.BPM> {
	return { kind: TopicKind.BPM };
}

export function topicDMX(): Topic<TopicKind.DMX> {
	return { kind: TopicKind.DMX };
}

export function topicAudioDevicesView(): Topic<TopicKind.AudioDevicesView> {
	return { kind: TopicKind.AudioDevicesView };
}

export function topicSelectAudioDevice(): Topic<TopicKind.AudioDeviceSelected> {
	return { kind: TopicKind.AudioDeviceSelected };
}

export function topicLog(): Topic<TopicKind.Log> {
	return { kind: TopicKind.Log };
}

export function topicWasmLog(): Topic<TopicKind.WasmLog> {
	return { kind: TopicKind.WasmLog };
}

export function topicWasmControlsLog(): Topic<TopicKind.WasmControlsLog> {
	return { kind: TopicKind.WasmControlsLog };
}

export function topicWasmControlsSet(): Topic<TopicKind.WasmControlsSet> {
	return { kind: TopicKind.WasmControlsSet };
}

export function topicWasmControlsConfig(): Topic<TopicKind.WasmControlsConfig> {
	return { kind: TopicKind.WasmControlsConfig };
}

export function topicVolume(): Topic<TopicKind.Volume> {
	return { kind: TopicKind.Volume };
}

export function topicBass(): Topic<TopicKind.Bass> {
	return { kind: TopicKind.Bass };
}

export function topicBassAvgShort(): Topic<TopicKind.BassAvgShort> {
	return { kind: TopicKind.BassAvgShort };
}

export function topicBassAvg(): Topic<TopicKind.BassAvg> {
	return { kind: TopicKind.BassAvg };
}

export function topicBeatVolume(): Topic<TopicKind.BeatVolume> {
	return { kind: TopicKind.BeatVolume };
}

export function topicLoopSpeed(): Topic<TopicKind.LoopSpeed> {
	return { kind: TopicKind.LoopSpeed };
}

export function topicTickSpeed(): Topic<TopicKind.TickSpeed> {
	return { kind: TopicKind.TickSpeed };
}

export function topicControl(): Topic<TopicKind.Control> {
	return { kind: TopicKind.Control };
}

export type UpdateMessage<T> = T extends TopicKind.DMX
	? { kind: Topic<T>; value: number[] }
	: T extends TopicKind.Heartbeat
		? { kind: Topic<T>; value: number }
		: T extends TopicKind.AudioDevicesView
			? { kind: Topic<T>; value: string[] }
			: T extends TopicKind.AudioDeviceSelected
				? { kind: Topic<T>; value: string }
				: T extends TopicKind.Log
					? { kind: Topic<T>; value: string }
					: T extends TopicKind.WasmLog
						? {
								kind: Topic<T>;
								value: {
									msg: string;
									plugin_id: number;
								};
							}
						: T extends TopicKind.WasmControlsLog
							? { kind: Topic<T>; value: { x: number; y: number; value: string } }
							: T extends TopicKind.WasmControlsSet
								? { kind: Topic<T>; value: { x: number; y: number; value: boolean } }
								: T extends TopicKind.WasmControlsConfig
									? { kind: Topic<T>; value: { x: number; y: number } }
									: T extends TopicKind.Volume
										? { kind: Topic<T>; value: number }
										: T extends TopicKind.Bass
											? { kind: Topic<T>; value: number }
											: T extends TopicKind.BassAvgShort
												? { kind: Topic<T>; value: number }
												: T extends TopicKind.BassAvg
													? { kind: Topic<T>; value: number }
													: T extends TopicKind.BeatVolume
														? { kind: Topic<T>; value: number }
														: T extends TopicKind.BPM
															? { kind: Topic<T>; value: number }
															: T extends TopicKind.LoopSpeed
																? { kind: Topic<T>; value: number }
																: T extends TopicKind.TickSpeed
																	? { kind: Topic<T>; value: number }
																	: T extends TopicKind.Control
																		? { kind: Topic<T>; value: unknown } // TODO: proper typing here!
																		: never;

type OnMessageCallBack<T extends TopicKind> = (data: UpdateMessage<T>) => void;

//
// Websocket.
//

export type SocketCallback = () => void;

export const sleep = (ms: number) => new Promise((res) => setTimeout(res, ms));

export class BlaulichtWebsocketCallbacks {
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	callbacks: Map<string, OnMessageCallBack<any>>;

	constructor() {
		this.callbacks = new Map();
	}

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	trigger(topic: string, data: any) {
		// console.dir(data)
		const callback = this.callbacks.get(topic);
		if (!callback) {
			throw `Required callback does not exist for topic ${data}`;
		}

		callback(data);
	}

	subscribe<K extends TopicKind>(topic: Topic<K>, callback: OnMessageCallBack<K>) {
		const topicStr = JSON.stringify(topic);
		this.callbacks.set(topicStr, callback);
		console.log(`after sub (topic=${topicStr}, callback=${callback})`);
		for (const key of this.callbacks.keys()) {
			console.log(`key=${key}`);
		}
		// this.sync();
	}

	unsubscribeAll() {
		this.callbacks.clear();
		// this.sync();
	}
}

export class BlaulichtWebsocket {
	socket: WebSocket;
	isReady: boolean = false;
	callbacks: BlaulichtWebsocketCallbacks;

	constructor(callbacksP: BlaulichtWebsocketCallbacks) {
		this.callbacks = callbacksP;
		let protocol = undefined;
		const host = document.location.host;

		switch (document.location.protocol) {
			case 'http:':
				protocol = 'ws:';
				break;
			case 'https:':
				protocol = 'wss:';
				break;
			default:
				throw `Unsupported protocol '${document.location.protocol}':
                        only http and https are supported`;
		}

		const url = `${protocol}//${host}/${WS_PATH}`;

		this.socket = new WebSocket(url);

		this.socket.onopen = () => {
			this.isReady = true;
			this.sync();
		};

		this.socket.onclose = () => {
			throw 'Websocket closed prematurely';
		};

		this.socket.onmessage = (evt) => {
			const payload = JSON.parse(evt.data) as UpdateMessage<TopicKind>;

			// if (!payload.topic.additional) {
			//   delete payload.topic.additional;
			// }

			this.onMessage(payload);
		};
	}

	private onMessage(data: UpdateMessage<TopicKind>) {
		const topicStr = JSON.stringify({ kind: data.kind });
		this.callbacks.trigger(topicStr, data);
	}

	private sync() {
		console.log('WS: SYNC');
		return;

		if (!this.isReady) {
			return;
		}

		// let topics: string[] = Array.from(this.callbacks.keys());

		// let topicUn = topics.map((u) => JSON.parse(u));
		//
		// this.socket.send(
		//   JSON.stringify({
		//     topics: topicUn,
		//   })
		// );
	}

	send(event: SendEvent) {
		this.socket.send(JSON.stringify(event));
	}
}
