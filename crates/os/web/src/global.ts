import type { ConfigAction } from '@smui/snackbar/kitchen'
import type { Writable } from 'svelte/store'
import { get, writable } from 'svelte/store'

export interface ModifyUserData {
    displayName: string
    dailyKiloCalorieTarget: number
    dailyProteinTargetGrams: number
}

export interface UserData {
    username: string
    displayName: string
    // Daily intake configuration.
    dailyKiloCalorieTarget: number
    dailyProteinTargetGrams: number
    dailyCarbsTargetGrams: number
    dailyFatsTargetGrams: number
    dailyWaterTargetMillis: number,
    isAdmin: boolean
}

export const user: Writable<UserData> = writable({
    username: '',
    displayName: '',
    dailyProteinTargetGrams: 0,
    dailyKiloCalorieTarget: 0,
    dailyCarbsTargetGrams: 0,
    dailyFatsTargetGrams: 0,
    dailyWaterTargetMillis: 0,
    isAdmin: false,
})

export async function fetchUserData() {
    try {
        user.set(await (await fetch('/api/user/self/data')).json())
    } catch (err) {
        get(createSnackbar)(`Could not fetch user information: ${err}`)
    }
}

export async function modifyUserData(data: UserData) {
    let body = JSON.stringify({
        displayName: data.displayName,
        dailyKiloCalorieTarget: data.dailyKiloCalorieTarget,
        dailyProteinTargetGrams: data.dailyProteinTargetGrams,
        dailyCarbsTargetGrams: data.dailyCarbsTargetGrams,
        dailyFatsTargetGrams: data.dailyFatsTargetGrams,
        dailyWaterTargetMillis: data.dailyWaterTargetMillis,
    })

    console.log(body)

    try {
        let res = await fetch('/api/user/self/data', {
            method: 'PATCH',
            headers: {
                'Content-Type': 'application/json',
            },
            body,
        })

        if (res.status !== 200) {
            let err = await res.json()

            throw (`${err.message}: ${err.error}`)
        }
    } catch (err) {
        get(createSnackbar)(`Could not modify user information: ${err}`)
    }
}

// eslint-disable-next-line @typescript-eslint/no-empty-function
export const createSnackbar: Writable<(message: string, actions?: ConfigAction[]) => void> = writable(() => {})

export const loading: Writable<boolean> = writable(false)

// Given an arbitrary input color, the function decides whether text on the color should be white or black
export function contrast(color: string): 'black' | 'white' {
    const r = parseInt(color.slice(1, 3), 16)
    const g = parseInt(color.slice(3, 5), 16)
    const b = parseInt(color.slice(5, 7), 16)
    const a = [r, g, b].map(v => {
        v /= 255
        return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
    })
    const luminance = a[0] * 0.2126 + a[1] * 0.7152 + a[2] * 0.0722
    const [darker, brighter] = [1.05, luminance + 0.05].sort()
    return brighter / darker <= 4.5 ? 'black' : 'white'
}
