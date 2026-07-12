@echo off
for /f "usebackq tokens=1,2 delims==" %%a in ("%~dp0.env") do (
    set "%%a=%%b"
)
