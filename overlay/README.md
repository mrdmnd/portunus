# Overlay
This is the core entry point to Portunus. The overlay will connect to the wow client's LUA addon data exporter and read the game state, then feed the game state into the inference engine for simulation and NN evaluation. 
Finally, the MCTS tree is visualized on screen with summary statistics.


# Repos that do overlay functionality stuff to reserach
- https://github.com/coderedart/egui_overlay
- https://github.com/imgui-rs/imgui-rs
- https://github.com/emilk/egui/issues/1677
- https://github.com/hasenbanck/egui_example/blob/master/src/main.rs
- https://github.com/SnowflakePowered/snowflake-ingame
- https://github.com/rmccrystal/window-overlay
- https://github.com/veeenu/hudhook
- https://github.com/zorftw/win-overlay/tree/master

- https://www.reddit.com/r/rust/comments/uun02v/which_library_should_i_use_for_creating_an_overlay/
- https://whoisryosuke.com/blog/2023/getting-started-with-egui-in-rust#drawing-shapes
