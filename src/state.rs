use serde::{Deserialize, Serialize};
use tf_demo_parser::demo::data::game_state::{Projectile, ProjectileType};
use tf_demo_parser::demo::data::DemoTick;
use tf_demo_parser::demo::gamevent::GameEvent;
use tf_demo_parser::demo::header::Header;
use tf_demo_parser::demo::parser::analyser::UserInfo;
use tf_demo_parser::demo::parser::gamestateanalyser::{
    Building, Class, Dispenser, GameState, Kill, PlayerState as PlayerAliveState, Sentry, Team,
    Teleporter, UserId, World,
};
use tf_demo_parser::demo::vector::VectorXY;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Angle(u8);

impl From<f32> for Angle {
    fn from(val: f32) -> Self {
        let ratio = val.rem_euclid(360.0) / 360.0;
        Angle((ratio * u8::MAX as f32) as u8)
    }
}

impl From<Angle> for f32 {
    fn from(val: Angle) -> Self {
        let ratio = val.0 as f32 / u8::MAX as f32;
        ratio * 360.0
    }
}

#[derive(Debug)]
pub struct ParsedDemo {
    last_tick: DemoTick,
    pub tick: usize,
    pub players: Vec<Vec<u8>>,
    pub buildings: Vec<Vec<u8>>,
    pub projectiles: Vec<Vec<u8>>,
    pub kills: Vec<Kill>,
    pub events: Vec<SearchableEvent>,
    pub header: Header,
    pub player_info: Vec<UserInfo>,
    pub max_building_count: usize,
    pub max_projectile_count: usize,
}

impl ParsedDemo {
    pub fn new(header: Header) -> Self {
        ParsedDemo {
            last_tick: DemoTick::default(),
            tick: 0,
            players: Vec::new(),
            buildings: Vec::new(),
            projectiles: Vec::new(),
            kills: Vec::new(),
            player_info: Vec::new(),
            max_building_count: 0,
            max_projectile_count: 0,
            events: Vec::new(),
            header,
        }
    }

    pub fn push_state(&mut self, game_state: &GameState) {
        if let Some(world) = game_state.world.as_ref() {
            for _tick in u32::from(self.last_tick)..u32::from(game_state.tick) {
                for (index, player) in game_state.players.iter().enumerate() {
                    let state = PlayerState {
                        position: player.position.into(),
                        angle: Angle::from(player.view_angle),
                        health: if player.state == PlayerAliveState::Alive {
                            player.health
                        } else {
                            0
                        },
                        team: player.team,
                        class: player.class,
                        charge: player.charge,
                    };

                    if self.players.get(index).is_none() {
                        let mut new_player = Vec::with_capacity(
                            self.header.ticks as usize * PlayerState::PACKET_SIZE,
                        );
                        // backfill with defaults
                        new_player.resize(self.tick * PlayerState::PACKET_SIZE, 0);
                        self.players.push(new_player);
                    };

                    if let (None, Some(info)) = (self.player_info.get(index), player.info.as_ref())
                    {
                        self.player_info.push(info.clone());
                    }

                    let parsed_player = &mut self.players[index];
                    parsed_player.extend_from_slice(&state.pack(world));
                }

                self.max_building_count = self.max_building_count.max(game_state.buildings.len());
                for (index, building) in game_state.buildings.values().enumerate() {
                    let state = BuildingState::new(building);

                    if self.buildings.get(index).is_none() {
                        let new_building = Vec::with_capacity(
                            self.header.ticks as usize * BuildingState::PACKET_SIZE,
                        );
                        self.buildings.push(new_building);
                    };

                    let parsed_building = &mut self.buildings[index];
                    parsed_building.resize(self.tick * BuildingState::PACKET_SIZE, 0);

                    parsed_building.extend_from_slice(&state.pack(world));
                }

                self.max_projectile_count =
                    self.max_projectile_count.max(game_state.projectiles.len());
                for (index, projectile) in game_state.projectiles.values().enumerate() {
                    let state = ProjectileState::new(projectile);

                    if self.projectiles.get(index).is_none() {
                        let new_projectile = Vec::with_capacity(
                            self.header.ticks as usize * ProjectileState::PACKET_SIZE,
                        );
                        self.projectiles.push(new_projectile);
                    };

                    let parsed_projectiles = &mut self.projectiles[index];
                    parsed_projectiles.resize(self.tick * ProjectileState::PACKET_SIZE, 0);

                    parsed_projectiles.extend_from_slice(&state.pack(world));
                }
                self.tick += 1;
            }
            self.last_tick = game_state.tick;
        }
    }

