use bevy::{prelude::{BackgroundColor, *}, core_pipeline::bloom::BloomSettings};
use iunorm::Inorm64;

use crate::{BlockEvent, FrameOffset, LastTickTime, LocalMarker, Player, FinalClashLives, GameState, FinalClash, AssetLoadingState};

#[derive(Component)]
struct MovingCaret;

#[derive(Component)]
struct StaminaBar;

#[derive(Component)]
pub struct BlockQualityIndicator;
#[derive(Component)]
pub struct GameStateViewer;

#[derive(Resource)]
pub struct Roboto(pub Handle<Font>);

pub struct GUI;

impl Plugin for GUI {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.insert_resource(Roboto(
            app.world.resource::<AssetServer>().load("roboto.ttf"),
        ))
        .add_systems((
            setup_timing_indicator,
            setup_stamina_bar,
            setup_block_quality,
            setup_final_clash_lives,
            setup_state_viewer,
        ).in_schedule(OnEnter(AssetLoadingState::Done)))
        .add_systems((
            handle_block_event,
            update_block_quality,
            update_local_stamina_bar,
            update_remote_stamina_bar,
            move_caret,
            move_remote_caret,
            update_local_final_clash_lives.run_if(resource_exists_and_equals(GameState::FinalClash)),
            update_remote_final_clash_lives.run_if(resource_exists_and_equals(GameState::FinalClash)),
            move_caret_final_clash.run_if(resource_exists_and_equals(GameState::FinalClash)),
            update_state_viewer,
        ).distributive_run_if(in_state(AssetLoadingState::Done)));
    }
}

