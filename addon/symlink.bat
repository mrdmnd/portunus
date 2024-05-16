REM Modify the two vars so it match you own setup. Make sure you have it surrounded by double quotes
REM WoWRep  : World of warcraft main directory
REM GHRep   : Where your the addon folder are stored
set WowRoot="D:\World of Warcraft"
set WowVersion="_retail_"
set InstallRoot="C:\Users\mttrd\portunus\addon"

REM Clear existing directories.
rmdir /s /q %WowRoot%"\"%WowVersion%"\Interface\Addons\PortunusExporter"
:warcraft
REM Making symlinks.
mklink /J %WowRoot%"\"%WowVersion%"\Interface\AddOns\PortunusExporter" %InstallRoot%

pause