    pub fn finish(&mut self, state: &GameState) {
        for parsed_building in self.buildings.iter_mut() {
            parsed_building.resize(self.tick * BuildingState::PACKET_SIZE, 0);
        }
        for parsed_projectiles in self.projectiles.iter_mut() {
            parsed_projectiles.resize(self.tick * ProjectileState::PACKET_SIZE, 0);
        }

        self.events = state
            .events
            .iter()
            .flat_map(|(tick, event)| SearchableEvent::from_event(*tick, event))
            .collect();
    }

    pub fn size(&self) -> usize {
        self.players
            .iter()
            .fold(0, |size, player| size + player.len())
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PlayerState {
    position: VectorXY,
    angle: Angle,
    health: u16,
    team: Team,
    class: Class,
    charge: u8,
}

impl PlayerState {
    const PACKET_SIZE: usize = 8;

    pub fn pack(&self, world: &World) -> [u8; Self::PACKET_SIZE] {
        // for the purpose of viewing the demo in the browser we dont really need high accuracy for
        // position or angle, so we save a bunch of space by truncating those down to half the number
        // of bits
        fn pack_f32(val: f32, min: f32, max: f32) -> u16 {
            let ratio = (val - min) / (max - min);
            (ratio * u16::MAX as f32) as u16
        }

        let x = pack_f32(self.position.x, world.boundary_min.x, world.boundary_max.x).to_le_bytes();
        let y = pack_f32(self.position.y, world.boundary_min.y, world.boundary_max.y).to_le_bytes();
        // 2 bits for team
        // 4 bits for class
        // 10 bits for health
        let team_class_health =
            ((self.team as u16) << 14) + ((self.class as u16) << 10) + self.health;
        let combined_bytes = team_class_health.to_le_bytes();

        [
            x[0],
            x[1],
            y[0],
            y[1],
            combined_bytes[0],
            combined_bytes[1],
            self.angle.0,
            self.charge,
        ]
    }

    #[allow(dead_code)]
    pub fn unpack(bytes: [u8; 8], world: &World) -> Self {
        fn unpack_f32(val: u16, min: f32, max: f32) -> f32 {
            let ratio = val as f32 / (u16::MAX as f32);
            ratio * (max - min) + min
        }

        let x = unpack_f32(
            u16::from_le_bytes([bytes[0], bytes[1]]),
            world.boundary_min.x,
            world.boundary_max.x,
        );
        let y = unpack_f32(
            u16::from_le_bytes([bytes[2], bytes[3]]),
            world.boundary_min.y,
            world.boundary_max.y,
        );
        let team_class_health = u16::from_le_bytes([bytes[4], bytes[5]]);
        let health = team_class_health & 1023;
        let angle = Angle(bytes[6]);
        let team = Team::new(team_class_health >> 14);
        let class = Class::new((team_class_health >> 10) & 15);
        let charge = bytes[7];

        PlayerState {
            position: VectorXY { x, y },
            angle,
            health,
            team,
            class,
            charge,
        }
    }
}

#[test]
fn test_player_packing() {
    use tf_demo_parser::demo::vector::Vector;

    let world = World {
        boundary_max: Vector {
            x: 10000.0,
            y: 10000.0,
            z: 100.0,
        },
        boundary_min: Vector {
            x: -10000.0,
            y: -10000.0,
            z: -100.0,
        },
    };

    let input = PlayerState {
        position: VectorXY {
            x: 100.0,
            y: -5000.0,
        },
        angle: Angle::from(213.0),
        health: 250,
        team: Team::Blue,
        class: Class::Demoman,
        charge: 7,
    };

    let bytes = input.pack(&world);

    let unpacked = PlayerState::unpack(bytes, &world);
    assert_eq!(input.angle, unpacked.angle);
    assert_eq!(input.health, unpacked.health);
    assert_eq!(input.class, unpacked.class);
    assert_eq!(input.team, unpacked.team);
    assert_eq!(input.charge, unpacked.charge);

    assert!(f32::abs(input.position.x - unpacked.position.x) < 0.5);
    assert!(f32::abs(input.position.y - unpacked.position.y) < 0.5);
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(u8)]
pub enum BuildingType {
    TeleporterEntrance = 0,
    TeleporterExit = 1,
    Dispenser = 2,
    Level1Sentry = 3,
    Level2Sentry = 4,
    Level3Sentry = 5,
    MiniSentry = 6,
    #[default]
    Unknown = 7,
}

impl BuildingType {
    pub fn new(raw: u8) -> BuildingType {
        match raw {
            0 => Self::TeleporterEntrance,
            1 => Self::TeleporterExit,
            2 => Self::Dispenser,
            3 => Self::Level1Sentry,
            4 => Self::Level2Sentry,
            5 => Self::Level3Sentry,
            6 => Self::MiniSentry,
            _ => Self::Unknown,
        }
    }

    pub fn from_building(building: &Building) -> Self {
        match building {
            Building::Sentry(Sentry { is_mini: true, .. }) => BuildingType::MiniSentry,
            Building::Sentry(Sentry {
                is_mini: false,
                level: 1,
                ..
            }) => BuildingType::Level1Sentry,
            Building::Sentry(Sentry {
                is_mini: false,
                level: 2,
                ..
            }) => BuildingType::Level2Sentry,
            Building::Sentry(Sentry {
                is_mini: false,
                level: 3,
                ..
            }) => BuildingType::Level3Sentry,
            Building::Dispenser(Dispenser { .. }) => BuildingType::Dispenser,
            Building::Teleporter(Teleporter {
                is_entrance: true, ..
            }) => BuildingType::TeleporterEntrance,
            Building::Teleporter(Teleporter {
                is_entrance: false, ..
            }) => BuildingType::TeleporterExit,
            _ => BuildingType::Unknown,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct BuildingState {
    position: VectorXY,
    angle: Angle,
    health: u16,
    team: Team,
    ty: BuildingType,
    level: u8,
}

// for the purpose of viewing the demo in the browser we dont really need high accuracy for
// position or angle, so we save a bunch of space by truncating those down to half the number
// of bits
fn pack_f32(val: f32, min: f32, max: f32) -> u16 {
    let ratio = (val - min) / (max - min);
    (ratio * u16::MAX as f32) as u16
}
fn unpack_f32(val: u16, min: f32, max: f32) -> f32 {
    let ratio = val as f32 / (u16::MAX as f32);
    ratio * (max - min) + min
}

impl BuildingState {
    const PACKET_SIZE: usize = 7;

    pub fn new(building: &Building) -> Self {
        let position = building.position();
        BuildingState {
            position: VectorXY {
                x: position.x,
                y: position.y,
            },
            angle: Angle::from(building.angle()),
            health: building.health(),
            team: building.team(),
            ty: BuildingType::from_building(building),
            level: building.level(),
        }
    }

    pub fn pack(&self, world: &World) -> [u8; Self::PACKET_SIZE] {
        let x = pack_f32(self.position.x, world.boundary_min.x, world.boundary_max.x).to_le_bytes();
        let y = pack_f32(self.position.y, world.boundary_min.y, world.boundary_max.y).to_le_bytes();
        // 2 bits level
        // 1 bit team
        // 3 bits for type
        // 10 bits for health
        let team = if self.team == Team::Blue { 0 } else { 1 };
        let team_type_health = ((self.level as u16) << 14)
            + ((team as u16) << 13)
            + ((self.ty as u16) << 10)
            + self.health;
        let combined_bytes = team_type_health.to_le_bytes();

        [
            x[0],
            x[1],
            y[0],
            y[1],
            combined_bytes[0],
            combined_bytes[1],
            self.angle.0,
        ]
    }

    #[allow(dead_code)]
    pub fn unpack(bytes: [u8; Self::PACKET_SIZE], world: &World) -> Self {
        let x = unpack_f32(
            u16::from_le_bytes([bytes[0], bytes[1]]),
            world.boundary_min.x,
            world.boundary_max.x,
        );
        let y = unpack_f32(
            u16::from_le_bytes([bytes[2], bytes[3]]),
            world.boundary_min.y,
            world.boundary_max.y,
        );
        let team_type_health = u16::from_le_bytes([bytes[4], bytes[5]]);
        let health = team_type_health & 1023;
        let angle = Angle(bytes[6]);
        let packed_team = (team_type_health >> 13) & 1;
        let team = if packed_team == 0 {
            Team::Blue
        } else {
            Team::Red
        };
        let ty = BuildingType::new((team_type_health >> 10) as u8 & 7);
        let level = (team_type_health >> 14) as u8;

        BuildingState {
            position: VectorXY { x, y },
            angle,
            health,
            team,
            ty,
            level,
        }
    }
}

#[test]
fn test_building_packing() {
    use tf_demo_parser::demo::vector::Vector;

    let world = World {
        boundary_max: Vector {
            x: 10000.0,
            y: 10000.0,
            z: 100.0,
        },
        boundary_min: Vector {
            x: -10000.0,
            y: -10000.0,
            z: -100.0,
        },
    };

    let input = BuildingState {
        position: VectorXY {
            x: 100.0,
            y: -5000.0,
        },
        angle: Angle::from(213.0),
        health: 250,
        team: Team::Blue,
        level: 3,
        ty: BuildingType::Level1Sentry,
    };

    let bytes = input.pack(&world);

    let unpacked = BuildingState::unpack(bytes, &world);
    assert_eq!(input.angle, unpacked.angle);
    assert_eq!(input.health, unpacked.health);
    assert_eq!(input.ty, unpacked.ty);
    assert_eq!(input.team, unpacked.team);
    assert_eq!(input.level, unpacked.level);

    assert!(f32::abs(input.position.x - unpacked.position.x) < 0.5);
    assert!(f32::abs(input.position.y - unpacked.position.y) < 0.5);
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ProjectileState {
    position: VectorXY,
    team: Team,
    ty: ProjectileType,
    angle: Angle,
}

impl ProjectileState {
    const PACKET_SIZE: usize = 6;

    pub fn new(projectile: &Projectile) -> Self {
        let position = projectile.position;
        ProjectileState {
            position: VectorXY {
                x: position.x,
                y: position.y,
            },
            angle: Angle::from(projectile.rotation.y),
            team: projectile.team,
            ty: projectile.ty,
        }
    }

    pub fn pack(&self, world: &World) -> [u8; Self::PACKET_SIZE] {
        let x = pack_f32(self.position.x, world.boundary_min.x, world.boundary_max.x).to_le_bytes();
        let y = pack_f32(self.position.y, world.boundary_min.y, world.boundary_max.y).to_le_bytes();
        // 1 bit team
        // 3 bits for type
        // 4 bits for angle, 16 angles should be enough for projectiles
        let team = if self.team == Team::Blue { 0 } else { 1 };
        let team_type = ((self.ty as u8) << 5) + ((team as u8) << 4);

        [x[0], x[1], y[0], y[1], team_type, self.angle.0]
    }

    #[allow(dead_code)]
    pub fn unpack(bytes: [u8; Self::PACKET_SIZE], world: &World) -> Self {
        let x = unpack_f32(
            u16::from_le_bytes([bytes[0], bytes[1]]),
            world.boundary_min.x,
            world.boundary_max.x,
        );
        let y = unpack_f32(
            u16::from_le_bytes([bytes[2], bytes[3]]),
            world.boundary_min.y,
            world.boundary_max.y,
        );
        let team_type = bytes[4];
        let packed_team = (team_type >> 4) & 1;
        let team = if packed_team == 0 {
            Team::Blue
        } else {
            Team::Red
        };
        let ty = ProjectileType::from((team_type >> 5) & 7);
        let angle = Angle(bytes[5]);

        ProjectileState {
            position: VectorXY { x, y },
            angle,
            team,
            ty,
        }
    }
}

#[test]
fn test_projectile_packing() {
    use tf_demo_parser::demo::vector::Vector;

    let world = World {
        boundary_max: Vector {
            x: 10000.0,
            y: 10000.0,
            z: 100.0,
        },
        boundary_min: Vector {
            x: -10000.0,
            y: -10000.0,
            z: -100.0,
        },
    };

    let input = ProjectileState {
        position: VectorXY {
            x: 100.0,
            y: -5000.0,
        },
        angle: Angle::from(123.0),
        team: Team::Blue,
        ty: ProjectileType::Flare,
    };

    let bytes = input.pack(&world);

    let unpacked = ProjectileState::unpack(bytes, &world);
    assert_eq!(input.ty, unpacked.ty);
    assert_eq!(input.team, unpacked.team);
    assert_eq!(input.angle, unpacked.angle);

    assert!(f32::abs(input.position.x - unpacked.position.x) < 0.5);
    assert!(f32::abs(input.position.y - unpacked.position.y) < 0.5);
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum RawBuildingType {
    Dispenser,
    Teleporter,
    SentryGun,
}

impl TryFrom<u16> for RawBuildingType {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(RawBuildingType::Dispenser),
            1 => Ok(RawBuildingType::Teleporter),
            2 => Ok(RawBuildingType::SentryGun),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum SearchableEvent {
    Uber {
        user_id: UserId,
        target_id: UserId,
        tick: DemoTick,
    },
    BuildingDestroyed {
        attacker_id: UserId,
        assister_id: UserId,
        victim_id: UserId,
        weapon: String,
        building_type: RawBuildingType,
        tick: DemoTick,
    },
}

impl SearchableEvent {
    pub fn from_event(tick: DemoTick, event: &GameEvent) -> Option<SearchableEvent> {
        match event {
            GameEvent::ObjectDestroyed(event) => {
                let building_type = RawBuildingType::try_from(event.object_type).ok()?;
                Some(SearchableEvent::BuildingDestroyed {
                    attacker_id: UserId::from(event.attacker),
                    assister_id: UserId::from(event.assister),
                    victim_id: UserId::from(event.user_id),
                    weapon: event.weapon.to_string(),
                    building_type,
                    tick,
                })
            }
            GameEvent::PlayerChargeDeployed(event) => Some(SearchableEvent::Uber {
                user_id: UserId::from(event.user_id),
                target_id: UserId::from(event.target_id),
                tick,
            }),
            _ => None,
        }
    }
}
