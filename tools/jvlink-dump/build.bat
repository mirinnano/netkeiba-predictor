@echo off
rem Build JvDump.exe as a 32-bit (x86) program with the C# compiler that ships with Windows.
rem JV-Link is a 32-bit COM component, so the program must be x86.
setlocal
set CSC=%WINDIR%\Microsoft.NET\Framework\v4.0.30319\csc.exe
if not exist "%CSC%" (
  echo csc.exe not found: %CSC%
  echo Enable .NET Framework 4.x in "Turn Windows features on or off".
  exit /b 1
)
"%CSC%" /nologo /codepage:65001 /platform:x86 /optimize+ /out:JvDump.exe /r:Microsoft.CSharp.dll /r:System.Core.dll JvDump.cs
if errorlevel 1 exit /b 1
echo Built JvDump.exe
