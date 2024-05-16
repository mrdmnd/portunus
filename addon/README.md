# Addon

This addon exports gamestate by writing it to a sequence of pixels. Our overlay client then reads the pixels, decodes the game state, and performs the appropriate operations.
Data is transferred with MessagePack (dependency included from https://github.com/markstinson/lua-MessagePack/blob/master/src/MessagePack.lua)