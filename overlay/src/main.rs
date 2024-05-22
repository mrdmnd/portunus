mod capturer;

use bevy::prelude::*;
use bevy::sprite::{MaterialMesh2dBundle, Mesh2dHandle};
use bevy::window::{Cursor, WindowLevel, WindowTheme};

fn main() {
    capturer::start_capture();

    let window = Window {
        transparent: true,
        title: "Portunus".into(),
        name: Some("portunus.app".into()),
        mode: bevy::window::WindowMode::BorderlessFullscreen,
        resolution: (3440., 1440.).into(),
        decorations: false,
        window_level: WindowLevel::AlwaysOnTop,
        cursor: Cursor {
            hit_test: false,
            ..default()
        },
        focused: true,
        window_theme: Some(WindowTheme::Dark),
        composite_alpha_mode: bevy::window::CompositeAlphaMode::Auto,
        ..default()
    };

    App::new()
        .insert_resource(ClearColor(Color::NONE))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(window),
            ..default()
        }))
        .insert_resource(Msaa::Sample4)
        .add_systems(Startup, startup)
        .add_systems(PostStartup, spawn_red_circle)
        .run()
}

fn startup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn spawn_red_circle(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let circle = Mesh2dHandle(meshes.add(Circle { radius: 50.0 }));
    let color = Color::rgba(1.0, 0.0, 0.0, 0.5);
    commands.spawn(MaterialMesh2dBundle {
        mesh: circle,
        material: materials.add(color),
        transform: Transform {
            translation: Vec3::new(0.0, -100.0, 0.0),
            ..Default::default()
        },
        ..default()
    });
}
