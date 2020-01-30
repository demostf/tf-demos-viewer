import {FlatState, XY} from '../pkg/index.d.ts';

export async function parseDemo(bytes: Uint8Array): Promise<ParsedDemo> {
    let m = await import("../pkg/index.js");
    const state = m.parse_demo(bytes);

    let playerCount = m.get_player_count(state);
    let boundaries = m.get_boundaries(state);
    let data = m.get_data(state);

    return new ParsedDemo(playerCount, {
        boundary_min: {
            x: boundaries.boundary_min.x,
            y: boundaries.boundary_min.y,
        },
        boundary_max: {
            x: boundaries.boundary_max.x,
            y: boundaries.boundary_max.y,
        }
    }, data);
}

export enum Team {
    Other = 0,
    Spectator = 1,
    Red = 2,
    Blue = 3,
}

export enum Class {
    Other = 0,
    Scout = 1,
    Sniper = 2,
    Solder = 3,
    Demoman = 4,
    Medic = 5,
    Heavy = 6,
    Pyro = 7,
    Spy = 8,
    Engineer = 9,
}

export interface WorldBoundaries {
    boundary_min: {
        x: number,
        y: number
    },
    boundary_max: {
        x: number,
        y: number
    }
}

export interface PlayerState {
    position: {
        x: number,
        y: number
    },
    angle: number,
    health: number,
    team: Team,
    playerClass: Class,
}

function unpack_f32(val: number, min: number, max: number): number {
    const ratio = val / (Math.pow(2, 16) - 1);
    return ratio * (max - min) + min;
}

function unpack_angle(val: number): number {
    const ratio = val / (Math.pow(2, 8) - 1);
    return ratio * 360;
}

export class ParsedDemo {
    private playerCount: number;
    private world: WorldBoundaries;
    private data: Uint8Array;
    private tickCount: number;

    constructor(playerCount: number, world: WorldBoundaries, data: Uint8Array) {
        this.playerCount = playerCount;
        this.world = world;
        this.data = data;
        this.tickCount = data.length / playerCount / PACK_SIZE;
    }

    getPlayer(tick: number, playerIndex: number): PlayerState {
        if (playerIndex >= this.playerCount) {
            throw new Error("Player out of bounds");
        }

        const base = ((playerIndex * this.tickCount) + tick) * PACK_SIZE;
        return unpackPlayer(this.data, base, this.world);
    }
}

const PACK_SIZE = 8;

function unpackPlayer(bytes: Uint8Array, base: number, world: WorldBoundaries): PlayerState {
    const x = unpack_f32(bytes[base] + (bytes[base + 1] << 8), world.boundary_min.x, world.boundary_max.x);
    const y = unpack_f32(bytes[base + 2] + (bytes[base + 3] << 8), world.boundary_min.y, world.boundary_max.y);
    let health = bytes[base + 4] + (bytes[base + 5] << 8);
    const angle = unpack_angle(bytes[base + 6]);
    const team = (bytes[base + 7] >> 4) as Team;
    const playerClass = (bytes[base + 7] & 15) as Class;

    return {
        position: {x, y},
        angle,
        health,
        team,
        playerClass
    }
}