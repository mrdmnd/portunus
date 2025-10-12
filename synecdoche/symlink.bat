REM Modify the two vars so it match you own setup. Make sure you have it surrounded by double quotes
set WowRoot="E:\World of Warcraft"
set WowVersion="_retail_"
set InstallRoot="C:\Users\mttrd\portunus\synecdoche"

REM Clear existing directories.
rmdir /s /q %WowRoot%"\"%WowVersion%"\Interface\Addons\Synecdoche"
:warcraft
REM Making symlinks.
mklink /J %WowRoot%"\"%WowVersion%"\Interface\AddOns\Synecdoche" %InstallRoot%

pause
