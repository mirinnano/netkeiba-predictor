@echo off
rem Register a Windows scheduled task that runs update_all.bat every Tuesday at 06:00.
rem The task runs only while you are logged on (JV-Link may need your session).
rem Remove it later with:  schtasks /delete /tn "JvDump weekly update" /f
setlocal
schtasks /create /tn "JvDump weekly update" /sc weekly /d TUE /st 06:00 /tr "\"%~dp0update_all.bat\"" /f
if errorlevel 1 exit /b 1
echo Registered. Check it in Task Scheduler under "JvDump weekly update".
