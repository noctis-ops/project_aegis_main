# 🎯 Project AEGIS - Standalone Windows Version

## 🚀 Super Simple Installation (Ready in 5 Minutes!)

### What You Get:
- **Single ZIP file** (15MB total)
- **No complex installation** required
- **Runs directly on any Windows 10/11**
- **Complete system in one click**

## 📦 Package Contents:

### 1. Executables & Scripts:
- `Run-AEGIS.bat` - Double-click to start everything
- `INSTALL-INSTRUCTIONS.txt` - Quick setup guide

### 2. Application Files:
- `bin/` - Core application files
- `public/` - Web interface (English & Arabic)
- `config/` - Settings files
- `data/` - Database files (auto-created)
- `logs/` - System logs (auto-created)
- `backup/` - Settings backup (auto-created)

## 🎯 Installation Steps (3 Simple Steps):

### Step 1: Download & Extract
1. Download `ProjectAEGIS-Standalone-Windows.zip` (15MB)
2. Extract to any folder (e.g., `C:\AEGIS` or `Desktop\AEGIS`)

### Step 2: Run the System
1. **Double-click** `Run-AEGIS.bat`
2. Wait 30-60 seconds for first-time setup
3. Browser opens automatically at: http://localhost:3000

### Step 3: Start Using
1. Click "Start HFT Engine" in the web dashboard
2. Monitor real-time performance metrics
3. Switch to Arabic: http://localhost:3000/ar

## 🎮 Features Included:

### ✅ Web Dashboard (Arabic & English)
- Real-time system monitoring
- Trading engine controls
- Performance metrics display
- Risk management panel

### ✅ Core Trading Engine
- Market data simulation
- Signal generation demo
- Risk management demo
- Telegram notifications (optional)

### ✅ Demo Mode (Default - Safe!)
- **No real money risk**
- Simulated market data
- Full system functionality
- Perfect for learning/testing

### ✅ Telegram Integration (Optional)
- Remote control via Telegram
- Real-time alerts
- System status updates

## ⚡ Minimal System Requirements:

### Absolutely Minimal:
- **Windows 10/11** (64-bit) ✅
- **2GB RAM** ✅
- **100MB free disk space** ✅
- **Internet connection** (first run only) ✅

### Recommended:
- Windows 10/11 (64-bit)
- 4GB RAM
- 500MB free disk space

## 🔧 Simple Configuration:

### Language Settings:
Edit `config/settings.json`:
```json
{
  "app": {
    "language": "english"  // or "arabic"
  }
}
```

### Demo vs Live Mode:
```json
{
  "trading": {
    "demoMode": true  // SAFE - No real trading
    // "demoMode": false  // Real trading (requires API keys)
  }
}
```

## 🎯 Quick Start Guide:

### 1. First Time Running:
1. Extract ZIP file anywhere
2. Double-click `Run-AEGIS.bat`
3. Wait for "Server running on port 3000"
4. Browser opens automatically

### 2. Daily Usage:
1. Double-click `Run-AEGIS.bat`
2. System starts in 5 seconds
3. Access via http://localhost:3000

### 3. Basic Operations:
- **Start Trading:** Click "Start HFT Engine"
- **Stop Trading:** Click "Stop HFT Engine"
- **View Status:** Dashboard shows live metrics
- **Switch Language:** Click language button

## 🛡️ Safety Features:

### Demo Mode Protection:
- **Default setting:** `demoMode: true`
- **No real trading** ever
- **Fake market data** for testing
- **Safe to experiment** anytime

### API Key Protection:
- Keys stored encrypted locally
- Never transmitted over network
- Only used for real trading (when enabled)

### System Isolation:
- Runs completely locally
- No external dependencies after setup
- Self-contained database
- No cloud requirements

## 📱 Mobile Access:

### From Any Device on Same Network:
1. Find your computer's IP address:
   - Press `Windows+R`
   - Type `cmd` and press Enter
   - Type `ipconfig`
   - Find "IPv4 Address" (e.g., 192.168.1.100)

2. Access from mobile/tablet:
   - Open browser
   - Go to: `http://192.168.1.100:3000`

## 🆘 Troubleshooting:

### Common Issues & Solutions:

#### ❌ Issue: "Node.js not found" error
**Solution:** 
1. Download Node.js from https://nodejs.org/
2. Install the LTS version
3. Restart your computer
4. Try again

#### ❌ Issue: Port 3000 already in use
**Solution:**
1. Edit `config/settings.json`
2. Change `"port": 3000` to `"port": 3001`
3. Save and restart

#### ❌ Issue: Slow performance
**Solution:**
1. Close other programs
2. Ensure 2GB+ RAM available
3. Restart computer if needed

### System Logs:
Automatic logs saved to `logs/` folder:
- `system.log` - Main system events
- `errors.log` - Error messages
- `trading.log` - Trading activity

## 🔄 Updates:

### Manual Update:
1. Download new ZIP file
2. Extract to same folder
3. Choose "Replace files" when prompted
4. Restart system

### Automatic Backup:
System automatically backs up settings:
- Backup saved to `backup/settings.backup`
- Restored if `settings.json` corrupted

## 🎁 What You're Getting:

### Complete Professional System:
- **Real-time trading engine** (demo mode)
- **Advanced risk management** (simulated)
- **Professional dashboard** (Arabic & English)
- **Mobile remote control** (Telegram optional)
- **Enterprise-grade security** (local only)

### All in One Small Package:
- **Total size:** 15MB
- **Installation time:** 5 minutes
- **Learning curve:** 30 minutes
- **Full functionality:** Immediate

## 📞 Support:

### Need Help?
1. **Read:** `INSTALL-INSTRUCTIONS.txt`
2. **Check:** System logs in `logs/` folder
3. **Email:** support@project-aegis.com
4. **GitHub:** https://github.com/your-repo/project-aegis

### Report Issues:
1. Describe the problem clearly
2. Include system specs (Windows version, RAM)
3. Attach relevant log files
4. Screenshots help a lot!

## 🎯 Ready to Get Started?

### Download Includes:
1. **ProjectAEGIS-Standalone-Windows.zip** (15MB)
2. **Quick start guide** (this document)
3. **Video tutorial link** (coming soon)

This standalone version gives you the full power of Project AEGIS with:
- **ZERO complex installation**
- **MINIMAL system requirements** 
- **MAXIMUM ease of use**
- **COMPLETE functionality**

Perfect for learning, testing, and demonstrating the system!

---
**Note:** This is a simplified educational version. The full professional system requires the complete development environment as described in the main documentation.