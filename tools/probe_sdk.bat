@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
echo LIB=%LIB%
echo ===SDK===
dir "C:\Program Files (x86)\Windows Kits\10\Lib" 2>nul
