#![macro_use]

use crate::state::ParsedDemo;
use tf_demo_parser::demo::header::Header;
use tf_demo_parser::demo::parser::analyser::UserInfo;
use tf_demo_parser::demo::parser::gamestateanalyser::{GameStateAnalyser, World};
use tf_demo_parser::demo::vector::Vector;
use tf_demo_parser::{Demo, DemoParser, ParseError};
use wasm_bindgen::prelude::*;

mod state;

#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct XY {
    pub x: f32,
    pub y: f32,
}

impl From<Vector> for XY {
    fn from(vec: Vector) -> Self {
        XY { x: vec.x, y: vec.y }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct WorldBoundaries {
    pub boundary_min: XY,
    pub boundary_max: XY,
}

impl From<World> for WorldBoundaries {
    fn from(world: World) -> Self {
        WorldBoundaries {
            boundary_min: world.boundary_min.into(),
            boundary_max: world.boundary_max.into(),
        }
    }
}

#[wasm_bindgen]
pub struct FlatState {
    pub player_count: usize,
    pub building_count: usize,
    pub boundaries: WorldBoundaries,
    pub interval_per_tick: f32,
    kill_ticks: Box<[u32]>,
    attackers: Box<[u8]>,
    assisters: Box<[u8]>,
    victims: Box<[u8]>,
    weapons: Vec<String>,
    player_info: Vec<UserInfo>,
    data: Box<[u8]>,
    header: Header,
}

impl FlatState {
    pub fn new(parsed: ParsedDemo, world: World) -> Self {
        let ParsedDemo {
            players,
            header,
            buildings,
            ..
        } = parsed;

        let player_count = players.len();
        let building_count = buildings.len();

        let flat: Vec<_> = players
            .into_iter()
            .chain(buildings.into_iter())
            .flat_map(|player| player.into_iter())
            .collect();

        FlatState {
            player_count,
            building_count,
            boundaries: world.into(),
            interval_per_tick: header.duration / (header.ticks as f32),
            data: flat.into_boxed_slice(),
            kill_ticks: parsed.kills.iter().map(|kill| kill.tick as u32).collect(),
            attackers: parsed
                .kills
                .iter()
                .map(|kill| kill.attacker_id as u8)
                .collect(),
            assisters: parsed
                .kills
                .iter()
                .map(|kill| kill.assister_id as u8)
                .collect(),
            victims: parsed
                .kills
                .iter()
                .map(|kill| kill.victim_id as u8)
                .collect(),
            weapons: parsed.kills.into_iter().map(|kill| kill.weapon).collect(),
            player_info: parsed.player_info,
            header,
        }
    }
}

#[wasm_bindgen]
pub fn parse_demo(buffer: Box<[u8]>) -> Result<FlatState, JsValue> {
    let (parsed, world) = parse_demo_inner(&buffer).map_err(|e| JsValue::from(e.to_string()))?;

    let world = world.ok_or_else(|| JsValue::from_str("No world defined in demo"))?;

    Ok(FlatState::new(parsed, world))
}

#[wasm_bindgen]
pub fn get_data(state: FlatState) -> Box<[u8]> {
    state.data
}

#[wasm_bindgen]
pub fn get_map(state: &FlatState) -> String {
    state.header.map.clone()
}

#[wasm_bindgen]
pub fn get_kill_ticks(state: &FlatState) -> Box<[u32]> {
    state.kill_ticks.clone()
}

#[wasm_bindgen]
pub fn get_attacker_ids(state: &FlatState) -> Box<[u8]> {
    state.attackers.clone()
}

#[wasm_bindgen]
pub fn get_assister_ids(state: &FlatState) -> Box<[u8]> {
    state.assisters.clone()
}

#[wasm_bindgen]
pub fn get_victim_ids(state: &FlatState) -> Box<[u8]> {
    state.victims.clone()
}

#[wasm_bindgen]
pub fn get_weapon(state: &FlatState, kill_id: usize) -> String {
    state.weapons[kill_id].clone()
}

#[wasm_bindgen]
pub fn get_player_name(state: &FlatState, player_id: usize) -> String {
    state.player_info[player_id].name.clone()
}

#[wasm_bindgen]
pub fn get_player_entity_id(state: &FlatState, player_id: usize) -> u32 {
    state.player_info[player_id].entity_id.into()
}

#[wasm_bindgen]
pub fn get_player_steam_id(state: &FlatState, player_id: usize) -> String {
    state.player_info[player_id].steam_id.clone()
}

pub fn parse_demo_inner(buffer: &[u8]) -> Result<(ParsedDemo, Option<World>), ParseError> {
    let demo = Demo::new(buffer);

    let parser = DemoParser::new_with_analyser(demo.get_stream(), GameStateAnalyser::default());
    let (header, mut ticker) = parser.ticker()?;

    let mut parsed_demo = ParsedDemo::new(header);

    let mut skip = false;
    while ticker.tick()? {
        if !skip {
            // halve framerate
            parsed_demo.push_state(ticker.state());
        }
        skip = !skip;
    }

    let state = ticker.into_state();

    parsed_demo.kills = state.kills;
    Ok((parsed_demo, state.world))
}

// This is like the `main` function, except for JavaScript.
#[wasm_bindgen(start)]
pub fn main_js() -> Result<(), JsValue> {
    Ok(())
}
