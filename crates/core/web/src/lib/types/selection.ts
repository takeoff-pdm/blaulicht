//
// Selection
//

export interface GroupSelection {
	onlyFixtures: Map<number, boolean>;
	entireGroup: boolean;
}

export interface ChangeEvent {
	key: string;
	newValue: unknown;
	groupID: number;
	fixID: number;
}

//
// Events
//

export interface SelectionEvent {
	group?: number;
	fixture?: number;
	selected: boolean;
}
