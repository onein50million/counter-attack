mod ui;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    net::{SocketAddr},
    thread::sleep,
    time::{Duration, Instant},
};

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    time::common_conditions::on_timer,
};
use bytemuck::{Pod, Zeroable};
use clap::Parser;
use ggrs::{
    Config, GGRSRequest, InputStatus, P2PSession, PlayerType, SessionBuilder, UdpNonBlockingSocket, PlayerHandle,
};
use iunorm::{Inorm64, Unorm64};
use ui::{GUI, Roboto};

const FRAMETIME: usize = 100;
const FPS: usize = 1000 / FRAMETIME;
const VOLUME_SCALE: f32 = 0.5;
const BASE_STAMINA_LOSS: f64 = 0.1;

#[derive(Parser, Debug)]
struct Args {
    local_port: u16,
    remote_addr: SocketAddr,
}

#[derive(Clone, Debug, Hash)]
struct Attack {
    startup_time: Duration,
    block_grace: Duration,
    recover_time: Duration,
}

const TEST_ATTACK: Attack = Attack {
    startup_time: Duration::from_millis(1000),
    block_grace: Duration::from_millis(200),
    recover_time: Duration::from_millis(200),
};

#[derive(Resource, Debug)]
struct LastTickTime {
    frame_number: usize,
    frame_instant: Instant,
}
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable, Hash)]
struct FrameOffset {
    frame: usize,
    offset: u64,
}
//TODO: Figure out why converting Instant::now() to frameoffset and back to an instant is not a no-op. Seems like it works so /shrug
impl FrameOffset {
    fn from_instant(instant: Instant, last_tick_time: &LastTickTime) -> Self {
        let raw_offset = if last_tick_time.frame_instant > instant {
            last_tick_time
                .frame_instant
                .duration_since(instant)
                .as_secs_f64()
        } else {
            -instant
                .duration_since(last_tick_time.frame_instant)
                .as_secs_f64()
        };
        let frametime_seconds = FRAMETIME as f64 / 1000.0;
        let frame_offset = raw_offset / frametime_seconds;
        let remaining_offset = raw_offset.rem_euclid(frametime_seconds);
        let frame = last_tick_time.frame_number + frame_offset as usize;
        Self {
            frame,
            offset: Unorm64::from_f64(remaining_offset).0,
        }
    }
    fn to_instant(&self, last_tick_time: &LastTickTime) -> Instant {
        let offset = Unorm64(self.offset).to_f64();
        let frame_delta = last_tick_time.frame_number as f64 - self.frame as f64;
        let frame_delta = frame_delta * (FRAMETIME as f64 / 1000.0) + offset;
        if frame_delta > 0.0 {
            last_tick_time.frame_instant - Duration::from_secs_f64(frame_delta)
        } else {
            last_tick_time.frame_instant + Duration::from_secs_f64(frame_delta.abs())
        }
    }
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
}
impl Player {
    fn swing(
        &mut self,
        other: &mut Self,
        frame_offset: FrameOffset,
        last_tick_time: &LastTickTime,
        attack: Attack,
    ) -> Option<f64> {
        self.attack_start_time = frame_offset;
        self.attack_recover_time = FrameOffset::from_instant(frame_offset.to_instant(&last_tick_time) + attack.startup_time + attack.recover_time, &last_tick_time);
        self.current_attack = Some(attack);

        let defend_time = frame_offset.to_instant(&last_tick_time);
        let impact_time = other.attack_start_time.to_instant(&last_tick_time)
            + other.current_attack.as_ref()?.startup_time;

        let defend_time_offset = if defend_time > impact_time {
            defend_time.duration_since(impact_time).as_secs_f64()
        } else {
            -impact_time.duration_since(defend_time).as_secs_f64()
        };
        other.current_attack = None;
        dbg!(defend_time_offset);
        self.last_defend_result = Inorm64::from_f64(defend_time_offset).0;
        Some(defend_time_offset)
    }
}

