# Addon

This addon exports gamestate by writing it to a sequence of pixels. Our overlay client then reads the pixels, decodes the game state, and performs the appropriate operations.

Data is transferred with MessagePack (dependency included from https://github.com/markstinson/lua-MessagePack/blob/master/src/MessagePack.lua)

NOTA BENE: this addon *requires* that you are running a pixel-perfect UI.
To do so, you need to set your UI scale so that your screen height matches with the UI coordinates.
In my case, I play on ultrawide 3440 x 1440p. The default y-resolution for the game is 768, so the correct UI scale is 768 / 1440 = 0.53333333333. Becaues the game limits the settings on the "UIScale" cvar to things above 0.64, we need to set the UIParent directly.

Go download this addon: https://www.curseforge.com/wow/addons/uiscale
Set the scale appropriately.

TODO: take a look at this protobuf impl: https://www.wowace.com/projects/protobuf
https://github.com/tg123/lua-pb