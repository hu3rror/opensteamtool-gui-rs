@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe" modify --installPath "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools" --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --quiet --norestart
echo SETUP_EXIT=%ERRORLEVEL%
