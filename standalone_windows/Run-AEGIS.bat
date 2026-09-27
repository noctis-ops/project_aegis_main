@echo off
TITLE Project AEGIS - Standalone Version
COLOR 0A

echo ================================
echo    Project AEGIS - Windows Edition
echo ================================
echo.
echo Checking system requirements...
echo.

REM Check if Node.js is installed
node --version >nul 2>&1
if %errorlevel% neq 0 (
    echo ⚠️  Node.js not found. Attempting to download...
    echo Please visit https://nodejs.org/ to download and install Node.js
    echo Then run this script again.
    echo.
    echo Press any key to open the download page...
    pause >nul
    start https://nodejs.org/
    exit /b 1
)

echo ✅ Node.js is installed

REM Check if required files exist
if not exist "bin\package.json" (
    echo ❌ ERROR: Missing package.json file!
    echo Please extract all files correctly.
    echo.
    pause
    exit /b 1
)

echo ✅ All required files found

REM Install dependencies if node_modules doesn't exist
if not exist "bin\node_modules" (
    echo 📦 Installing required dependencies (this may take 1-2 minutes)...
    cd bin
    npm install --silent
    if %errorlevel% neq 0 (
        echo ❌ Failed to install dependencies!
        echo Please check your internet connection and try again.
        cd ..
        pause
        exit /b 1
    )
    cd ..
    echo ✅ Dependencies installed successfully!
    echo.
)

echo 📁 Creating required directories...
REM Create data directory if it doesn't exist
if not exist "data" (
    mkdir data
)

REM Create logs directory if it doesn't exist
if not exist "logs" (
    mkdir logs
)

REM Backup current settings
if exist "config\settings.json" (
    copy "config\settings.json" "backup\settings.backup" >nul
    echo ✅ Settings backed up
)

echo 🚀 Starting Project AEGIS server...
echo.
echo ========================================
echo 🌐 System will be available at: http://localhost:3000
echo 📱 Mobile access: http://YOUR_IP_ADDRESS:3000
echo ⚠️  Press CTRL+C to stop the system
echo ========================================
echo.

cd bin
node server.js

echo.
echo 🛑 Project AEGIS has been stopped.
echo Thank you for using Project AEGIS!
pause