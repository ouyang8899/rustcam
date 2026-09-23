@echo off
setlocal

set QT_DIR=E:\qt\5.15.2
set QT_VERSION=5.15.2
set GENERATOR="Visual Studio 16 2019"

if not exist "%QT_DIR%" (
    echo Error: Qt not found at %QT_DIR%
    exit /b 1
)

echo ========================================
echo Generating VS2019 project for rustcam-client
echo Qt: %QT_DIR%
echo ========================================

cmake -B build-vs2019 ^
    -G %GENERATOR% ^
    -A x64 ^
    -DCMAKE_PREFIX_PATH="%QT_DIR%\msvc2019_64" ^
    -DQt5_DIR="%QT_DIR%\msvc2019_64\lib\cmake\Qt5" ^
    -DQT_VERSION_STRING=%QT_VERSION% ^
    -DCMAKE_BUILD_TYPE=Release ^
    -Wno-author

if %ERRORLEVEL% NEQ 0 (
    echo CMake configuration failed!
    exit /b 1
)

echo ========================================
echo VS2019 project generated successfully
echo Open: build-vs2019\rustcam-client.sln
echo ========================================

echo.
echo To run the application, use:
echo   run-dev.bat
echo.

endlocal