fn setup_block_quality(mut commands: Commands, roboto: Res<Roboto>) {
    commands
        .spawn(TextBundle {
            text: Text::from_section(
                "",
                TextStyle {
                    font: roboto.0.clone(),
                    font_size: 32.0,
                    color: Color::Rgba {
                        red: 1.0,
                        green: 1.0,
                        blue: 1.0,
                        alpha: 1.0,
                    },
                    ..Default::default()
                },
            )
            .with_alignment(TextAlignment::Center),
            style: Style {
                size: Size::all(Val::Percent(25.0)),
                justify_content: JustifyContent::Center,
                align_content: AlignContent::Center,
                position_type: PositionType::Absolute,
                position: UiRect {
                    top: Val::Percent(50.0),
                    left: Val::Percent(30.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        })
        .insert(BlockQualityIndicator);
}

fn update_block_quality(
    time: Res<Time>,
    mut text_query: Query<&mut Text, With<BlockQualityIndicator>>,
) {
    let mut text = text_query.single_mut();
    let style = &mut text.sections.first_mut().unwrap().style;
    style.color.set_a(style.color.a() - time.delta_seconds());
}

fn handle_block_event(
    mut ev_block: EventReader<BlockEvent>,
    mut text_query: Query<&mut Text, With<BlockQualityIndicator>>,
) {
    let mut text = text_query.single_mut();

    for event in ev_block.into_iter() {
        text.sections[0].style.color.set_a(1.0);
        text.sections[0].value = event.0.clone();
    }
}

fn setup_stamina_bar(mut commands: Commands) {
    // let font = &roboto.0;

    commands
        .spawn(NodeBundle {
            style: Style {
                size: Size {
                    width: Val::Percent(80.0),
                    height: Val::Percent(5.0),
                },
                justify_content: JustifyContent::Center,
                position_type: PositionType::Absolute,
                position: UiRect {
                    bottom: Val::Percent(10.0),
                    left: Val::Percent(10.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            background_color: BackgroundColor(Color::BLACK),
            ..Default::default()
        })
        .with_children(|parent| {
            parent
                .spawn(NodeBundle {
                    style: Style {
                        size: Size::all(Val::Percent(100.0)),
                        justify_content: JustifyContent::Start,
                        align_content: AlignContent::Start,
                        position_type: PositionType::Absolute,
                        position: UiRect {
                            bottom: Val::Percent(0.0),
                            left: Val::Percent(0.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    background_color: BackgroundColor(Color::GREEN),
                    ..Default::default()
                })
                .insert(StaminaBar).insert(LocalMarker);
        });
    commands
        .spawn(NodeBundle {
            style: Style {
                size: Size {
                    width: Val::Percent(80.0),
                    height: Val::Percent(5.0),
                },
                justify_content: JustifyContent::Center,
                position_type: PositionType::Absolute,
                position: UiRect {
                    bottom: Val::Percent(20.0),
                    left: Val::Percent(10.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            background_color: BackgroundColor(Color::BLACK),
            ..Default::default()
        })
        .with_children(|parent| {
            parent
                .spawn(NodeBundle {
                    style: Style {
                        size: Size::all(Val::Percent(100.0)),
                        justify_content: JustifyContent::Start,
                        align_content: AlignContent::Start,
                        position_type: PositionType::Absolute,
                        position: UiRect {
                            bottom: Val::Percent(0.0),
                            left: Val::Percent(0.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    background_color: BackgroundColor(Color::RED),
                    ..Default::default()
                })
                .insert(StaminaBar);
        });
}

fn update_local_stamina_bar(
    mut query: Query<&mut Style, (With<StaminaBar>, With<LocalMarker>)>,
    player: Query<&Player, With<LocalMarker>>,
) {
    let player = player.single();
    let mut style = query.single_mut();
    style.size.width = Val::Percent(player.stamina.to_f32() * 100.0)
}
fn update_remote_stamina_bar(
    mut query: Query<&mut Style, (With<StaminaBar>, Without<LocalMarker>)>,
    player: Query<&Player, Without<LocalMarker>>,
) {
    let player = player.single();
    let mut style = query.single_mut();
    style.size.width = Val::Percent(player.stamina.to_f32() * 100.0)
}

fn setup_timing_indicator(mut commands: Commands, roboto: Res<Roboto>) {
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(0.0, 0.0, 10.0),
        camera: Camera{
            hdr: true,
            ..Default::default()
        },
        ..Default::default()
    }).insert(
        BloomSettings::default()
    );
    let font = &roboto.0;
    // let font: Handle<Font> = asset_server.load("roboto.ttf");

    commands.spawn(TextBundle {
        text: Text::from_section(
            "|",
            TextStyle {
                font: font.clone(),
                font_size: 32.0,
                color: Color::WHITE,
                ..Default::default()
            },
        )
        .with_alignment(TextAlignment::Center),
        style: Style {
            size: Size::all(Val::Percent(100.0)),
            justify_content: JustifyContent::Center,
            align_content: AlignContent::Center,
            position_type: PositionType::Absolute,
            position: UiRect {
                top: Val::Percent(10.0),
                left: Val::Percent(50.0),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });
    commands
        .spawn(TextBundle {
            text: Text::from_section(
                "^",
                TextStyle {
                    font: font.clone(),
                    font_size: 32.0,
                    color: Color::WHITE,
                    ..Default::default()
                },
            )
            .with_alignment(TextAlignment::Center),
            style: Style {
                size: Size::all(Val::Percent(100.0)),
                justify_content: JustifyContent::Center,
                align_content: AlignContent::Center,
                position_type: PositionType::Absolute,
                position: UiRect {
                    top: Val::Percent(10.0),
                    left: Val::Percent(50.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        })
        .insert(MovingCaret)
        .insert(LocalMarker);

    commands
        .spawn(TextBundle {
            text: Text::from_section(
                "^",
                TextStyle {
                    font: font.clone(),
                    font_size: 32.0,
                    color: Color::RED,
                    ..Default::default()
                },
            )
            .with_alignment(TextAlignment::Center),
            style: Style {
                size: Size::all(Val::Percent(100.0)),
                justify_content: JustifyContent::Center,
                align_content: AlignContent::Center,
                position_type: PositionType::Absolute,
                position: UiRect {
                    top: Val::Percent(10.0),
                    left: Val::Percent(50.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        })
        .insert(MovingCaret);
}

fn move_caret(
    mut style_query: Query<&mut Style, (With<MovingCaret>, With<LocalMarker>)>,
    remote_player: Query<&Player, Without<LocalMarker>>,
    local_player: Query<&Player, With<LocalMarker>>,
    last_tick_time: Res<LastTickTime>,
) {
    let mut style = style_query.single_mut();
    let remote_player = remote_player.single();
    let local_player = local_player.single();
    if let Some(current_attack) = &remote_player.current_attack {
        let start_offset = remote_player.attack_start_time;
        let impact_offset = start_offset + current_attack.startup_time;
        let now = FrameOffset::now(&last_tick_time);
        let offset = now.get_offset_seconds(&impact_offset);

        style.position.left = Val::Percent(50.0 + offset.0 as f32 * 100.0);
    } else {
        style.position.left =
            Val::Percent(50.0 + (Inorm64(local_player.last_defend_result).to_f32() * 100.0))
    }
}

fn move_remote_caret(
    mut style_query: Query<&mut Style, (With<MovingCaret>, Without<LocalMarker>)>,
    remote_player: Query<&Player, Without<LocalMarker>>,
    local_player: Query<&Player, With<LocalMarker>>,
    last_tick_time: Res<LastTickTime>,
) {
    let mut style = style_query.single_mut();
    let remote_player = remote_player.single();
    let local_player = local_player.single();
    if let Some(current_attack) = &local_player.current_attack {
        let start_offset = local_player.attack_start_time;
        let impact_offset = start_offset + current_attack.startup_time;
        let now = FrameOffset::now(&last_tick_time);
        let offset = now.get_offset_seconds(&impact_offset);

        style.position.left = Val::Percent(50.0 + offset.0 as f32 * 100.0);
    } else {
        style.position.left =
            Val::Percent(50.0 + (Inorm64(remote_player.last_defend_result).to_f32() * 100.0))
    }
}

fn move_caret_final_clash(
    mut style_query: Query<&mut Style, (With<MovingCaret>, With<LocalMarker>)>,
    final_clash: Res<FinalClash>,
    last_tick_time: Res<LastTickTime>,
) {
    let mut style = style_query.single_mut();
    if let Some(next_clash) = final_clash.next_clash{
        let now = FrameOffset::now(&last_tick_time);
        style.position.left =
            Val::Percent(50.0 + (now.get_offset_seconds(&next_clash).0 * 100.0 )as f32)

    }
}

fn setup_state_viewer(mut commands: Commands, roboto: Res<Roboto>){
    commands.spawn(TextBundle {
        text: Text::from_section(
            "game state here",
            TextStyle {
                font: roboto.0.clone(),
                font_size: 8.0,
                color: Color::WHITE,
                ..Default::default()
            },
        )
        .with_alignment(TextAlignment::Center),
        style: Style {
            // size: Size::all(Val::Percent(50.0)),
            justify_content: JustifyContent::Center,
            align_content: AlignContent::Center,
            position_type: PositionType::Absolute,
            position: UiRect {
                bottom: Val::Percent(2.0),
                right: Val::Percent(2.0),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }).insert(GameStateViewer);
}

fn update_state_viewer(
    game_state: Res<GameState>,
    mut text_query: Query<&mut Text, With<GameStateViewer>>,
){
    text_query.single_mut().sections[0].value = format!("{:?}", game_state);
}
fn setup_final_clash_lives(mut commands: Commands, roboto: Res<Roboto>){

    commands
        .spawn(TextBundle {
            text: Text::from_section(
                "",
                TextStyle {
                    font: roboto.0.clone(),
                    font_size: 48.0,
                    color: Color::WHITE,
                    ..Default::default()
                },
            )
            .with_alignment(TextAlignment::Center),
            style: Style {
                // size: Size::all(Val::Percent(50.0)),
                justify_content: JustifyContent::Center,
                align_content: AlignContent::Center,
                position_type: PositionType::Absolute,
                position: UiRect {
                    bottom: Val::Percent(10.0),
                    right: Val::Percent(10.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        })
        .insert(LocalMarker)
        .insert(FinalClashLives);
    commands
        .spawn(TextBundle {
            text: Text::from_section(
                "",
                TextStyle {
                    font: roboto.0.clone(),
                    font_size: 48.0,
                    color: Color::WHITE,
                    ..Default::default()
                },
            )
            .with_alignment(TextAlignment::Center),
            style: Style {
                // size: Size::all(Val::Percent(50.0)),
                justify_content: JustifyContent::Center,
                align_content: AlignContent::Center,
                position_type: PositionType::Absolute,
                position: UiRect {
                    bottom: Val::Percent(10.0),
                    left: Val::Percent(10.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        })
        .insert(FinalClashLives);
}

fn update_local_final_clash_lives(
    mut lives_query: Query<&mut Text, (With<FinalClashLives>, With<LocalMarker>)>,
    player_query: Query<&Player, With<LocalMarker>>,
) {
    if let Ok(mut text) = lives_query.get_single_mut(){
        text.sections[0].value = "* ".repeat(player_query.single().final_clash_lives as usize);
    }
}
fn update_remote_final_clash_lives(
    mut lives_query: Query<&mut Text, (With<FinalClashLives>, Without<LocalMarker>)>,
    player_query: Query<&Player, Without<LocalMarker>>,
) {
    if let Ok(mut text) = lives_query.get_single_mut(){
        text.sections[0].value = "* ".repeat(player_query.single().final_clash_lives as usize);
    }}