#[derive(Debug, Resource)]
struct LocalInput {
    attacking: Option<Instant>,
    // defending: Option<Instant>,
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

#[derive(Clone, Hash)]
struct WorldSnapshot {
    players: [Player; 2],
}

#[derive(Resource)]
struct Session(P2PSession<GGRSConfig>);

#[derive(Resource)]
struct SoundLibrary {
    block: Handle<AudioSource>,
    perfect_block: Handle<AudioSource>,
}

#[derive(Component)]
pub struct LocalMarker;

pub struct BlockEvent(String);
pub enum GameEvent{
    GameOver{
        loser: PlayerHandle
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, States)]
pub enum GameState{
    #[default]
    Waiting,
    Started,
    Over
}
fn main() {
    let args = Args::parse();

    let mut app = App::new();

    let session_builder = SessionBuilder::<GGRSConfig>::new()
        .with_fps(FPS)
        .unwrap()
        .add_player(PlayerType::Local, 0)
        .unwrap()
        .add_player(
            PlayerType::Remote(args.remote_addr),
            1,
        )
        .unwrap();

    let udp_socket = UdpNonBlockingSocket::bind_to_port(args.local_port).unwrap();
    let session = session_builder.start_p2p_session(udp_socket).unwrap();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            present_mode: bevy::window::PresentMode::Immediate,
            ..Default::default()
        }),
        ..Default::default()
    }))
    .add_plugin(LogDiagnosticsPlugin::default())
    .add_plugin(FrameTimeDiagnosticsPlugin::default())
    .add_plugin(GUI)
    .add_state::<GameState>()
    .add_event::<BlockEvent>()
    .add_event::<GameEvent>()
    .add_startup_system(setup_players)
    .add_startup_system(setup_audio)
    .add_system(input)
    .add_system(rollback_system.run_if(on_timer(Duration::from_millis(
        FRAMETIME.try_into().unwrap(),
    )).and_then(not(in_state(GameState::Over)))))
    .add_system(network_stats.run_if(on_timer(Duration::from_secs_f64(5.0))))
    .add_system(poll_clients)
    .add_system(handle_game_events.run_if(not(in_state(GameState::Over))))
    .insert_resource(Session(session))
    .insert_resource(LastTickTime {
        frame_number: 0,
        frame_instant: Instant::now(),
    })
    .insert_resource(LocalInput {
        attacking: None,
        // defending: None,
    })
    .run();
}

fn network_stats(session: Res<Session>) {
    // dbg!(session.0.network_stats(0));
    println!("{:?}", session.0.network_stats(1));
}

fn setup_audio(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(SoundLibrary {
        block: asset_server.load("block.ogg"),
        perfect_block: asset_server.load("perfect_block.ogg"),
    });
}

fn setup_players(mut commands: Commands, last_tick_time: Res<LastTickTime>) {
    commands
        .spawn(Player {
            current_attack: None,
            attack_start_time: FrameOffset::from_instant(Instant::now(), &last_tick_time),
            attack_recover_time: FrameOffset::from_instant(Instant::now(), &last_tick_time),
            last_defend_result: 0,
            stamina: Unorm64(u64::MAX),
        })
        .insert(LocalMarker);
    commands.spawn(Player {
        current_attack: None,
        attack_start_time: FrameOffset::from_instant(Instant::now(), &last_tick_time),
        attack_recover_time: FrameOffset::from_instant(Instant::now(), &last_tick_time),
        last_defend_result: 0,
        stamina: Unorm64(u64::MAX),
    });
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
            let attack_recovered = local_player.attack_start_time.to_instant(&last_tick_time)
                + current_attack.startup_time
                + current_attack.recover_time;
            if FrameOffset::from_instant(Instant::now(), &last_tick_time)
                .to_instant(&last_tick_time)
                > attack_recovered
            {
                local_input.attacking = Some(Instant::now());
            }
        } else {
            local_input.attacking = Some(Instant::now());
        }
    }
}

fn poll_clients(mut session: ResMut<Session>) {
    session.0.poll_remote_clients();
}

fn handle_game_events(
    mut commands: Commands,
    mut ev_game: EventReader<GameEvent>,
    mut next_state: ResMut<NextState<GameState>>,
    roboto: Res<Roboto>,
){
    for event in ev_game.into_iter(){
        match event{
            GameEvent::GameOver { loser } => {
                println!("Game over!");
                let text;
                if loser == &0{
                    text = "Defeat"
                }else{
                    text = "Victory"
                }
                next_state.set(GameState::Over);

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
            },
        }
    }
}

