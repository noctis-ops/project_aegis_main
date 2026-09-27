# Project AEGIS Installation Options

## 🚀 Option 1: FULL Development Installation (Complex)
**Requirements:** 9GB+ disk space, Visual Studio, Rust compiler
**Purpose:** Development, modification, compilation
**Time:** 2-3 hours setup

## 🎯 Option 2: MINIMAL Runtime Installation (Simple)
**Requirements:** 500MB disk space, Windows 10/11
**Purpose:** Run pre-compiled system
**Time:** 15 minutes setup

## 📱 Option 3: PORTABLE Version (Simplest)
**Requirements:** None (standalone EXE)
**Purpose:** Quick demo/testing
**Time:** 2 minutes setup

---

## 🎯 Recommended: Minimal Runtime Installation

### Step 1: Download Pre-compiled Binaries
I'll provide ZIP file containing:
- `aegis-frontend.exe` (Web interface - 50MB)
- `aegis-layer1.exe` (Data engine - 15MB)
- `aegis-layer2.exe` (Alpha engine - 12MB)
- `aegis-layer3.exe` (Execution engine - 14MB)
- `aegis-layer4.exe` (Risk management - 11MB)
- `aegis-layer5.exe` (Simulation engine - 16MB)
- `postgresql-lite` (Database - 30MB)
- **TOTAL:** ~150MB

### Step 2: Simple Installation
1. Extract ZIP to `C:\ProjectAEGIS`
2. Run `install-minimal.bat`
3. Start system with `start-aegis.bat`
4. Access via http://localhost:3000

### Step 3: Configuration (2 minutes)
Edit `config.ini`:
```ini
[database]
host=localhost
port=5432
username=aegis
password=secure123

[binance]
api_key=your_key_here
api_secret=your_secret_here

[telegram]
bot_token=your_bot_token
authorized_users=123456789
```

---

## 📱 Portable Demo Version

### Single Executable Approach
- One file: `ProjectAEGIS-Demo.exe` (100MB)
- No installation required
- Run directly
- Limited features (demo mode)
- Perfect for testing

---

## 🏗️ Which Option Do You Prefer?

1. **[ ] Full Development Setup** (Complete system, can modify code)
2. **[ ] Minimal Runtime Setup** (Pre-compiled, ready to run)  
3. **[ ] Portable Demo** (Single EXE, quick test)

Please let me know which option you'd prefer and I'll prepare the appropriate files!