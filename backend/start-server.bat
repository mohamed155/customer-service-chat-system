@echo off
call "%~dp0.env-loader.bat" || goto :error
"%~dp0target\release\server.exe"
exit /b %errorlevel%
:error
echo Failed to load env
exit /b 1
