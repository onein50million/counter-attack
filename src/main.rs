mod ui;

use std::{
    cmp::Ordering,
    collections::{hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    net::SocketAddr,
    ops::{Add, AddAssign, Neg, Sub, SubAssign},
    thread::sleep,
    time::{Duration, Instant}, iter::repeat,
};

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    reflect::TypeUuid,
    render::render_resource::{FilterMode, SamplerDescriptor},
    time::common_conditions::on_timer,
};
use bevy_asset_loader::prelude::{AssetCollection, LoadingState, LoadingStateAppExt};
use bevy_easings::{EaseValue, Lerp};
use bevy_hanabi::prelude::*;
use bevy_sprite3d::{AtlasSprite3d, AtlasSprite3dComponent, Sprite3dParams, Sprite3dPlugin};
use bytemuck::{Pod, Zeroable};
use clap::Parser;
use ggrs::{
    Config, GGRSRequest, InputStatus, P2PSession, PlayerHandle, PlayerType, SessionBuilder,
    UdpNonBlockingSocket,
};
use iunorm::{Inorm64, Unorm64};
use ui::{Roboto, GUI};
//https://freesound.org/people/aarrnnoo/sounds/516189/

const FRAMETIME: f64 = 0.1;
const FPS: f64 = 1.0 / FRAMETIME;
const VOLUME_SCALE: f32 = 0.5;
const BASE_STAMINA_LOSS: f64 = 0.1;
const CLASH_LENGTH: Second = Second(1.0);
const FINAL_CLASH_LIVES: u8 = 4;

const TEST_ATTACK: Attack = Attack {
    startup_time: Second(0.9),
    block_grace: Second(0.3),
    recover_time: Second(0.2),
};

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
struct Second(f64);

