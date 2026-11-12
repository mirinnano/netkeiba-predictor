@echo off
rem Fetch only what is new since the last run, for every data set dumped before.
rem Run dump_all.bat once first. Results are appended to out\<spec>\*.txt
rem and each run is recorded in out\update_log.txt.
setlocal
cd /d "%~dp0"
for %%S in (RACE SLOP WOOD DIFN BLOD) do (
  if exist "out\%%S\lastfiletimestamp.txt" (
    echo ===== %%S
    JvDump.exe update %%S out
    if errorlevel 1 echo %%S failed, continuing with the next one
  ) else (
    echo ===== %%S skipped: no previous dump
  )
)
echo Done.
