@echo off

set "BUILD_DIR=%~1"
if "%BUILD_DIR%"=="" set "BUILD_DIR=%~dp0bin\Debug"

set "QT_DIR=E:\qt\5.15.2\msvc2019_64"
set "THIRD_DIR=%~dp03rd_party"

echo Deploying rustcam-client dependencies to %BUILD_DIR%...
echo 3rd_party source: %THIRD_DIR%

REM Create destination directories
if not exist "%BUILD_DIR%" mkdir "%BUILD_DIR%"
if not exist "%BUILD_DIR%\platforms" mkdir "%BUILD_DIR%\platforms"
if not exist "%BUILD_DIR%\styles" mkdir "%BUILD_DIR%\styles"
if not exist "%BUILD_DIR%\iconengines" mkdir "%BUILD_DIR%\iconengines"
if not exist "%BUILD_DIR%\imageformats" mkdir "%BUILD_DIR%\imageformats"

REM Create 3rd party source directories if needed
if not exist "%THIRD_DIR%" (
    echo 3rd_party directory not found: %THIRD_DIR%
    goto :end
)

REM Copy resources
echo Copying resources...
if not exist "%BUILD_DIR%\resources" mkdir "%BUILD_DIR%\resources"
if not exist "%BUILD_DIR%\resources\qss" mkdir "%BUILD_DIR%\resources\qss"
if not exist "%BUILD_DIR%\resources\icons" mkdir "%BUILD_DIR%\resources\icons"
if not exist "%BUILD_DIR%\resources\fonts" mkdir "%BUILD_DIR%\resources\fonts"
xcopy /Y /Q "%~dp0resources\qss\*.*" "%BUILD_DIR%\resources\qss\" >nul
xcopy /Y /Q /I "%~dp0resources\icons" "%BUILD_DIR%\resources\icons\" >nul
xcopy /Y /Q /I "%~dp0resources\fonts" "%BUILD_DIR%\resources\fonts\" >nul
if not exist "%BUILD_DIR%\rustcam_plugins" mkdir "%BUILD_DIR%\rustcam_plugins"

REM Copy Qt DLLs (all)
echo Copying Qt DLLs...
for %%F in ("%QT_DIR%\bin\*.dll") do copy /Y "%%F" "%BUILD_DIR%\" >nul

REM Copy Qt plugins (both release and debug)
echo Copying Qt plugins...
copy /Y "%QT_DIR%\plugins\platforms\qwindows.dll" "%BUILD_DIR%\platforms\" >nul
copy /Y "%QT_DIR%\plugins\platforms\qwindowsd.dll" "%BUILD_DIR%\platforms\" >nul
copy /Y "%QT_DIR%\plugins\platforms\qminimal.dll" "%BUILD_DIR%\platforms\" >nul
copy /Y "%QT_DIR%\plugins\platforms\qminimald.dll" "%BUILD_DIR%\platforms\" >nul
copy /Y "%QT_DIR%\plugins\platforms\qoffscreen.dll" "%BUILD_DIR%\platforms\" >nul
copy /Y "%QT_DIR%\plugins\platforms\qoffscreend.dll" "%BUILD_DIR%\platforms\" >nul
copy /Y "%QT_DIR%\plugins\styles\qwindowsvistastyle.dll" "%BUILD_DIR%\styles\" >nul
copy /Y "%QT_DIR%\plugins\styles\qwindowsvistastyled.dll" "%BUILD_DIR%\styles\" >nul
copy /Y "%QT_DIR%\plugins\iconengines\qsvgicon.dll" "%BUILD_DIR%\iconengines\" >nul
copy /Y "%QT_DIR%\plugins\iconengines\qsvgicond.dll" "%BUILD_DIR%\iconengines\" >nul
for %%F in ("%QT_DIR%\plugins\imageformats\*.dll") do copy /Y "%%F" "%BUILD_DIR%\imageformats\" >nul
for %%F in ("%QT_DIR%\plugins\imageformats\*d.dll") do copy /Y "%%F" "%BUILD_DIR%\imageformats\" >nul

REM Copy 3rd party libraries
echo Copying 3rd party libraries...
copy /Y "%THIRD_DIR%\CURL\bin\libcurl.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\CURL\bin\libcrypto-4-x64.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\CURL\bin\libssl-4-x64.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\CURL\bin\libzstd.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\zlib\bin\zlib.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\SQLITE3\bin\sqlite3.dll" "%BUILD_DIR%\" >nul

REM Copy Boost DLLs (x64 release)
copy /Y "%THIRD_DIR%\boost\lib\boost_chrono-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\boost\lib\boost_date_time-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\boost\lib\boost_filesystem-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\boost\lib\boost_json-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\boost\lib\boost_program_options-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\boost\lib\boost_regex-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\boost\lib\boost_thread-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\boost\lib\boost_log-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul
copy /Y "%THIRD_DIR%\boost\lib\boost_log_setup-vc142-mt-x64-1_91.dll" "%BUILD_DIR%\" >nul

REM Copy MSVC runtime
echo Copying MSVC runtime...
copy /Y "C:\Windows\System32\vcruntime140.dll" "%BUILD_DIR%\" >nul
copy /Y "C:\Windows\System32\vcruntime140_1.dll" "%BUILD_DIR%\" >nul
copy /Y "C:\Windows\System32\msvcp140.dll" "%BUILD_DIR%\" >nul
copy /Y "C:\Windows\System32\msvcp140_1.dll" "%BUILD_DIR%\" >nul
copy /Y "C:\Windows\System32\msvcp140_2.dll" "%BUILD_DIR%\" >nul

echo Deployment complete!
goto :eof

:end
pause
