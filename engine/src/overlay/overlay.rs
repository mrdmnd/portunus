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
