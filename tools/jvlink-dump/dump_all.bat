@echo off
rem Dump the data sets used for backtesting into .\out (raw JV-Data records, Shift_JIS).
rem Option 4 = setup without dialog. First run downloads a lot; it can take hours.
rem Re-running continues from out\<spec>\lastfiletimestamp.txt is NOT automatic; see README.
setlocal
set FROM=20210101000000
if not "%1"=="" set FROM=%1
for %%S in (RACE SLOP WOOD DIFN BLOD) do (
  echo ===== %%S from %FROM%
  JvDump.exe open %%S %FROM% 4 out
  if errorlevel 1 echo %%S failed, continuing with the next one
)
echo Done. Copy the "out" folder to the Mac.
