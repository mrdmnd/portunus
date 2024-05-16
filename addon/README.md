# Addon

This addon exports gamestate by writing it to a sequence of pixels. Our overlay client then reads the pixels, decodes the game state, and performs the appropriate operations.

For development, you can fix up the constants set in symlink.bat to match your installation, then run symlink.bat directly to link the dev files to your game client.
For development, you want to install the flatbuffers compiler `flatc` and ensure it's on your path somewhere. You'll be able to compile the schema definition if you make changes.
To install `flatc` I ran (on windows, it'll look different on linux) `cmake -B build/` and then `cmake --build build/`
The invocation is `flatc --lua gamestate.fbs`