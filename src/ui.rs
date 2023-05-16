use std::time::Instant;

use bevy::{prelude::{*, BackgroundColor}};
use iunorm::Inorm64;

use crate::{LastTickTime, Player, FrameOffset, LocalMarker, BlockEvent};

#[derive(Component)]
struct MovingCaret;

#[derive(Component)]
struct StaminaBar;

#[derive(Component)]
pub struct BlockQualityIndicator;


#[derive(Resource)]
pub struct Roboto(pub Handle<Font>);

pub struct GUI;

impl Plugin for GUI{
    fn build(&self, app: &mut bevy::prelude::App) {
        app
        .insert_resource(Roboto(app.world.resource::<AssetServer>().load("roboto.ttf")))
        .add_startup_system(setup_timing_indicator)
        .add_startup_system(setup_stamina_bar)
        .add_startup_system(setup_block_quality)
        .add_system(handle_block_event)
        .add_system(update_block_quality)
        .add_system(update_stamina_bar)
        .add_system(move_caret)
        .add_system(move_remote_caret);
    }
}

fn setup_block_quality(mut commands: Commands, roboto: Res<Roboto>){
    commands.spawn(TextBundle {
        text: Text::from_section(
            "",
            TextStyle {
                font: roboto.0.clone(),
                font_size: 32.0,
                color: Color::Rgba { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 },
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
    }).insert(BlockQualityIndicator);
}

fn update_block_quality(
    time: Res<Time>,
    mut text_query: Query<&mut Text, With<BlockQualityIndicator>>,
){
    let mut text = text_query.single_mut();
    let style = &mut text.sections.first_mut().unwrap().style;
    style.color.set_a(style.color.a() - time.delta_seconds());
}

fn handle_block_event(
    mut ev_block: EventReader<BlockEvent>,
    mut text_query: Query<&mut Text, With<BlockQualityIndicator>>,
){
    let mut text = text_query.single_mut();

    for event in ev_block.into_iter(){
        text.sections[0].style.color.set_a(1.0);
        text.sections[0].value = event.0.clone();
    }

}

fn setup_stamina_bar(mut commands: Commands){
    // let font = &roboto.0;

    commands.spawn(NodeBundle{
        style: Style {
            size: Size{
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
    }).with_children(|parent|{
        parent.spawn(
            NodeBundle{
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
            }
        ).insert(StaminaBar);
    });
}

fn update_stamina_bar(
    mut query: Query<&mut Style, With<StaminaBar>>,
    remote_player: Query<&Player, Without<LocalMarker>>,
    local_player: Query<&Player, With<LocalMarker>>,

){
    let _remote_player = remote_player.single();
    let local_player = local_player.single();
    let mut style = query.single_mut();
    style.size.width = Val::Percent(local_player.stamina.to_f32()*100.0)
}

fn setup_timing_indicator(mut commands: Commands, roboto: Res<Roboto>) {
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(0.0, 0.0, -10.0),
        ..Default::default()
    });
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
        let start_instant = remote_player.attack_start_time.to_instant(&last_tick_time);
        let impact_instant = start_instant + current_attack.startup_time;
        let now =
            FrameOffset::from_instant(Instant::now(), &last_tick_time).to_instant(&last_tick_time);
        let offset = if now > impact_instant {
            -now.duration_since(impact_instant).as_secs_f32()
        } else {
            impact_instant.duration_since(now).as_secs_f32()
        };

        style.position.left = Val::Percent(50.0 + offset * 100.0);
    } else {
        style.position.left =
            Val::Percent(50.0 - (Inorm64(local_player.last_defend_result).to_f32() * 100.0))
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
        let start_instant = local_player.attack_start_time.to_instant(&last_tick_time);
        let impact_instant = start_instant + current_attack.startup_time;
        let now =
            FrameOffset::from_instant(Instant::now(), &last_tick_time).to_instant(&last_tick_time);
        let offset = if now > impact_instant {
            -now.duration_since(impact_instant).as_secs_f32()
        } else {
            impact_instant.duration_since(now).as_secs_f32()
        };

        style.position.left = Val::Percent(50.0 + offset * 100.0);
    } else {
        style.position.left =
            Val::Percent(50.0 - (Inorm64(remote_player.last_defend_result).to_f32() * 100.0))
    }
}