impl Hash for Second {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_be_bytes().hash(state);
    }
}
impl Neg for Second {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

#[derive(Parser, Debug)]
struct Args {
    local_port: u16,
    remote_addr: SocketAddr,
}

#[derive(Clone, Debug, Hash)]
struct Attack {
    startup_time: Second,
    block_grace: Second,
    recover_time: Second,
}
#[derive(Resource, Debug)]
struct LastTickTime {
    // frame_offset: FrameOffset,
    frame: usize,
    instant: Instant,
}
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable, Hash, Eq)]
struct FrameOffset {
    frame: usize,
    offset: u64,
}
impl PartialOrd for FrameOffset {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(if self.frame > other.frame {
            Ordering::Greater
        } else if self.frame < other.frame {
            Ordering::Less
        } else {
            if self.offset > other.offset {
                Ordering::Greater
            } else if self.offset > other.offset {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
    }
}
impl Ord for FrameOffset {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl Add<Second> for FrameOffset {
    type Output = FrameOffset;

    fn add(mut self, rhs: Second) -> Self::Output {
        let mut out_offset = Unorm64(self.offset).to_f64() + rhs.0 / FRAMETIME;
        loop {
            if out_offset > 1.0 {
                self.frame += 1;
                out_offset -= 1.0;
            } else if out_offset < 0.0 {
                self.frame -= 1;
                out_offset += 1.0;
            } else {
                break;
            }
        }
        Self {
            frame: self.frame,
            offset: Unorm64::from_f64(out_offset).0,
        }
    }
}

impl Add<FrameOffset> for Second {
    type Output = FrameOffset;

    fn add(self, rhs: FrameOffset) -> Self::Output {
        rhs.add(self)
    }
}
impl Sub<FrameOffset> for Second {
    type Output = FrameOffset;

    fn sub(self, rhs: FrameOffset) -> Self::Output {
        rhs.add(-self)
    }
}

impl Sub<Second> for FrameOffset {
    type Output = Self;

    fn sub(self, rhs: Second) -> Self::Output {
        self.add(-rhs)
    }
}
impl SubAssign<Second> for FrameOffset {
    fn sub_assign(&mut self, rhs: Second) {
        *self = *self - rhs
    }
}
impl AddAssign<Second> for FrameOffset {
    fn add_assign(&mut self, rhs: Second) {
        *self = *self + rhs
    }
}
impl FrameOffset {
    fn now(last_tick_time: &LastTickTime) -> Self {
        FrameOffset {
            frame: last_tick_time.frame,
            offset: 0,
            // offset: Unorm64::from_f64(last_tick_time.instant.elapsed().as_secs_f64()).0,
            // offset: 0,
        } + Second(last_tick_time.instant.elapsed().as_secs_f64().min(1.0))
        // + Second(last_tick_time.instant.elapsed().as_secs_f64())
    }
    fn get_offset_seconds(&self, future: &Self) -> Second {
        let self_frame = (self.frame as f64 + Unorm64(self.offset).to_f64()) * FRAMETIME;
        let future_frame = (future.frame as f64 + Unorm64(future.offset).to_f64()) * FRAMETIME;

        Second(future_frame - self_frame)
    }
    // fn from_instant(instant: Instant, last_tick_time: &LastTickTime) -> Self {
    //     let raw_offset = if last_tick_time.frame_instant > instant {
    //         last_tick_time
    //             .frame_instant
    //             .duration_since(instant)
    //             .as_secs_f64()
    //     } else {
    //         -instant
    //             .duration_since(last_tick_time.frame_instant)
    //             .as_secs_f64()
    //     };
    //     let frametime_seconds = FRAMETIME as f64 / 1000.0;
    //     let frame_offset = raw_offset / frametime_seconds;
    //     let remaining_offset = raw_offset.rem_euclid(frametime_seconds);
    //     let frame = last_tick_time.frame_number + frame_offset as usize;
    //     Self {
    //         frame,
    //         offset: Unorm64::from_f64(remaining_offset).0,
    //     }
    // }
    // fn to_instant(&self, last_tick_time: &LastTickTime) -> Instant {
    //     let offset = Unorm64(self.offset).to_f64();
    //     let frame_delta = last_tick_time.frame_number as f64 - self.frame as f64;
    //     let frame_delta = (frame_delta + offset) * (FRAMETIME as f64 / 1000.0);
    //     if frame_delta > 0.0 {
    //         last_tick_time.frame_instant + Duration::from_secs_f64(frame_delta)
    //     } else {
    //         last_tick_time.frame_instant - Duration::from_secs_f64(frame_delta.abs())
    //     }
    // }
    // fn get_offset_seconds(&self, other: &Self, last_tick_time: &LastTickTime) -> f64 {
    //     let self_instant = self.to_instant(last_tick_time);
    //     let other_instant = other.to_instant(last_tick_time);
    //     if self_instant > other_instant {
    //         self_instant.duration_since(other_instant).as_secs_f64()
    //     } else {
    //         -other_instant.duration_since(self_instant).as_secs_f64()
    //     }
    // }
    fn is_valid(&self) -> bool {
        self.frame != usize::MAX
    }
}

#[derive(Component, Clone, Debug, Hash)]
struct Player {
    current_attack: Option<Attack>,
    attack_start_time: FrameOffset,
    attack_recover_time: FrameOffset,
    last_defend_result: i64,
    stamina: Unorm64,
    final_clash_lives: u8,
    final_clash_last_swing: Option<FrameOffset>,
}
impl Player {
    fn swing(
        &mut self,
        other: &mut Self,
        frame_offset: FrameOffset,
        attack: Attack,
    ) -> Option<f64> {
        self.attack_start_time = frame_offset;
        self.attack_recover_time = frame_offset + attack.startup_time + attack.recover_time;

        self.current_attack = Some(attack);

        let defend_time = frame_offset;
        let impact_time = other.attack_start_time + other.current_attack.as_ref()?.startup_time;

        let defend_time_offset = defend_time.get_offset_seconds(&impact_time);
        other.current_attack = None;
        self.last_defend_result = Inorm64::from_f64(dbg!(defend_time_offset.0)).0;
        Some(defend_time_offset.0)
    }
    fn take_final_clash_life(&mut self) {
        // self.final_clash_last_swing = None;
        if self.final_clash_lives > 0 {
            self.final_clash_lives -= 1;
        }
    }
}

// #[derive(Component, Debug, Clone)]
// struct AnimatedAtlas {
//     atlas: TextureAtlas,
// }

#[derive(Debug, Resource)]
struct LocalInput {
    attacking: Option<FrameOffset>,
    // defending: Option<Instant>,
}
#[derive(Debug, Resource, AssetCollection)]
struct AtlasLoader {
    #[asset(texture_atlas(tile_size_x = 56.0, tile_size_y = 36.0))]
    #[asset(texture_atlas(columns = 11, rows = 1))]
    #[asset(path = "samurai/heavy-attack.png")]
    attack_atlas: Handle<TextureAtlas>,
    #[asset(texture_atlas(tile_size_x = 32.0, tile_size_y = 32.0))]
    #[asset(texture_atlas(columns = 1, rows = 1))]
    #[asset(path = "samurai/block.png")]
    defend_atlas: Handle<TextureAtlas>,
    #[asset(texture_atlas(tile_size_x = 30.0, tile_size_y = 22.0))]
    #[asset(texture_atlas(columns = 3, rows = 1))]
    #[asset(path = "samurai/idle.png")]
    idle_atlas: Handle<TextureAtlas>,
}

#[derive(Resource)]
struct AnimationLibrary {
    attack: Handle<Animation>,
    counter_attack: Handle<Animation>,
    idle: Handle<Animation>,
}

#[derive(Clone, TypeUuid)]
#[uuid = "a8bd7069-58a8-4fa9-bcae-87704fb309c4"]
struct Animation(Vec<Frame>);

#[derive(Component)]
struct Animated {
    previous_frame: usize,
    // animation: Handle<Animation>,
    // current_frame: usize,
    // start: FrameOffset,
    // next: VecDeque<AnimationQueueItem>,
}
// enum AnimationQueueItem{
//     Immediate(Handle<Animation>),
//     UponCompletion(Handle<Animation>),
//     // Idle(Handle<Animation>)
// }

#[derive(Clone)]
struct Frame {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    // atlas_index: usize,
    trigger: Option<AnimationTriggerType>,
}

#[derive(Clone)]
enum AnimationTriggerType {
    Woosh,
    // Loop,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Pod, Zeroable)]
struct SendInput {
    attacking: FrameOffset,
    // defending: FrameOffset,
}

#[derive(Debug)]
struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = SendInput;
    type State = WorldSnapshot;
    type Address = SocketAddr;
}

#[derive(Resource, Clone, Hash)]
struct FinalClash {
    next_clash: Option<FrameOffset>,
}

#[derive(Clone, Hash)]
struct WorldSnapshot {
    players: [Player; 2],
    final_clash: FinalClash,
    game_state: GameState,
}

#[derive(Resource)]
struct Session(P2PSession<GGRSConfig>);

#[derive(Resource, AssetCollection)]
struct SoundLibrary {
    #[asset(path = "block.ogg")]
    block: Handle<AudioSource>,
    #[asset(path = "perfect_block.ogg")]
    perfect_block: Handle<AudioSource>,
    #[asset(path = "flesh_cut.ogg")]
    flesh_cut: Handle<AudioSource>,
    #[asset(path = "swoosh.ogg")]
    swoosh: Handle<AudioSource>,
}

#[derive(Resource)]
struct SparkEffect(Handle<EffectAsset>);

#[derive(Component)]
pub struct LocalMarker;

#[derive(Component)]
pub struct FinalClashLives;
pub struct BlockEvent(String);
pub enum GameEvent {
    GameOver { loser: Option<PlayerHandle> },
}
#[derive(Clone, Hash, Resource, Default, PartialEq, Eq, Debug)]
pub enum GameState {
    #[default]
    Playing,
    FinalClash,
    Over,
}

#[derive(States, Debug, Hash, PartialEq, Eq, Default, Clone, Copy)]
pub enum AssetLoadingState {
    #[default]
    Loading,
    Done,
}

// #[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone, Copy)]
// struct LoadedUpdateSet;

fn main() {
    let args = Args::parse();

    let mut app = App::new();

    let session_builder = SessionBuilder::<GGRSConfig>::new()
        .with_fps(FPS as usize)
        .unwrap()
        .add_player(PlayerType::Local, 0)
        .unwrap()
        .add_player(PlayerType::Remote(args.remote_addr), 1)
        .unwrap();

    let udp_socket = UdpNonBlockingSocket::bind_to_port(args.local_port).unwrap();
    let session = session_builder.start_p2p_session(udp_socket).unwrap();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    present_mode: bevy::window::PresentMode::Immediate,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .set(ImagePlugin {
                default_sampler: SamplerDescriptor {
                    mag_filter: FilterMode::Nearest,
                    ..Default::default()
                },
            }),
    )
    .add_state::<AssetLoadingState>()
    .add_asset::<Animation>()
    .add_plugin(HanabiPlugin)
    .add_plugin(LogDiagnosticsPlugin::default())
    .add_plugin(FrameTimeDiagnosticsPlugin::default())
    .add_plugin(GUI)
    .add_plugin(Sprite3dPlugin)
    .add_event::<BlockEvent>()
    .add_event::<GameEvent>()
    .add_loading_state(
        LoadingState::new(AssetLoadingState::Loading).continue_to_state(AssetLoadingState::Done),
    )
    .add_collection_to_loading_state::<_, AtlasLoader>(AssetLoadingState::Loading)
    .add_collection_to_loading_state::<_, SoundLibrary>(AssetLoadingState::Loading)
    .add_startup_system(setup_particles)
    .add_system((setup_players).in_schedule(OnEnter(AssetLoadingState::Done)))
    .add_systems(
        (
            input,
            rollback_system.run_if(
                on_timer(Duration::from_secs_f64(FRAMETIME))
                    .and_then(not(resource_exists_and_equals(GameState::Over))),
            ),
            network_stats.run_if(on_timer(Duration::from_secs_f64(5.0))),
            poll_clients,
            handle_game_events.run_if(not(resource_exists_and_equals(GameState::Over))),
            update_animated_atlas,
            block_sparks,
            update_animations,
        )
            .distributive_run_if(in_state(AssetLoadingState::Done)),
    )
    .insert_resource(Session(session))
    .insert_resource(LastTickTime {
        frame: 0,
        instant: Instant::now(),
    })
    .insert_resource(FinalClash { next_clash: None })
    .insert_resource(LocalInput { attacking: None })
    .insert_resource(GameState::default())
    .run();
}

// fn update_last_tick_time(mut last_tick_time: ResMut<LastTickTime>, time: Res<Time>){
//     last_tick_time.frame_offset += Second(time.delta_seconds_f64());
// }

fn network_stats(session: Res<Session>) {
    // dbg!(session.0.network_stats(0));
    println!("{:?}", session.0.network_stats(1));
}

// fn setup_audio(mut commands: Commands, asset_server: Res<AssetServer>) {
//     commands.insert_resource(SoundLibrary {
//         block: asset_server.load("block.ogg"),
//         perfect_block: asset_server.load("perfect_block.ogg"),
//         flesh_cut: asset_server.load("flesh_cut.ogg"),
//         flesh_cut: asset_server.load("flesh_cut.ogg"),
//     });
// }

fn setup_players(
    mut commands: Commands,
    last_tick_time: Res<LastTickTime>,
    mut sprite_params: Sprite3dParams,
    atlas: Res<AtlasLoader>,
    mut animation_asset: ResMut<Assets<Animation>>,
    // mut fighter_sprites: ResMut<FighterSprites>,
    // texture_atlas_assets: ResMut<Assets<TextureAtlas>>
) {
    // images.get(&texture_atlas.get_mut(&atlas.attack_atlas).unwrap().texture).unwrap().sampler_descriptor
    let attack_bundle = AtlasSprite3d {
        atlas: atlas.attack_atlas.clone(),
        unlit: true,
        pixels_per_metre: 8.0,
        ..Default::default()
    }
    .bundle(&mut sprite_params);
    let defend_bundle = AtlasSprite3d {
        atlas: atlas.defend_atlas.clone(),
        unlit: true,
        pixels_per_metre: 8.0,
        ..Default::default()
    }
    .bundle(&mut sprite_params);
    let idle_bundle = AtlasSprite3d {
        atlas: atlas.idle_atlas.clone(),
        unlit: true,
        pixels_per_metre: 8.0,
        ..Default::default()
    }
    .bundle(&mut sprite_params);

    let counter_attack = animation_asset.add({
        let mut animation = vec![];
        animation.extend(repeat(Frame {
            mesh: sprite_params
                .sr
                .mesh_cache
                .get(&defend_bundle.params.atlas[0])
                .unwrap()
                .clone(),
            material: defend_bundle.pbr.material.clone(),
            trigger: None,
        }).take(3));

        for i in 0..attack_bundle.params.atlas.len() {
            animation.push(Frame {
                mesh: sprite_params
                    .sr
                    .mesh_cache
                    .get(&attack_bundle.params.atlas[i])
                    .unwrap()
                    .clone(),
                material: attack_bundle.pbr.material.clone(),
                trigger: None,
            })
        }
        animation[6].trigger = Some(AnimationTriggerType::Woosh);
        Animation(animation)
    });
    let attack = animation_asset.add({
        let mut animation = vec![];
        for i in 0..attack_bundle.params.atlas.len() {
            animation.push(Frame {
                mesh: sprite_params
                    .sr
                    .mesh_cache
                    .get(&attack_bundle.params.atlas[i])
                    .unwrap()
                    .clone(),
                material: attack_bundle.pbr.material.clone(),
                trigger: None,
            })
        }
        animation[5].trigger = Some(AnimationTriggerType::Woosh);
        Animation(animation)
    });
    let idle = animation_asset.add({
        let mut animation = vec![];
        for i in 0..idle_bundle.params.atlas.len() {
            animation.push(Frame {
                mesh: sprite_params
                    .sr
                    .mesh_cache
                    .get(&idle_bundle.params.atlas[i])
                    .unwrap()
                    .clone(),
                material: idle_bundle.pbr.material.clone(),
                trigger: None,
            })
        }
        // animation.last_mut().unwrap().trigger = Some(AnimationTriggerType::Loop);
        Animation(animation)
    });

    commands.insert_resource(AnimationLibrary {
        attack: attack.clone(),
        counter_attack: counter_attack.clone(),
        idle: idle.clone(),
    });

    commands
        .spawn(Player {
            current_attack: None,
            attack_start_time: FrameOffset::now(&last_tick_time),
            attack_recover_time: FrameOffset::now(&last_tick_time),
            last_defend_result: 0,
            stamina: Unorm64(u64::MAX),
            final_clash_last_swing: None,
            final_clash_lives: FINAL_CLASH_LIVES,
        })
        .insert(LocalMarker)
        .insert(PbrBundle {
            transform: Transform::from_translation(Vec3::new(-2.0, 0.0, 0.0)),
            ..Default::default()
        })
        .insert(Animated {
            // animation: idle.clone(),
            // current_frame: 0,
            // start: FrameOffset::now(&last_tick_time),
            // next: VecDeque::new(),
            previous_frame: usize::MAX
        });

    commands
        .spawn(Player {
            current_attack: None,
            attack_start_time: FrameOffset::now(&last_tick_time),
            attack_recover_time: FrameOffset::now(&last_tick_time),
            last_defend_result: 0,
            stamina: Unorm64(u64::MAX),
            final_clash_last_swing: None,
            final_clash_lives: FINAL_CLASH_LIVES,
        })
        .insert(PbrBundle {
            transform: Transform::from_scale(Vec3::new(-1.0, 1.0, 1.0))
                .with_translation(Vec3::new(2.0, 0.0, 0.0)),
            ..Default::default()
        })
        .insert(Animated {
            // animation: idle.clone(),
            // current_frame: 0,
            // start: FrameOffset::now(&last_tick_time),
            // next: VecDeque::new(),
            previous_frame: usize::MAX
        });
}

fn setup_particles(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
    // mut spark_effect: ResMut<SparkEffect>
) {
    let mut gradient = Gradient::new();
    let brightness = 2.0;
    gradient.add_key(
        0.0,
        (Vec4::new(255.0, 203.0, 125.0, 255.0) / 255.0) * brightness,
    );
    gradient.add_key(
        1.0,
        (Vec4::new(252.0, 128.0, 50.0, 0.0) / 255.0) * brightness,
    );

    commands.insert_resource(SparkEffect(
        effects.add(
            EffectAsset {
                name: "spark".into(),
                capacity: 32768,
                spawner: Spawner::once(Value::Single(64.0), true),
                ..Default::default()
            }
            .init(InitPositionSphereModifier {
                center: Vec3::ZERO,
                radius: 0.1,
                dimension: ShapeDimension::Volume,
            })
            .init(InitVelocitySphereModifier {
                center: Vec3::ZERO,
                speed: Value::Uniform((2.0, 4.0)),
            })
            .init(InitLifetimeModifier {
                lifetime: Value::Uniform((0.1, 2.0)),
            })
            .init(InitSizeModifier {
                size: DimValue::D1(Value::Uniform((0.03, 0.05))),
            })
            .update(AccelModifier::constant(Vec3::new(0.0, -9.8 * 0.4, 0.0)))
            .render(ColorOverLifetimeModifier { gradient }),
        ),
    ));
}

fn input(
    keyboard_input: Res<bevy::input::Input<KeyCode>>,
    mut local_input: ResMut<LocalInput>,
    local_player_query: Query<&mut Player, With<LocalMarker>>,
    last_tick_time: Res<LastTickTime>,
) {
    let local_player = local_player_query.single();
    if keyboard_input.just_pressed(KeyCode::A) {
        if let Some(current_attack) = &local_player.current_attack {
            let attack_recovered = local_player.attack_start_time
                + current_attack.startup_time
                + current_attack.recover_time;
            if FrameOffset::now(&last_tick_time) > attack_recovered {
                local_input.attacking = Some(FrameOffset::now(&last_tick_time));
            }
        } else {
            local_input.attacking = Some(FrameOffset::now(&last_tick_time));
        }
    }
}

fn poll_clients(mut session: ResMut<Session>) {
    session.0.poll_remote_clients();
}

fn handle_game_events(
    mut commands: Commands,
    mut ev_game: EventReader<GameEvent>,
    mut state: ResMut<GameState>,
    roboto: Res<Roboto>,
) {
    for event in ev_game.into_iter() {
        match event {
            GameEvent::GameOver { loser } => {
                println!("Game over!");
                let text;
                if loser.is_none() {
                    text = "Tie"
                } else if loser.unwrap() == 0 {
                    text = "Defeat"
                } else {
                    text = "Victory"
                }
                *state = GameState::Over;

                commands.spawn(TextBundle {
                    text: Text::from_section(
                        text,
                        TextStyle {
                            font: roboto.0.clone(),
                            font_size: 96.0,
                            color: Color::WHITE,
                            ..Default::default()
                        },
                    )
                    .with_alignment(TextAlignment::Center),
                    style: Style {
                        size: Size::all(Val::Percent(50.0)),
                        justify_content: JustifyContent::Center,
                        align_content: AlignContent::Center,
                        position_type: PositionType::Absolute,
                        position: UiRect {
                            top: Val::Percent(10.0),
                            left: Val::Percent(10.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                });
            }
        }
    }
}

fn update_animated_atlas(
    mut atlas_query: Query<(&mut AtlasSprite3dComponent, &Player)>,
    last_tick_time: Res<LastTickTime>,
) {
    for (mut atlas, player) in atlas_query.iter_mut() {
        let frame = ((player
            .attack_start_time
            .get_offset_seconds(&FrameOffset::now(&last_tick_time))
            .0
            / FRAMETIME) as usize)
            .min(atlas.atlas.len())
            % atlas.atlas.len();
        if frame != atlas.index {
            //For change detection purposes
            atlas.index = frame;
        }
    }
}

// fn update_animations(
//     mut animation_query: Query<(&mut AtlasSprite3dComponent, &mut Handle<Mesh>, &mut Handle<StandardMaterial>, &mut Animated)>,
//     last_tick_time: Res<LastTickTime>,
// ){
//     for (mut atlas, mut mesh_handle, mut material_handle, mut animated) in animation_query.iter_mut(){
//         if let Some(next) = &animated.next{
//             *mesh_handle = next.mesh.clone();
//             *material_handle= next.material.clone();
//             animated.next = None;
//         }
//         let frame = ((animated.start.get_offset_seconds(&FrameOffset::now(&last_tick_time)).0 / FRAMETIME) as usize).min(atlas.atlas.len()) % atlas.atlas.len();
//         if frame != atlas.index{ //For change detection purposes
//             atlas.index = frame;
//         }

//     }

// }
// fn update_animations(
//     mut animation_query: Query<(&mut Handle<Mesh>, &mut Handle<StandardMaterial>, &mut Animated)>,
//     last_tick_time: Res<LastTickTime>,
//     sound_library: Res<SoundLibrary>,
//     audio: Res<Audio>,
//     animation_assets: Res<Assets<Animation>>,
// ){
//     for (mut mesh_handle, mut material_handle, mut animated) in animation_query.iter_mut(){
//         let mut animation_changed = false;
//         // if animated.next.is_some(){
//         //     animated.animation = animated.next.take().unwrap();
//         //     animation_changed = true;
//         // }
//         dbg!(animated.next.len());
//         if let Some(next_animation) = animated.next.get(0){
//             match next_animation{
//                 AnimationQueueItem::Immediate(next_animation) => {
//                     animated.animation = next_animation.clone();
//                     animation_changed = true;
//                     animated.start = FrameOffset::now(&last_tick_time);
//                     animated.next.pop_front();
//                 },
//                 AnimationQueueItem::UponCompletion(next_animation) => {
//                     let animation = animation_assets.get(&animated.animation).unwrap();
//                     if (animated.start.get_offset_seconds(&FrameOffset::now(&last_tick_time)).0 / FRAMETIME) as usize > animation.0.len(){
//                         animated.animation = next_animation.clone();
//                         animation_changed = true;
//                         animated.start = FrameOffset::now(&last_tick_time);
//                         animated.next.pop_front();
//                     }
//                 },
//                 // AnimationQueueItem::Idle(next_animation) => {
//                 //     let animation = animation_assets.get(&animated.animation).unwrap();
//                 //     if (animated.start.get_offset_seconds(&FrameOffset::now(&last_tick_time)).0 / FRAMETIME) as usize > animation.0.len(){
//                 //         animated.animation = next_animation.clone();
//                 //         animation_changed = true;
//                 //     }
//                 // },
//             }
//         }

//         let animation_handle = animated.animation.clone();
//         let animation = animation_assets.get(&animated.animation).unwrap();
//         let frame = ((animated.start.get_offset_seconds(&FrameOffset::now(&last_tick_time)).0 / FRAMETIME) as usize).min(animation.0.len()) % animation.0.len();
//         if frame != animated.current_frame || animation_changed{ //For change detection purposes
//             animated.current_frame = frame;
//             let frame = &animation.0[animated.current_frame];
//             *mesh_handle = frame.mesh.clone();
//             *material_handle = frame.material.clone();
//             if let Some(trigger) = &frame.trigger{
//                 match trigger{
//                     AnimationTriggerType::Woosh => {
//                         audio.play(sound_library.swoosh.clone());
//                     },
//                     AnimationTriggerType::Loop => {
//                         println!("loop");
//                         if animated.next.len() == 0{
//                             println!("loop reset");
//                             // animated.start = FrameOffset::now(&last_tick_time);
//                             animated.next.push_back(AnimationQueueItem::UponCompletion(animation_handle.clone()));
//                         }
//                     },

//                 }
//             }

//         }

//     }

// }

fn update_animations(
    mut animation_query: Query<(
        &Player,
        &mut Handle<Mesh>,
        &mut Handle<StandardMaterial>,
        &mut Animated,
    )>,
    last_tick_time: Res<LastTickTime>,
    sound_library: Res<SoundLibrary>,
    time: Res<Time>,
    audio: Res<Audio>,
    animation_assets: Res<Assets<Animation>>,
    animation_library: Res<AnimationLibrary>,
) {
    for (player, mut mesh_handle, mut material_handle, mut animated) in animation_query.iter_mut() {
        let (animation, progress_seconds) = if player.current_attack.is_some() {
            (
                animation_library.counter_attack.clone(),
                player
                    .attack_start_time
                    .get_offset_seconds(&FrameOffset::now(&last_tick_time))
                    .0,
            )
        } else {
            (animation_library.idle.clone(), time.elapsed_seconds_f64())
        };
        let animation = animation_assets.get(&animation).unwrap();
        let frame = (progress_seconds / FRAMETIME) as usize % animation.0.len();
        if animated.previous_frame != frame{
            animated.previous_frame = frame;
            let frame = &animation.0[frame];
            *mesh_handle = frame.mesh.clone();
            *material_handle = frame.material.clone();
            if let Some(trigger) = &frame.trigger {
                match trigger {
                    AnimationTriggerType::Woosh => {
                        audio.play(sound_library.swoosh.clone());
                    }
                }
            }
        }

    }
}

// fn animation_effects(
//     atlas_query: Query<&AtlasSprite3dComponent, Changed<AtlasSprite3dComponent>>,
//     sound_library: Res<SoundLibrary>,
//     audio: Res<Audio>,
// ){
//     for atlas in atlas_query.iter(){
//         if atlas.index == 5{
//             audio.play(sound_library.swoosh.clone());
//         }
//     }
// }

fn block_sparks(
    mut commands: Commands,
    spark_effect: Res<SparkEffect>,
    mut ev_block: EventReader<BlockEvent>,
    remote_player: Query<&Transform, (With<Player>, Without<LocalMarker>)>,
    local_player: Query<&Transform, (With<Player>, With<LocalMarker>)>,
) {
    for _ in ev_block.iter() {
        let transform = EaseValue(remote_player.single().clone())
            .lerp(&EaseValue(local_player.single().clone()), &0.5)
            .0;
        commands.spawn(ParticleEffectBundle {
            effect: ParticleEffect::new(spark_effect.0.clone()),
            transform,
            ..Default::default()
        });
    }
}

fn rollback_system(
    mut session: ResMut<Session>,
    mut local_input: ResMut<LocalInput>,
    mut last_tick_time: ResMut<LastTickTime>,
    mut local_player_query: Query<&mut Player, With<LocalMarker>>,
    mut remote_player_query: Query<&mut Player, Without<LocalMarker>>,
    mut final_clash: ResMut<FinalClash>,
    // mut text_query: Query<&mut Text, With<BlockQualityIndicator>>,
    mut ev_block: EventWriter<BlockEvent>,
    mut ev_game: EventWriter<GameEvent>,
    audio_library: Res<SoundLibrary>,
    // animation_library: Res<AnimationLibrary>,
    audio: Res<Audio>,
    mut game_state: ResMut<GameState>,
    // audio_sinks: Res<Assets<AudioSink>>,
) {
    let attacking = if let Some(attacking) = local_input.attacking {
        attacking
    } else {
        FrameOffset {
            frame: usize::MAX,
            offset: 0,
        }
    };
    *local_input = LocalInput { attacking: None };
    session
        .0
        .add_local_input(0, SendInput { attacking })
        .unwrap();

    if session.0.frames_ahead() > 0 {
        sleep(Duration::from_secs_f64(FRAMETIME))
    }

    let advance_result = session.0.advance_frame();
    if let Ok(session) = advance_result {
        for request in session {
            match request {
                GGRSRequest::SaveGameState { cell, frame } => {
                    assert_eq!(last_tick_time.frame as i32, frame);
                    let world_snapshot = WorldSnapshot {
                        players: [
                            local_player_query.single().clone(),
                            remote_player_query.single().clone(),
                        ],
                        final_clash: final_clash.clone(),
                        game_state: game_state.clone(),
                    };
                    let mut hasher = DefaultHasher::new();
                    world_snapshot.hash(&mut hasher);
                    cell.save(
                        last_tick_time.frame as i32,
                        Some(world_snapshot),
                        Some(hasher.finish().into()),
                    )
                }
                GGRSRequest::LoadGameState { cell, frame } => {
                    let world_snapshot: WorldSnapshot = cell.load().unwrap();

                    *local_player_query.single_mut() = world_snapshot.players[0].clone();
                    *remote_player_query.single_mut() = world_snapshot.players[1].clone();

                    // let frame_delta = frame - last_tick_time.frame_offset.frame as i32;

                    // let new_offset = last_tick_time.frame_offset + Second(FRAMETIME * frame_delta as f64);

                    *game_state = world_snapshot.game_state;
                    *final_clash = world_snapshot.final_clash;
                    *last_tick_time = LastTickTime {
                        frame: frame as usize,
                        instant: Instant::now(),
                    }
                }
                GGRSRequest::AdvanceFrame { inputs } => {
                    last_tick_time.frame += 1;
                    last_tick_time.instant = Instant::now();
                    if matches!(*game_state, GameState::FinalClash) {
                        if final_clash.next_clash.is_none() {
                            let now = FrameOffset::now(&last_tick_time);
                            final_clash.next_clash = Some(now + CLASH_LENGTH);
                        }
                    }

                    for (handle, (received_input, status)) in inputs.into_iter().enumerate() {
                        assert!(!matches!(status, InputStatus::Disconnected));
                        let (
                            mut current_player,
                            mut other_player,
                        ) = if handle == 0 {
                            (
                                local_player_query.single_mut(),
                                remote_player_query.single_mut(),
                            )
                        } else {
                            (
                                remote_player_query.single_mut(),
                                local_player_query.single_mut(),
                            )
                        };

                        if matches!(*game_state, GameState::FinalClash) {
                            if received_input.attacking.is_valid()
                                && current_player.final_clash_last_swing.is_none()
                            {
                                current_player.final_clash_last_swing =
                                    Some(received_input.attacking);
                            }
                        } else {
                            let mut stamina_loss = Unorm64(0);
                            if received_input.attacking.is_valid() {
                                let swing_result = current_player.swing(
                                    &mut other_player,
                                    received_input.attacking,
                                    TEST_ATTACK,
                                );
                                if let Some(swing_result) = swing_result {
                                    // current_player_animation.next.push_back(
                                    //     AnimationQueueItem::Immediate(
                                    //         animation_library.counter_attack.clone(),
                                    //     ),
                                    // );

                                    let block_quality =
                                        1.0 - (swing_result.abs() as f32).clamp(0.0, 1.0);
                                    let sound_block_quality = block_quality.powf(16.0);
                                    audio.play_with_settings(
                                        audio_library.block.clone(),
                                        PlaybackSettings {
                                            repeat: false,
                                            volume: (1.0 - sound_block_quality) * VOLUME_SCALE,
                                            speed: 1.0,
                                        },
                                    );
                                    audio.play_with_settings(
                                        audio_library.perfect_block.clone(),
                                        PlaybackSettings {
                                            repeat: false,
                                            volume: sound_block_quality * VOLUME_SCALE,
                                            speed: 1.0,
                                        },
                                    );

                                    let is_local = handle == 0;
                                    if block_quality == 1.0 {
                                        if is_local {
                                            ev_block.send(BlockEvent("INHUMAN BLOCK".into()));
                                        }
                                        // stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 0.01);
                                    } else if block_quality > 0.999 {
                                        if is_local {
                                            ev_block.send(BlockEvent("Perfect Block".into()));
                                        }
                                        // stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 0.1);
                                    } else if block_quality > 0.99 {
                                        if is_local {
                                            ev_block.send(BlockEvent("Excellent Block".into()));
                                        }
                                        // stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 0.25);
                                    } else if block_quality > 0.9 {
                                        if is_local {
                                            ev_block.send(BlockEvent("Good Block".into()));
                                        }
                                        // stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 0.5);
                                    } else if block_quality > 0.8 {
                                        if is_local {
                                            ev_block.send(BlockEvent("Decent Block".into()));
                                        }
                                        // stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 1.0);
                                    } else {
                                        if is_local {
                                            ev_block.send(BlockEvent("Sloppy Block".into()));
                                        }
                                        // stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 1.2);
                                    }
                                    stamina_loss = Unorm64::from_f64(
                                        BASE_STAMINA_LOSS
                                            * (1.0 - (block_quality * 0.8)).powf(1.0) as f64,
                                    )
                                } else {
                                    // current_player_animation.next.push_back(
                                    //     AnimationQueueItem::Immediate(
                                    //         animation_library.attack.clone(),
                                    //     ),
                                    // );
                                }
                                // current_player_animation.start = FrameOffset::now(&last_tick_time);
                                // current_player_animation.next.push_back(
                                //     AnimationQueueItem::UponCompletion(
                                //         animation_library.idle.clone(),
                                //     ),
                                // );
                            } else {
                                if let Some(current_attack) = &other_player.current_attack {
                                    if FrameOffset::now(&last_tick_time)
                                        > other_player.attack_start_time
                                            + current_attack.startup_time
                                            + current_attack.block_grace
                                    {
                                        stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 1.5);
                                        other_player.current_attack = None;
                                    }
                                }
                            }
                            if stamina_loss < current_player.stamina {
                                current_player.stamina.0 -= stamina_loss.0;
                            } else {
                                if stamina_loss.0 > 0 && current_player.stamina == Unorm64(0) {
                                    ev_game.send(GameEvent::GameOver {
                                        loser: Some(handle),
                                    });
                                } else {
                                    current_player.stamina = Unorm64(0);
                                }
                            }
                        }
                    }
                    let (
                        mut local_player,
                        mut remote_player,
                    ) = {
                        (
                            local_player_query.single_mut(),
                            remote_player_query.single_mut(),
                        )
                    };
                    if matches!(*game_state, GameState::FinalClash) {
                        if let Some(next_clash) = final_clash.next_clash {
                            if FrameOffset::now(&last_tick_time)
                                .get_offset_seconds(&final_clash.next_clash.unwrap())
                                < -CLASH_LENGTH
                            {
                                println!("next clash");
                                if local_player.final_clash_last_swing.is_none() {
                                    local_player.take_final_clash_life();
                                    if remote_player.final_clash_last_swing.is_some() {
                                        audio.play_with_settings(
                                            audio_library.flesh_cut.clone(),
                                            PlaybackSettings {
                                                repeat: false,
                                                volume: 1.0 * VOLUME_SCALE,
                                                speed: 1.0,
                                            },
                                        );
                                    }
                                }
                                if remote_player.final_clash_last_swing.is_none() {
                                    remote_player.take_final_clash_life();
                                    if local_player.final_clash_last_swing.is_some() {
                                        audio.play_with_settings(
                                            audio_library.flesh_cut.clone(),
                                            PlaybackSettings {
                                                repeat: false,
                                                volume: 1.0 * VOLUME_SCALE,
                                                speed: 1.0,
                                            },
                                        );
                                    }
                                }

                                final_clash.next_clash = None;
                                local_player.final_clash_last_swing = None;
                                remote_player.final_clash_last_swing = None;
                            } else {
                                match (
                                    local_player.final_clash_last_swing,
                                    remote_player.final_clash_last_swing,
                                ) {
                                    (Some(local_clash), Some(remote_clash)) => {
                                        let local_offset =
                                            local_clash.get_offset_seconds(&next_clash);
                                        let remote_offset =
                                            remote_clash.get_offset_seconds(&next_clash);
                                        if local_offset.0.abs() < remote_offset.0.abs() {
                                            remote_player.take_final_clash_life()
                                        }
                                        if remote_offset.0.abs() < local_offset.0.abs() {
                                            local_player.take_final_clash_life()
                                        }

                                        audio.play_with_settings(
                                            audio_library.block.clone(),
                                            PlaybackSettings {
                                                repeat: false,
                                                volume: 1.0 * VOLUME_SCALE,
                                                speed: 1.0,
                                            },
                                        );

                                        final_clash.next_clash = None;
                                        local_player.final_clash_last_swing = None;
                                        remote_player.final_clash_last_swing = None;
                                    }
                                    (_, _) => {}
                                }
                            }

                            let local_dead = local_player.final_clash_lives == 0;
                            let remote_dead = remote_player.final_clash_lives == 0;
                            match (local_dead, remote_dead) {
                                (true, true) => ev_game.send(GameEvent::GameOver { loser: None }),
                                (true, false) => {
                                    ev_game.send(GameEvent::GameOver { loser: Some(1) })
                                }
                                (false, true) => {
                                    ev_game.send(GameEvent::GameOver { loser: Some(0) })
                                }
                                (false, false) => {}
                            }
                        }
                    } else {
                        if local_player.stamina == Unorm64(0) && remote_player.stamina == Unorm64(0)
                        {
                            println!("Beginning final clash");
                            *game_state = GameState::FinalClash;
                        }
                    }
                }
            }
        }
    }
}
