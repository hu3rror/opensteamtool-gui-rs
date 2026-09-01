@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 (echo VCVARS_FAILED& exit /b 1)
cargo check 2>&1
echo CARGO_EXIT=%ERRORLEVEL%