fn rollback_system(
    mut session: ResMut<Session>,
    mut local_input: ResMut<LocalInput>,
    mut last_tick_time: ResMut<LastTickTime>,
    mut local_player_query: Query<&mut Player, With<LocalMarker>>,
    mut remote_player_query: Query<&mut Player, Without<LocalMarker>>,
    // mut text_query: Query<&mut Text, With<BlockQualityIndicator>>,
    mut ev_block: EventWriter<BlockEvent>,
    mut ev_game: EventWriter<GameEvent>,
    audio_library: ResMut<SoundLibrary>,
    audio: Res<Audio>,
    // audio_sinks: Res<Assets<AudioSink>>,
) {
    let attacking = if let Some(attacking) = local_input.attacking {
        FrameOffset::from_instant(attacking, &last_tick_time)
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
        sleep(Duration::from_millis((FRAMETIME).try_into().unwrap()))
    }

    let advance_result = session.0.advance_frame();
    if let Ok(session) = advance_result {
        for request in session {
            match request {
                GGRSRequest::SaveGameState { cell, frame } => {
                    // println!("SaveGameState");
                    // dbg!(frame);
                    // let mut player_iter = player_query.iter();
                    assert_eq!(last_tick_time.frame_number as i32, frame);
                    let world_snapshot = WorldSnapshot {
                        players: [
                            local_player_query.single().clone(),
                            remote_player_query.single().clone(),
                        ],
                    };
                    let mut hasher = DefaultHasher::new();
                    world_snapshot.hash(&mut hasher);
                    cell.save(
                        last_tick_time.frame_number as i32,
                        Some(world_snapshot),
                        Some(hasher.finish().into()),
                    )
                }
                GGRSRequest::LoadGameState { cell, frame } => {
                    // println!("LoadGameState");
                    // dbg!(frame);
                    let world_snapshot: WorldSnapshot = cell.load().unwrap();

                    *local_player_query.single_mut() = world_snapshot.players[0].clone();
                    *remote_player_query.single_mut() = world_snapshot.players[1].clone();

                    let frame_delta = frame - last_tick_time.frame_number as i32;

                    let new_instant = if frame_delta > 0 {
                        last_tick_time.frame_instant
                            + Duration::from_millis(FRAMETIME as u64)
                                * frame_delta.abs().try_into().unwrap()
                    } else {
                        last_tick_time.frame_instant
                            - Duration::from_millis(FRAMETIME as u64)
                                * frame_delta.abs().try_into().unwrap()
                    };

                    *last_tick_time = LastTickTime {
                        frame_number: frame as usize,
                        frame_instant: new_instant,
                    };
                }
                GGRSRequest::AdvanceFrame { inputs } => {
                    // println!("AdvanceFrame");
                    *last_tick_time = LastTickTime {
                        frame_number: last_tick_time.frame_number + 1,
                        // frame_instant: last_tick_time.frame_instant + Duration::from_millis(FRAMETIME as u64),
                        frame_instant: Instant::now(),
                    };
                    for (handle, (received_input, status)) in inputs.into_iter().enumerate() {
                        assert!(!matches!(status, InputStatus::Disconnected));
                        // dbg!(received_input);
                        let (mut current_player, mut other_player) = if handle == 0 {
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
                        let mut stamina_loss = Unorm64(0);
                        if received_input.attacking.is_valid() {
                            let swing_result = current_player.swing(
                                &mut other_player,
                                received_input.attacking,
                                &last_tick_time,
                                TEST_ATTACK,
                            );
                            if let Some(swing_result) = swing_result {
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

                                // let mut text = text_query.single_mut();
                                // text.sections[0].style.color.set_a(1.0);

                                let is_local = handle == 0;
                                if block_quality == 1.0 {
                                    if is_local {
                                        ev_block.send(BlockEvent("INHUMAN BLOCK".into()));
                                    }
                                    stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 0.01);
                                } else if block_quality > 0.999 {
                                    if is_local {
                                        ev_block.send(BlockEvent("Perfect Block".into()));
                                    }
                                    stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 0.1);
                                } else if block_quality > 0.99 {
                                    if is_local {
                                        ev_block.send(BlockEvent("Excellent Block".into()));
                                    }
                                    stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 0.25);
                                } else if block_quality > 0.9 {
                                    if is_local {
                                        ev_block.send(BlockEvent("Good Block".into()));
                                    }
                                    stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 0.5);
                                } else if block_quality > 0.8 {
                                    if is_local {
                                        ev_block.send(BlockEvent("Decent Block".into()));
                                    }
                                    stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 1.0);
                                } else {
                                    if is_local {
                                        ev_block.send(BlockEvent("Sloppy Block".into()));
                                    }
                                    stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 1.2);
                                }
                            }
                        }
                        if let Some(current_attack) = &other_player.current_attack {
                            if FrameOffset::from_instant(Instant::now(), &last_tick_time)
                                .to_instant(&last_tick_time)
                                > other_player.attack_start_time.to_instant(&last_tick_time)
                                    + current_attack.startup_time
                                    + current_attack.block_grace
                            {
                                stamina_loss = Unorm64::from_f64(BASE_STAMINA_LOSS * 1.5);
                                other_player.current_attack = None;
                            }
                        }
                        if stamina_loss < current_player.stamina {
                            current_player.stamina.0 -= stamina_loss.0;
                        } else {
                            //GAMEOVER
                            current_player.stamina = Unorm64(0);
                            ev_game.send(GameEvent::GameOver { loser: handle});
                        }
                    }
                }
            }
        }
    }
}
